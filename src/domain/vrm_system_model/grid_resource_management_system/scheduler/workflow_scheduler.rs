use crate::domain::vrm_system_model::grid_resource_management_system::adc::ADC;
use crate::domain::vrm_system_model::reservation::reservation::{Reservation, ReservationState};
use crate::domain::vrm_system_model::workflow::workflow_execution_context::WorkflowExecutionContext;
use crate::domain::vrm_system_model::{
    reservation::{
        reservation_store::{ReservationId, ReservationStore},
        reservations::Reservations,
    },
    workflow::workflow::Workflow,
};
use std::any::Any;

/// Represents the result of a workflow scheduling operation.
///
/// This decouples the scheduling result from the hardware topology,
/// providing a clean output that the ADC can apply.
#[derive(Debug, Clone)]
pub struct WorkflowSchedulingResult {
    /// Whether the scheduling operation was successful.
    pub success: bool,
}

impl WorkflowSchedulingResult {
    pub fn new(success: bool) -> Self {
        Self { success }
    }

    pub fn success() -> Self {
        Self { success: true }
    }

    pub fn failure() -> Self {
        Self { success: false }
    }
}

/// Defines the core interface for scheduling workflows within the **VRM System**.
///
/// A **Workflow Scheduler** is responsible for managing the lifecycle of complex workflows within
/// an **ADC** unit. It orchestrates the process of validating constraints,
/// probing for available resources across distributed nodes, and finalizing reservations.
///
/// This trait provides the blueprint for various scheduling algorithms (e.g., HEFT, Exhaustive).
///
/// ### Decoupled from Hardware Topology
///
/// The `schedule()` method uses [`WorkflowExecutionContext`] instead of direct ADC references,
/// allowing scheduling algorithms to remain pure domain logic testable without a full VRM setup.
pub trait WorkflowScheduler: std::fmt::Debug + Any + Send {
    fn new(reservation: ReservationStore) -> Box<dyn WorkflowScheduler>
    where
        Self: Sized;
    fn get_reservation_store(&self) -> &ReservationStore;
    fn as_any(&self) -> &dyn Any;
    fn name(&self) -> &str;

    /// Schedules a workflow using the provided execution context.
    ///
    /// This is the primary scheduling method, decoupled from the VRM hardware topology.
    /// The scheduling algorithm only needs to interact with the `WorkflowExecutionContext`
    /// to probe, reserve, and manage resources.
    ///
    /// # Arguments
    /// * `workflow` - The workflow to schedule (mutable for state updates).
    /// * `context` - The execution context providing hardware-independent resource operations.
    ///
    /// # Returns
    /// A `WorkflowSchedulingResult` indicating success or failure.
    fn schedule(&mut self, workflow: &mut Workflow, context: &mut WorkflowExecutionContext) -> WorkflowSchedulingResult;

    /// Attempts to reserve resources for a workflow such that all distributed constraints are met.
    ///
    /// This method is the legacy API that takes an ADC reference directly.
    /// New code should prefer `schedule()` instead.
    ///
    /// # Arguments
    /// * `workflow_res_id` - The unique identifier of the workflow reservation request.
    /// * `adc` - The ADC unit responsible for the grid resources.
    ///
    /// # Returns
    /// * `true` if the reservation was successful (state becomes `ReservationState::ReservedAnswer`).
    /// * `false` if the reservation was rejected (state becomes `ReservationState::Rejected`).
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
