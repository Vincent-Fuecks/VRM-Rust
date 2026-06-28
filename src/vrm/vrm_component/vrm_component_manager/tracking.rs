use std::collections::HashMap;

use crate::vrm::reservation::reservation::ReservationState;
use crate::vrm::reservation::reservation_store::ReservationId;
use crate::vrm::common::id::{ComponentId, ShadowScheduleId};

use super::VrmComponentManager;

impl VrmComponentManager {
    // --- Tracking Methods ---
    /// Registers a mapping for a single reservation (Atomic Job).
    pub fn register_allocation(&mut self, reservation_id: ReservationId, component_id: ComponentId) -> Option<ComponentId> {
        self.res_to_vrm_component.insert(reservation_id, component_id)
    }

    /// Merges a "transaction map" (from a Workflow Scheduler) into the global state.
    pub fn register_workflow_subtasks(&mut self, workflow_id: ReservationId, allocations: &HashMap<ReservationId, ComponentId>) {
        let subtask_ids: Vec<ReservationId> = allocations.keys().cloned().collect();

        // 1. Merge the allocation map (Who has what)
        self.res_to_vrm_component.extend(allocations.clone());

        // 2. Track relationship: Parent -> Children
        self.workflow_subtasks.insert(workflow_id.clone(), subtask_ids.clone());

        // 3. Track relationship: Child -> Parent
        for subtask_id in subtask_ids.clone() {
            self.reverse_workflow_subtasks.insert(subtask_id, workflow_id.clone());
        }

        // Check if reserve of all workflow subtask worked correctly
        for res_id in &subtask_ids {
            if !self.reservation_store.is_reservation_state_at_least(*res_id, ReservationState::ReserveAnswer) {
                panic!(
                    "ErrorVrmComponentManagerWorkflowSubtaskIsNotReserved: The registration of workflow {:?} for ADC {} failed, because workflow subtask {:?} was not successfully reserved (ReservationState is < ReserveAnswer). This suggests that there is an error during the reserve operation of the WorkflowScheduler or the VrmComponent reservation process.",
                    self.reservation_store.get_name_for_key(workflow_id),
                    self.adc_id,
                    self.reservation_store.get_name_for_key(*res_id)
                );
            }

            if !self.not_committed_reservations.contains_key(res_id) {
                panic!(
                    "ErrorVrmComponentManagerWorkflowSubtaskWasNotAddedToNotCommittedReservations: The registration of workflow {:?} for ADC {} failed, because workflow subtask {:?} was not successfully added to the not_committed_reservations. This suggests that there is an error during the reserve operation of the WorkflowScheduler or the VrmComponent reservation process.",
                    self.reservation_store.get_name_for_key(workflow_id),
                    self.adc_id,
                    self.reservation_store.get_name_for_key(*res_id)
                );
            }
        }
    }

    /// Retrieves the ComponentId responsible for a specific reservation.
    pub fn get_handler_id(&self, reservation_id: ReservationId) -> Option<ComponentId> {
        self.res_to_vrm_component.get(&reservation_id).cloned()
    }

    /// Retrieves the Parent Workflow ID for a given subtask.
    pub fn get_parent_workflow(&self, subtask_id: ReservationId) -> Option<ReservationId> {
        self.reverse_workflow_subtasks.get(&subtask_id).cloned()
    }

    /// Removes tracking for a reservation.
    /// If it's a workflow, this might need to clean up children, or children cleanup calls this.
    /// Currently, this removes the specific ID from the allocation map.
    pub fn remove_allocation(&mut self, reservation_id: &ReservationId) -> Option<ComponentId> {
        // Remove from reverse lookup if it exists
        self.reverse_workflow_subtasks.remove(reservation_id);
        // Remove from allocation map
        self.res_to_vrm_component.remove(reservation_id)
    }

    /// Removes all tracking associated with a workflow (children and the workflow entry itself).
    pub fn remove_workflow_tracking(&mut self, workflow_id: &ReservationId) {
        if let Some(subtasks) = self.workflow_subtasks.remove(workflow_id) {
            for subtask in subtasks {
                self.res_to_vrm_component.remove(&subtask);
                self.reverse_workflow_subtasks.remove(&subtask);
            }
        }
    }

    pub fn update_commit_tracking(&mut self, reservation_id: ReservationId, component_id: ComponentId) {
        if !self.is_reservation_reserved(reservation_id) {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            log::error!(
                "ErrorInCommitPreProcess: Commit at Component {} of ADC {} failed for Reservation {:?}. There was no reserve at a 
                    VrmComponent for the reservation found. Should happen before.",
                component_id,
                self.adc_id,
                self.reservation_store.get_name_for_key(reservation_id)
            );
        }
        if self.committed_reservations.contains_key(&reservation_id) {
            log::error!(
                "ErrorInCommitPreProcess: Commit at Component {} of ADC {} failed for Reservation {:?}. The reservation was already committed to a VrmComponent",
                component_id,
                self.adc_id,
                self.reservation_store.get_name_for_key(reservation_id)
            );
        }

        self.not_committed_reservations.remove(&reservation_id);
        self.committed_reservations.insert(reservation_id, component_id);
    }

    pub fn update_reserve_tracking(
        &mut self,
        reservation_id: ReservationId,
        component_id: ComponentId,
        shadow_schedule_id: Option<ShadowScheduleId>,
    ) {
        if shadow_schedule_id.is_none() {
            let old_value = self.not_committed_reservations.insert(reservation_id, component_id.clone());

            if !old_value.is_none() {
                panic!(
                    "ErrorVrmManagerDuplicateReserveReservationInNotCommittedReservations: The tracking update of a reserved reservation of ADC {} failed. The Reservation {:?} was already reserved before on VrmComponent {}. The new reserve was performed for VrmComponent {}",
                    self.adc_id,
                    self.reservation_store.get_name_for_key(reservation_id),
                    old_value.unwrap(),
                    component_id
                );
            }

            if !self.reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::ReserveAnswer) {
                panic!(
                    "ErrorVrmManagerReservationStateIsNotAtLeastReserveAnswer: The tracking update of a reserved reservation of ADC {} failed. The Reservation {:?} reserved on VrmComponent {} must have at least ReservationState::ReservedAnswer, but as only state {:?}.",
                    self.adc_id,
                    self.reservation_store.get_name_for_key(reservation_id),
                    component_id,
                    self.reservation_store.get_state(reservation_id)
                );
            }

            let old_value = self.register_allocation(reservation_id, component_id.clone());

            if !old_value.is_none() {
                panic!(
                    "ErrorVrmManagerDuplicateReserveInResToVrmComponent: The tracking update of a reserved reservation of ADC {} failed. The Reservation {:?} was already reserved before on VrmComponent {}. The new reserve was performed for VrmComponent {}",
                    self.adc_id,
                    self.reservation_store.get_name_for_key(reservation_id),
                    old_value.unwrap(),
                    component_id
                );
            }
        } else {
            let (shadow_not_committed_reservations, shadow_reservation_store) =
                self.shadow_schedule_reservations.get_mut(&shadow_schedule_id.clone().unwrap()).expect("ErrorVrmManagerShadowScheduleWasNotFound");

            // In shadow mode, we often use `res_to_vrm_component` (shadow_map) to track everything.
            // `not_committed_reservations` is technically derived from state.
            // Here we update the map directly.

            let old_value = shadow_not_committed_reservations.insert(reservation_id, component_id.clone());

            if !old_value.is_none() {
                panic!(
                    "ErrorVrmManagerDuplicateReserve: The reservation tracking update of a reserved reservation of ADC {} on ShadowSchedule {:?} failed. The Reservation {:?} was already reserved before on VrmComponent {}. The new reserve was performed for VrmComponent {}",
                    self.adc_id,
                    shadow_schedule_id,
                    shadow_reservation_store.get_name_for_key(reservation_id),
                    old_value.unwrap(),
                    component_id
                );
            }

            if !shadow_reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::ReserveAnswer) {
                panic!(
                    "ErrorVrmManagerReservationStateIsNotAtLeastReserveAnswer: The tracking update of a reserved reservation of ADC {} on ShadowSchedule {:?} failed. The Reservation {:?} reserved on VrmComponent {} must have at least ReservationState::ReservedAnswer, but as only state {:?}.",
                    self.adc_id,
                    shadow_schedule_id,
                    shadow_reservation_store.get_name_for_key(reservation_id),
                    component_id,
                    shadow_reservation_store.get_state(reservation_id)
                );
            }
        }
    }
}
