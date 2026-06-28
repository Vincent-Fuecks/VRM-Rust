use std::collections::HashMap;

use lazy_static::lazy_static;

use crate::vrm::vrm_component::vrm_component_order::VrmComponentOrder;
use crate::vrm::common::config::TRY_N_PROMOTIONS;
use crate::vrm::common::id::{ComponentId, ShadowScheduleId};
use crate::vrm::reservation::probe_reservations::{ProbeReservationComparator, ProbeReservations};
use crate::vrm::reservation::reservation::ReservationState;
use crate::vrm::reservation::reservation_store::ReservationId;

use super::VrmComponentManager;

// In the case where dummy dependencies where scheduled, a dummy VrmComponentId is utilized.
// This happens in the cases, where network transfers can be skipped, as both endpoints are on the same node.
lazy_static! {
    pub static ref DUMMY_COMPONENT_ID: ComponentId = ComponentId::new("ADC INTERNAL JOB");
}

impl VrmComponentManager {
    pub fn probe(
        &mut self,
        component_id: ComponentId,
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
    ) -> ProbeReservations {
        match self.vrm_components.get_mut(&component_id) {
            Some(container) => container.vrm_component.probe(reservation_id, shadow_schedule_id),
            None => {
                log::error!(
                    "ComponentManagerHasNotFoundGridComponent: ComponentManager of ADC {}, requested component {} for probe request of reservation {:?} on shadow_schedule {:?}",
                    self.adc_id,
                    component_id,
                    reservation_id,
                    shadow_schedule_id
                );

                return ProbeReservations::new(reservation_id, self.reservation_store.clone());
            }
        }
    }

    pub fn probe_all_components(&mut self, reservation_id: ReservationId) -> ProbeReservations {
        let mut probe_results = ProbeReservations::new(reservation_id, self.reservation_store.clone());

        for (_, container) in &mut self.vrm_components {
            let res_snapshot = self.reservation_store.get_reservation_snapshot(reservation_id).unwrap();

            if container.can_handel(res_snapshot) {
                let probe_reservations = container.vrm_component.probe(reservation_id, None);

                probe_results.add_probe_reservations(probe_reservations);
            }
        }

        if probe_results.is_empty() {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
        }

        return probe_results;
    }

    pub fn reserve(
        &mut self,
        component_id: ComponentId,
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
    ) -> ReservationId {
        match self.vrm_components.get_mut(&component_id) {
            Some(container) => {
                container.vrm_component.reserve(reservation_id, shadow_schedule_id);

                if self.reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::ReserveAnswer) {
                    self.not_committed_reservations.insert(reservation_id, component_id.clone());
                }
                println!("{:?}", component_id.clone());
                self.res_to_vrm_component.insert(reservation_id, component_id);

                return reservation_id;
            }
            None => {
                log::error!(
                    "ComponentManagerHasNotFoundGridComponent: ComponentManager of ADC {}, requested component {} for reserve request of reservation {:?} on shadow_schedule {:?}",
                    self.adc_id,
                    component_id,
                    reservation_id,
                    shadow_schedule_id
                );

                return reservation_id;
            }
        }
    }

    pub fn reserve_without_check(&mut self, component_id: ComponentId, reservation_id: ReservationId) {
        match self.vrm_components.get_mut(&component_id) {
            Some(container) => container.schedule.reserve_without_check(reservation_id),
            None => {
                log::error!(
                    "ComponentManagerHasNotFoundGridComponent: ComponentManager of ADC {}, requested component {} for reserve_without_check request of reservation {:?} on schedule",
                    self.adc_id,
                    component_id,
                    reservation_id,
                );
            }
        }
    }

    // Handles only single reservation and no child reservations (deletes also workflow reservation but not the related children)
    pub fn delete_reservation(&mut self, reservation_id: &ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId {
        match self.res_to_vrm_component.get(reservation_id) {
            Some(component_id) => {
                // No real reservation
                if component_id.compare(&DUMMY_COMPONENT_ID) {
                    self.reservation_store.update_state(*reservation_id, ReservationState::Deleted);
                    return *reservation_id;
                }
                // Del Reservation form VrmComponent and update Local schedule view
                if let Some(container) = self.vrm_components.get_mut(component_id) {
                    container.vrm_component.delete(reservation_id.clone(), shadow_schedule_id);
                    container.schedule.delete_reservation(reservation_id.clone());
                } else {
                    log::error!(
                        "ComponentManagerHasNotFoundVrmComponentWhereReservationIsLocated: ComponentManager of ADC {}, requested to delete the reservation {:?} on shadow schedule {:?} on VrmComponent {}. ",
                        self.adc_id,
                        self.reservation_store.get_name_for_key(reservation_id.clone()),
                        shadow_schedule_id,
                        component_id,
                    );
                }
                return *reservation_id;
            }
            None => {
                log::error!(
                    "ComponentManagerHasNotFoundVrmComponentForReservationToDelete: ComponentManager of ADC {}, requested to delete the reservation {:?} on shadow schedule {:?}. ",
                    self.adc_id,
                    self.reservation_store.get_name_for_key(reservation_id.clone()),
                    shadow_schedule_id
                );
                return *reservation_id;
            }
        }
    }

    pub fn delete_task_at_component(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> bool {
        let mut target_component = None;

        if let Some(sid) = &shadow_schedule_id {
            if let Some((shadow_map, _)) = self.shadow_schedule_reservations.get(&sid) {
                target_component = shadow_map.get(&reservation_id).cloned();
            }
        } else {
            target_component = self.res_to_vrm_component.get(&reservation_id).cloned();
        }

        match target_component {
            Some(component_id) => {
                // No Real Task
                if component_id == *DUMMY_COMPONENT_ID {
                    if let Some(sid) = &shadow_schedule_id {
                        if let Some((_, store)) = self.shadow_schedule_reservations.get(&sid) {
                            store.update_state(reservation_id, ReservationState::Deleted);
                        }
                    } else {
                        self.reservation_store.update_state(reservation_id, ReservationState::Deleted);
                    }
                    return true;
                }

                let container = self.get_vrm_component_container_mut(component_id.clone());

                container.vrm_component.delete(reservation_id, shadow_schedule_id.clone());

                // Note: We check the store to verify deletion.
                // If shadow, we check the shadow store
                let is_deleted = if let Some(sid) = &shadow_schedule_id {
                    // Check shadow store state
                    if let Some((_, store)) = self.shadow_schedule_reservations.get(&sid) {
                        store.get_state(reservation_id) == ReservationState::Deleted
                    } else {
                        false
                    }
                } else {
                    self.reservation_store.get_state(reservation_id) == ReservationState::Deleted
                };

                if is_deleted {
                    // Update Local view
                    let container = self.get_vrm_component_container_mut(component_id);
                    container.schedule.delete_reservation(reservation_id);

                    // Cleanup Mapping
                    if let Some(sid) = &shadow_schedule_id {
                        if let Some((shadow_map, _)) = self.shadow_schedule_reservations.get_mut(&sid) {
                            shadow_map.remove(&reservation_id);
                        }
                    } else {
                        self.res_to_vrm_component.remove(&reservation_id);
                    }

                    return true;
                }

                self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                return false;
            }
            None => {
                log::error!(
                    "ReservationForDeletionWasNotFound: In ADC {} ShadowSchedule {:?} was Reservation {:?} not found.",
                    self.adc_id,
                    shadow_schedule_id,
                    self.reservation_store.get_name_for_key(reservation_id)
                );
                return false;
            }
        }
    }

    /// Performs the commit operation at the specific underlying component.
    ///
    /// This is used internally for both atomic tasks and sub-tasks within a workflow.
    /// If the component is a dummy/internal component, the state is updated locally.
    /// Returns `true` if the component successfully committed the reservation.
    pub fn commit_at_component(&mut self, reservation_id: ReservationId, component_id: ComponentId) -> bool {
        // Is dummy task/ "Internal task"
        if component_id == *DUMMY_COMPONENT_ID {
            self.reservation_store.update_state(reservation_id, ReservationState::Committed);
            return true;
        }

        let container = self.get_vrm_component_container_mut(component_id.clone());
        if container.vrm_component.commit(reservation_id) {
            self.update_commit_tracking(reservation_id, component_id);
            return true;
        }

        // If commit fails, clean up local schedule and global mapping
        container.schedule.delete_reservation(reservation_id);
        self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
        return false;
    }

    /// Transitions all committed reservations into state `ReservationState::Rejected` state following a scheduling or resource failure.
    pub fn handle_commit_failure(&mut self, clean_vrm_of_res_ids: Vec<ReservationId>) {
        for reservation_id in &clean_vrm_of_res_ids {
            self.reservation_store.update_state(*reservation_id, ReservationState::Rejected);
            if !self.delete_task_at_component(*reservation_id, None) {
                panic!("Deletion of Committed task failed.");
            }
        }
    }

    /// Submits a task to the first VrmComponent that accepts the reservation based on the defined `VrmComponentOrder`.
    pub fn reserve_task_at_first_grid_component(
        &mut self,
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        vrm_component_order: VrmComponentOrder,
    ) -> ReservationId {
        // Wrong order
        for component_id in self.get_ordered_vrm_components(vrm_component_order) {
            let res_snapshot = self.reservation_store.get_reservation_snapshot(reservation_id).unwrap();

            if self.can_component_handel(component_id.clone(), res_snapshot) {
                let reserve_res_id = self.reserve(component_id.clone(), reservation_id, shadow_schedule_id.clone());

                let is_reserved = if let Some(sid) = &shadow_schedule_id {
                    if let Some((_, store)) = self.shadow_schedule_reservations.get(sid) {
                        store.is_reservation_state_at_least(reserve_res_id, ReservationState::ReserveAnswer)
                    } else {
                        false
                    }
                } else {
                    self.reservation_store.is_reservation_state_at_least(reserve_res_id, ReservationState::ReserveAnswer)
                };

                if is_reserved {
                    self.update_reserve_tracking(reserve_res_id, component_id.clone(), shadow_schedule_id);

                    // Update VrmComponent's local view (schedule) of the underlying VrmComponents
                    self.reserve_without_check(component_id.clone(), reserve_res_id);
                    return reserve_res_id;
                }
            }
        }

        // Update failure state in appropriate store
        if let Some(sid) = &shadow_schedule_id {
            if let Some((_, store)) = self.shadow_schedule_reservations.get(sid) {
                store.update_state(reservation_id, ReservationState::Rejected);
            }
        } else {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
        }

        return reservation_id;
    }

    /// Probes all available VrmComponents and selects the best candidate based on the provided comparison function.
    ///
    /// This implements a "Best Fit" strategy, useful for optimizing resource utilization or
    /// meeting Earliest Finish Time (EFT) constraints.
    pub fn reserve_reservation_at_best_vrm_component(
        &mut self,
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        grid_component_res_database: &mut HashMap<ReservationId, ComponentId>,
        probe_reservation_comparator: ProbeReservationComparator,
    ) -> Option<ReservationId> {
        let mut probe_reservations = ProbeReservations::new(reservation_id, self.reservation_store.clone());

        let res_snapshot = match self.reservation_store.get_reservation_snapshot(reservation_id) {
            Some(snapshot) => snapshot,
            None => {
                log::error!("Cannot submit task: snapshot for {:?} not found.", reservation_id);
                return None;
            }
        };

        for component_id in self.get_random_ordered_vrm_components() {
            if self.can_component_handel(component_id.clone(), res_snapshot.clone()) {
                let probe_res = self.get_vrm_component_mut(component_id.clone()).probe(reservation_id, shadow_schedule_id.clone());

                probe_reservations.add_probe_reservations(probe_res);
            }
        }

        for _ in 0..TRY_N_PROMOTIONS {
            if let Some((component_id, shadow_schedule_id)) = probe_reservations.prompt_best(reservation_id, probe_reservation_comparator.clone()) {
                self.reserve(component_id.clone(), reservation_id, shadow_schedule_id);

                if self.reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::ReserveAnswer) {
                    log::info!("Reservation {:?} successful!", reservation_id);

                    // Update local schedule
                    self.reserve_without_check(component_id.clone(), reservation_id);

                    // Register new schedule Sub-Task
                    // Update grid_component_res_database for rollback and for ADC to keep track
                    // Update Transaction Log
                    if grid_component_res_database.contains_key(&reservation_id) {
                        log::error!(
                            "ErrorReservationWasReservedInMultipleGridComponents: The reservation {:?} was multiple times to the GirdComponent {} submitted.",
                            self.reservation_store.get_name_for_key(reservation_id),
                            component_id,
                        );
                    }

                    grid_component_res_database.insert(reservation_id, component_id);
                    return Some(reservation_id);
                }
            }
        }
        return None;
    }
}
