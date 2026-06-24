use std::collections::HashMap;

use crate::domain::vrm_system_model::grid_resource_management_system::adc::ADC;
use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_manager::scheduling::DUMMY_COMPONENT_ID;
use crate::domain::vrm_system_model::reservation::probe_reservations::ProbeReservationComparator;
use crate::domain::vrm_system_model::reservation::reservation::ReservationState;
use crate::domain::vrm_system_model::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::utils::id::{ComponentId, WorkflowNodeId};
use crate::domain::vrm_system_model::workflow::workflow::Workflow;
use crate::domain::vrm_system_model::workflow::workflow_node::WorkflowNode;

/// Encapsulates the hardware-specific operations needed by workflow scheduling algorithms.
///
/// This context provides a simplified, decoupled API for:
/// - Probing available resources on VrmComponents
/// - Reserving individual node tasks
/// - Reserving link/dependency tasks between components
/// - Committing or rolling back all allocations made during scheduling
///
/// By using this context, workflow scheduling algorithms (such as HEFTSync, ExhaustiveEFT, etc.)
/// remain pure domain logic without direct dependency on the VRM hardware topology (ADC, AcI, RMS).
///
/// # Usage
/// 1. Create a new context via [`WorkflowExecutionContext::new_from_adc`] from the ADC.
/// 2. Pass the context to the scheduler's [`schedule()`](crate::domain::vrm_system_model::grid_resource_management_system::scheduler::workflow_scheduler::WorkflowScheduler::schedule) method.
/// 3. After scheduling, call [`apply_allocations`] on success, or [`cancel_all`] on failure.
pub struct WorkflowExecutionContext<'a> {
    /// Reference to the ADC for all hardware interactions.
    adc: &'a mut ADC,

    /// The average link speed across all registered resources, cached at context creation time.
    average_link_speed: i64,

    /// The booking interval end (deadline) for the workflow being scheduled.
    workflow_booking_interval_end: i64,

    /// Tracks which reservations are allocated to which VrmComponents.
    /// This is used for rollback and final registration.
    allocations: HashMap<ReservationId, ComponentId>,
}

impl<'a> WorkflowExecutionContext<'a> {
    /// Creates a new execution context from an ADC reference.
    ///
    /// # Arguments
    /// * `adc` - The ADC unit responsible for the grid resources.
    /// * `workflow_booking_interval_end` - The latest allowed end time (deadline) for this workflow.
    pub fn new_from_adc(adc: &'a mut ADC, workflow_booking_interval_end: i64) -> Self {
        let average_link_speed = adc.manager.get_average_link_speed() as i64;

        Self { adc, average_link_speed, workflow_booking_interval_end, allocations: HashMap::new() }
    }

    /// Returns the cached average link speed.
    pub fn get_average_link_speed(&self) -> i64 {
        self.average_link_speed
    }

    /// Returns the workflow deadline (booking interval end).
    pub fn get_workflow_booking_interval_end(&self) -> i64 {
        self.workflow_booking_interval_end
    }

    /// Returns a reference to the reservation store.
    pub fn get_reservation_store(&self) -> &ReservationStore {
        &self.adc.reservation_store
    }

    /// Returns the current allocations map.
    pub fn get_allocations(&self) -> &HashMap<ReservationId, ComponentId> {
        &self.allocations
    }

    /// Returns whether any allocation has failed so far.
    pub fn has_failure(&self) -> bool {
        self.allocations.keys().any(|res_id| {
            !self.adc.reservation_store.is_reservation_state_at_least(*res_id, ReservationState::ReserveAnswer)
        })
    }

    /// Reserves a single node task (computation) at the best VrmComponent using EFT comparison.
    ///
    /// # Arguments
    /// * `reservation_id` - The reservation ID of the node to schedule.
    /// * `workflow` - The parent workflow (used for updating time boundaries).
    ///
    /// # Returns
    /// `true` if the reservation was successful, `false` otherwise.
    pub fn reserve_node_eft(&mut self, reservation_id: ReservationId, workflow: &mut Workflow) -> bool {
        let candidate_id = self.adc.manager.reserve_reservation_at_best_vrm_component(
            reservation_id,
            None,
            &mut self.allocations,
            ProbeReservationComparator::EFTReservationCompare,
        );

        match candidate_id {
            Some(res_id)
                if self.adc.reservation_store.is_reservation_state_at_least(
                    res_id,
                    ReservationState::ReserveAnswer,
                ) =>
            {
                workflow.update_reservation(self.adc.reservation_store.clone(), res_id);
                if let Some(component_id) = self.allocations.get(&res_id) {
                    self.adc.manager.reserve_without_check(component_id.clone(), res_id);
                }
                true
            }
            _ => false,
        }
    }

    /// Reserves a set of co-allocated nodes (connected by sync dependencies) at the same time.
    pub fn schedule_co_allocation(&mut self, workflow: &mut Workflow, workflow_node: &mut WorkflowNode) -> bool {
        let co_allocation_key = match &workflow_node.co_allocation_key {
            Some(key) => key.clone(),
            None => {
                log::error!("WorkflowNode has no CoAllocation key assigned.");
                return false;
            }
        };

        let co_allocation = match workflow.co_allocations.get(&co_allocation_key) {
            Some(ca) => ca.clone(),
            None => {
                log::error!("CoAllocation key '{}' not found in workflow.", co_allocation_key);
                return false;
            }
        };

        let representative_res_id = workflow_node.reservation_id;

        let first_candidate_id = match self.reserve_single_node(workflow, representative_res_id) {
            Some(res_id) => res_id,
            None => return false,
        };

        let duration = self.adc.reservation_store.get_task_duration(first_candidate_id);
        let start = self.adc.reservation_store.get_assigned_start(first_candidate_id);
        let end = self.adc.reservation_store.get_assigned_end(first_candidate_id);

        for member_id in &co_allocation.members {
            let member_res_id = match workflow.nodes.get(member_id) {
                Some(node) => node.reservation_id,
                None => {
                    log::error!("CoAllocation member '{}' not found in workflow nodes.", member_id);
                    return false;
                }
            };

            if member_res_id == first_candidate_id {
                continue;
            }

            self.adc
                .reservation_store
                .set_booking_interval_start(member_res_id, start);
            self.adc.reservation_store.set_booking_interval_end(member_res_id, end);
            self.adc.reservation_store.adjust_capacity(member_res_id, duration);

            let member_candidate_id =
                self.adc
                    .submit_task_at_first_grid_component(member_res_id, None, &mut self.allocations);

            if !self.adc.reservation_store.is_reservation_state_at_least(
                member_candidate_id,
                ReservationState::ReserveAnswer,
            ) {
                log::debug!(
                    "CoAllocation member reservation failed: {:?}, start: {}, end: {}",
                    self.adc.reservation_store.get_name_for_key(member_res_id),
                    start,
                    end,
                );
                return false;
            }

            workflow.update_reservation(self.adc.reservation_store.clone(), member_candidate_id);
        }

        for member_id in co_allocation.members {
            if !self.schedule_sync_dependencies_for_node(workflow, member_id) {
                return false;
            }
        }

        true
    }

    /// Schedules a data dependency (file transfer) between two nodes.
    pub fn schedule_dependency(
        &mut self,
        dependency_reservation_id: ReservationId,
        workflow: &mut Workflow,
        start: i64,
        end: i64,
        is_file_transfer: bool,
        source_component_id: ComponentId,
        target_component_id: ComponentId,
    ) -> bool {
        if self.adc.reservation_store.get_reserved_capacity(dependency_reservation_id) == 0
            || source_component_id.compare(&target_component_id)
        {
            let actual_end = if is_file_transfer { start } else { end };
            return self.schedule_dummy_dependency(dependency_reservation_id, workflow, start, actual_end);
        }

        self.schedule_real_dependency(
            dependency_reservation_id,
            workflow,
            start,
            end,
            is_file_transfer,
            source_component_id,
            target_component_id,
        )
    }

    /// Registers all successful allocations with the VrmComponentManager.
    pub fn apply_allocations(&mut self, workflow_id: ReservationId) {
        self.adc.manager.register_workflow_subtasks(workflow_id, &self.allocations);
    }

    /// Cancels (rolls back) all reservations made within this context.
    pub fn cancel_all(&mut self) {
        for (reservation_id, _) in self.allocations.clone().iter() {
            if !self.adc.manager.delete_task_at_component(*reservation_id, None) {
                log::error!(
                    "Failed to rollback reservation {:?} during workflow scheduling cancellation.",
                    self.adc.reservation_store.get_name_for_key(*reservation_id)
                );
            }
        }
        self.allocations.clear();
    }

    // ========================
    // Private Helper Methods
    // ========================

    fn reserve_single_node(&mut self, workflow: &mut Workflow, reservation_id: ReservationId) -> Option<ReservationId> {
        let candidate = self.reserve_node_eft(reservation_id, workflow);

        if !candidate {
            self.adc.reservation_store.update_state(reservation_id, ReservationState::Open);
            let candidate = self.reserve_node_eft(reservation_id, workflow);
            if candidate { Some(reservation_id) } else { None }
        } else {
            Some(reservation_id)
        }
    }

    fn schedule_sync_dependencies_for_node(
        &mut self,
        workflow: &mut Workflow,
        target_node_id: WorkflowNodeId,
    ) -> bool {
        let target_node = match workflow.nodes.get(&target_node_id) {
            Some(node) => node,
            None => {
                log::error!("Target node '{}' not found in workflow.", target_node_id);
                return false;
            }
        };

        let target_res_id = target_node.reservation_id;
        let start_time = self.adc.reservation_store.get_assigned_start(target_res_id);
        let end_time = self.adc.reservation_store.get_assigned_end(target_res_id);

        for sync_dep_id in target_node.incoming_sync.clone() {
            let sync_dep = match workflow.sync_dependencies.get(&sync_dep_id) {
                Some(dep) => dep,
                None => {
                    log::error!("Sync dependency '{}' not found.", sync_dep_id);
                    return false;
                }
            };

            let source_node_id = match &sync_dep.source_node {
                Some(id) => id,
                None => {
                    log::error!("Sync dependency '{}' has no source node.", sync_dep_id);
                    return false;
                }
            };

            let source_res_id = match workflow.nodes.get(source_node_id) {
                Some(node) => node.reservation_id,
                None => {
                    log::error!("Source node '{}' not found for sync dependency.", source_node_id);
                    return false;
                }
            };

            let source_component_id = match self.allocations.get(&source_res_id) {
                Some(id) => id.clone(),
                None => {
                    log::error!(
                        "Source reservation {:?} for sync dependency has no component allocation.",
                        self.adc.reservation_store.get_name_for_key(source_res_id)
                    );
                    return false;
                }
            };

            let target_component_id = match self.allocations.get(&target_res_id) {
                Some(id) => id.clone(),
                None => {
                    log::error!(
                        "Target reservation {:?} for sync dependency has no component allocation.",
                        self.adc.reservation_store.get_name_for_key(target_res_id)
                    );
                    return false;
                }
            };

            if !self.schedule_dependency(
                sync_dep.reservation_id,
                workflow,
                start_time,
                end_time,
                false,
                source_component_id,
                target_component_id,
            ) {
                return false;
            }
        }

        true
    }

    fn schedule_dummy_dependency(
        &mut self,
        dependency_reservation_id: ReservationId,
        workflow: &mut Workflow,
        start: i64,
        end: i64,
    ) -> bool {
        self.adc
            .reservation_store
            .update_state(dependency_reservation_id, ReservationState::Committed);
        self.adc
            .reservation_store
            .set_assigned_start(dependency_reservation_id, start);
        self.adc
            .reservation_store
            .set_assigned_end(dependency_reservation_id, end);
        self.adc
            .reservation_store
            .set_reserved_capacity(dependency_reservation_id, 0);
        self.adc
            .reservation_store
            .set_task_duration(dependency_reservation_id, end - start);

        self.allocations
            .insert(dependency_reservation_id, DUMMY_COMPONENT_ID.clone());

        workflow.update_reservation(self.adc.reservation_store.clone(), dependency_reservation_id);
        true
    }

    fn schedule_real_dependency(
        &mut self,
        dependency_reservation_id: ReservationId,
        workflow: &mut Workflow,
        start: i64,
        end: i64,
        is_file_transfer: bool,
        source_component_id: ComponentId,
        target_component_id: ComponentId,
    ) -> bool {
        self.adc
            .reservation_store
            .update_state(dependency_reservation_id, ReservationState::Open);
        self.adc
            .reservation_store
            .set_booking_interval_start(dependency_reservation_id, start);
        self.adc
            .reservation_store
            .set_booking_interval_end(dependency_reservation_id, end);

        if is_file_transfer {
            self.adc.reservation_store.set_is_moldable(dependency_reservation_id, true);
        } else {
            self.adc.reservation_store.set_is_moldable(dependency_reservation_id, false);
            self.adc.reservation_store.set_task_duration(dependency_reservation_id, end - start);
        }

        let source_router_list = self.adc.manager.get_component_router_list(source_component_id.clone());
        let target_router_list = self.adc.manager.get_component_router_list(target_component_id.clone());

        for source_router_id in &source_router_list {
            for target_router_id in &target_router_list {
                if let Some(res_arc) = self.adc.reservation_store.get(dependency_reservation_id) {
                    let mut guard = res_arc.write();
                    if let Some(link) = guard.as_link_mut() {
                        link.start_point = Some(source_router_id.clone());
                        link.end_point = Some(target_router_id.clone());
                    }
                }

                if is_file_transfer {
                    self.adc.reservation_store.adjust_task_duration(dependency_reservation_id, 1);
                }

                let candidate_id = self.adc.submit_task_at_first_grid_component(
                    dependency_reservation_id,
                    None,
                    &mut self.allocations,
                );

                if self.adc.reservation_store.is_reservation_state_at_least(
                    candidate_id,
                    ReservationState::ReserveAnswer,
                ) {
                    self.allocations.insert(candidate_id, source_component_id.clone());
                    workflow.update_reservation(self.adc.reservation_store.clone(), candidate_id);
                    return true;
                }
            }
        }

        log::debug!(
            "Failed to schedule real dependency {:?} between components {:?} and {:?}",
            self.adc.reservation_store.get_name_for_key(dependency_reservation_id),
            source_component_id,
            target_component_id,
        );

        false
    }
}
