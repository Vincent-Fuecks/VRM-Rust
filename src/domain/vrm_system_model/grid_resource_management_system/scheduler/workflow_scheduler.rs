use crate::domain::vrm_system_model::grid_resource_management_system::adc::ADC;
use crate::vrm::reservation::reservation::{Reservation, ReservationState};
use crate::vrm::reservation::{
    reservation_store::{ReservationId, ReservationStore},
    reservations::Reservations,
};
use std::any::Any;

/// Defines the core interface for scheduling workflows within the **VRM System**.
///
/// A **Workflow Scheduler** is responsible for managing the lifecycle of complex workflows within
/// an **ADC** unit. It orchestrates the process of validating constraints,
/// probing for available resources across distributed nodes, and finalizing reservations.
///
/// This trait provides the blueprint for various scheduling algorithms (e.g., HEFT, Exhaustive).
pub trait WorkflowScheduler: std::fmt::Debug + Any + Send {
    fn new(reservation: ReservationStore) -> Box<dyn WorkflowScheduler>
    where
        Self: Sized;
    fn get_reservation_store(&self) -> &ReservationStore;
    fn as_any(&self) -> &dyn Any;
    fn name(&self) -> &str;

    /// Attempts to reserve resources for a workflow such that all distributed constraints are met.
    ///
    /// # Arguments
    /// * `workflow_res_id` - The unique identifier of the workflow reservation request.
    /// * `adc` - The ADC unit responsible for the grid resources.
    ///
    /// # Returns
    /// * `true` if the reservation was successful (state becomes `ReservationState::ReservedAnswer`).
    /// * `false` if the reservation was rejected (state becomes `ReservationState::Rejected`)..
    fn reserve(&mut self, workflow_res_id: ReservationId, adc: &mut ADC) -> bool;

    /// Probes the system for possible reservation configurations without committing resources.
    ///
    /// This is used to check multiple "what-if" scenarios across registered [`ExtendedReservationProcessor`] components.
    ///
    /// # Returns
    /// A [`Reservations`] collection containing possible configurations. Each valid configuration
    /// will have its state set to `ReservationState::ProbeAnser`.
    fn probe(&mut self, workflow_res_id: ReservationId, adc: &mut ADC) -> Reservations;

    /// Retrieves all sub-reservation identifiers associated with a parent workflow (for commit, delete_task).
    fn get_sub_ids(&self, workflow_id: ReservationId) -> Vec<ReservationId> {
        self.get_reservation_store()
            .get(workflow_id)
            .and_then(|handle| {
                let res = handle.read();
                if let Reservation::Workflow(ref workflow) = *res { Some(workflow.get_all_reservation_ids()) } else { None }
            })
            .unwrap_or_default()
    }

    /// Finalizes the commitment of a workflow and all its sub-reservations.
    fn finalize_commit(&mut self, workflow_id: ReservationId) {
        let store = self.get_reservation_store();
        if let Some(handle) = store.get(workflow_id) {
            let mut reservation = handle.write();
            if let Reservation::Workflow(ref mut workflow) = *reservation {
                for res_id in workflow.get_all_reservation_ids() {
                    workflow.update_reservation(store.clone(), res_id);
                }
            }
        }
        store.update_state(workflow_id, ReservationState::Committed);
    }

    /// Deletes a previously submitted workflow from all booked resource providers and sets all reservations in to `ReservationState::Deleted.
    fn delete(&mut self, workflow_id: ReservationId) {
        for sub_res_id in self.get_reservation_store().get_workflow_res_ids(workflow_id).unwrap() {
            self.get_reservation_store().update_state(sub_res_id, ReservationState::Rejected);
        }
    }
}

/// A base structure providing shared storage for concrete [`WorkflowScheduler`] implementations.
#[derive(Debug)]
pub struct WorkflowSchedulerBase {
    pub reservation_store: ReservationStore,
}
