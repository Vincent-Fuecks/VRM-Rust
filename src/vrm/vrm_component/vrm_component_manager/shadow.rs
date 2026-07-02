use crate::vrm::common::id::ShadowScheduleId;
use crate::vrm::reservation::reservation::ReservationState;

use super::VrmComponentManager;

impl VrmComponentManager {
    /// Creates a new Shadow Schedule environment.
    ///
    /// This snapshots the current ReservationStore and Component Mappings and propagates the creation
    /// to all child components.
    pub fn create_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        if self.shadow_schedule_reservations.contains_key(&shadow_schedule_id) {
            log::error!(
                "VrmComponentManagerShadowScheduleWithIdExistsAlready: The process of creating a new shadow Schedule for the ADC {} with ShadowScheduleId {:?} failed, because the provided ShadowScheduleId already exists, please first delete the other ShadowScheduleId.",
                self.adc_id,
                shadow_schedule_id
            );
            return false;
        }

        // 1. Snapshot the local state (ReservationStore and Allocation Map)
        let shadow_store = self.reservation_store.snapshot();
        // We clone the current allocation map (Who handles what) to serve as the baseline for the shadow schedule
        let shadow_map = self.res_to_vrm_component.clone();

        // 2. Propagate creation to all children (VrmComponents)
        for container in self.vrm_components.values_mut() {
            if !container.vrm_component.create_shadow_schedule(shadow_schedule_id.clone()) {
                log::error!("Failed to create shadow schedule on child component {:?}", container.vrm_component.get_id());
                // In a robust system, we would trigger a rollback here
                return false;
            }
        }

        // 3. Store the shadow context
        self.shadow_schedule_reservations.insert(shadow_schedule_id, (shadow_map, shadow_store));

        true
    }

    /// Discards a Shadow Schedule without applying changes (Rollback).
    pub fn delete_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        if !self.shadow_schedule_reservations.contains_key(&shadow_schedule_id) {
            return false;
        }

        // 1. Propagate deletion to all children
        for container in self.vrm_components.values_mut() {
            container.vrm_component.delete_shadow_schedule(shadow_schedule_id.clone());
        }

        // 2. Remove local shadow context
        self.shadow_schedule_reservations.remove(&shadow_schedule_id);

        true
    }

    /// Commits the Shadow Schedule to be the new Master Schedule.
    ///
    /// This replaces the live state with the shadow state.
    pub fn commit_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        if !self.shadow_schedule_reservations.contains_key(&shadow_schedule_id) {
            log::error!("Cannot commit shadow schedule {:?} as it does not exist.", shadow_schedule_id);
            return false;
        }

        // 1. Propagate commit to all children first
        for container in self.vrm_components.values_mut() {
            if !container.vrm_component.commit_shadow_schedule(shadow_schedule_id.clone()) {
                log::error!("Child component {:?} failed to commit shadow schedule.", container.vrm_component.get_id());
                return false;
            }
        }

        // 2. Atomic Switch: Replace Master State with Shadow State
        let (shadow_map, shadow_store) = self.shadow_schedule_reservations.remove(&shadow_schedule_id).unwrap();

        // Update the component mapping (Who handles what)
        self.res_to_vrm_component = shadow_map;

        // Update the reservation store (The source of truth for reservation states)
        self.reservation_store = shadow_store;

        // Rebuild derived mappings (committed/not_committed) based on the new store state
        // This ensures internal consistency after the swap
        self.committed_reservations.clear();
        self.not_committed_reservations.clear();

        for (res_id, component_id) in &self.res_to_vrm_component {
            let state = self.reservation_store.get_state(*res_id);
            match state {
                ReservationState::Committed => {
                    self.committed_reservations.insert(*res_id, component_id.clone());
                }
                ReservationState::ReserveAnswer => {
                    self.not_committed_reservations.insert(*res_id, component_id.clone());
                }
                _ => {} // Ignore others or log warning
            }
        }

        true
    }
}
