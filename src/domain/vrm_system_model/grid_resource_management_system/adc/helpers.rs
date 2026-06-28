use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::domain::vrm_system_model::{
    grid_resource_management_system::adc::ADC,
    reservation::{probe_reservations::ProbeReservationComparator, reservation::ReservationState, reservation_store::ReservationId},
    utils::{
        id::{ComponentId, ShadowScheduleId},
        statistics::ANALYTICS_TARGET,
    },
};

impl ADC {
    /// Orchestrates the Shadow Scheduling process to optimize system utilization.
    ///
    /// This function performs the 4-step process requested:
    /// 1. Check satisfaction.
    /// 2. Create shadow schedule.
    /// 3. Reschedule reservations (Delete & Re-Reserve in Shadow).
    /// 4. Commit if better.
    pub fn optimize_schedule(&mut self) {
        // (1) Init shadow scheduling if system satisfaction is worse than threshold
        // Satisfaction Index: 0.0 (Optimal) -> 1.0 (Fragmented)
        // If fragmentation is high (> 0.5), we try to optimize.
        let current_satisfaction = self.manager.get_system_satisfaction(None);
        if current_satisfaction > 0.5 {
            let shadow_id = ShadowScheduleId::new("optimization_run".to_string());

            // (2) Create shadow schedule
            if self.manager.create_shadow_schedule(shadow_id.clone()) {
                // (3) Reschedule all reserved Reservation
                // Strategy: Clear the shadow schedule and re-insert tasks sorted by duration (Longest Task First).

                // A. Collect all active reservation IDs currently managed
                let active_ids: Vec<ReservationId> = self.manager.res_to_vrm_component.keys().cloned().collect();

                // B. Sort them by duration (Longest First Heuristic)
                let mut sorted_ids = active_ids.clone();
                sorted_ids.sort_by(|a, b| {
                    let dur_a = self.reservation_store.get_task_duration(*a);
                    let dur_b = self.reservation_store.get_task_duration(*b);
                    dur_b.cmp(&dur_a) // Descending
                });

                // C. Delete them from the Shadow Schedule context
                for res_id in &active_ids {
                    // This removes them from the underlying components in the shadow world
                    self.manager.delete_task_at_component(*res_id, Some(shadow_id.clone()));

                    // Reset the state in the shadow store to 'Open' so they can be reserved again.
                    // Accessing the shadow store via the manager.
                    if let Some((_, store)) = self.manager.shadow_schedule_reservations.get_mut(&shadow_id) {
                        store.update_state(*res_id, ReservationState::Open);
                    }
                }

                // D. Re-reserve them in the Shadow Schedule context
                let mut shadow_db = HashMap::new();
                for res_id in sorted_ids {
                    self.manager.reserve_reservation_at_best_vrm_component(
                        res_id,
                        Some(shadow_id.clone()),
                        &mut shadow_db,
                        ProbeReservationComparator::ESTReservationCompare,
                    );
                }

                // (4) Commit ShadowSchedule as the new master schedule if better
                let new_satisfaction = self.manager.get_system_satisfaction(Some(shadow_id.clone()));

                // Lower satisfaction index means less fragmentation/better schedule
                if new_satisfaction < current_satisfaction {
                    log::info!(
                        "Shadow Optimization Successful: Improved satisfaction from {} to {}. Committing.",
                        current_satisfaction,
                        new_satisfaction
                    );
                    self.manager.commit_shadow_schedule(shadow_id);
                } else {
                    log::info!("Shadow Optimization Discarded: No improvement ({} vs {}). Rolling back.", new_satisfaction, current_satisfaction);
                    self.manager.delete_shadow_schedule(shadow_id);
                }
            }
        }
    }

    /// Submits a task to the first VrmComponent that accepts the reservation based on the defined `VrmComponentOrder`.
    ///
    /// Updates the `TODO` to maintain the mapping between the
    /// reservation and the component that accepted it.
    pub fn submit_task_at_first_grid_component(
        &mut self,
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        grid_component_res_database: &mut HashMap<ReservationId, ComponentId>,
    ) -> ReservationId {
        let res_snapshot = match self.reservation_store.get_reservation_snapshot(reservation_id) {
            Some(snapshot) => snapshot,
            None => {
                log::error!("Cannot submit task: snapshot for {:?} not found.", reservation_id);
                self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                return reservation_id;
            }
        };

        // Wrong order
        for component_id in self.manager.get_ordered_vrm_components(self.vrm_component_order) {
            if self.manager.can_component_handel(component_id.clone(), res_snapshot.clone()) {
                let reserve_res_id = self.manager.reserve(component_id.clone(), reservation_id, shadow_schedule_id.clone());

                if self.reservation_store.is_reservation_state_at_least(reserve_res_id, ReservationState::ReserveAnswer) {
                    // Register new schedule Sub-Task
                    // Update grid_component_res_database for rollback and for ADC to keep track
                    // Update local WorkflowScheduler Log (for rollback and later merge)
                    if grid_component_res_database.contains_key(&reserve_res_id) {
                        log::error!(
                            "ErrorReservationWasReservedInMultipleGridComponents: The reservation {:?} was multiple times to the GirdComponent {} submitted.",
                            self.reservation_store.get_name_for_key(reserve_res_id),
                            component_id
                        );
                    }
                    grid_component_res_database.insert(reserve_res_id, component_id.clone());

                    // Update VrmComponent's local view (schedule) of the underlying VrmComponents
                    self.manager.reserve_without_check(component_id.clone(), reserve_res_id);

                    if !self.reservation_store.is_reservation_state_at_least(reserve_res_id, ReservationState::ReserveAnswer) {
                        log::error!("Reserve of reservation {:?} in local schedule copy of Grid Component {} failed.", reserve_res_id, component_id);
                    }

                    return reserve_res_id;
                }
            }
        }
        self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
        return reservation_id;
    }

    pub fn log_state_probe(&mut self, num_of_answers: i64, arrival_time_at_aci: i64) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let processing_time = self.simulator.get_system_time_s() - arrival_time_at_aci;
        tracing::info!(
            target: ANALYTICS_TARGET,
            Time = now,
            Command = "Commit".to_string(),
            ProbeAnswers = num_of_answers,
            ProcessingTime = processing_time,
        );
    }

    pub fn log_stat(&mut self, command: String, reservation_id: ReservationId, arrival_time_at_aci: i64) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let processing_time = self.simulator.get_system_time_s() - arrival_time_at_aci;

        if let Some(res_handle) = self.reservation_store.get(reservation_id) {
            let (start, end, res_name, capacity, workload, state, proceeding, num_tasks) = {
                let res = res_handle.read();

                let start = res.get_base_reservation().get_assigned_start();
                let end = res.get_base_reservation().get_assigned_end();
                let name = res.get_base_reservation().get_name().clone();
                let cap = res.get_base_reservation().get_reserved_capacity();
                let workload = res.get_base_reservation().get_task_duration() * cap;
                let state = res.get_base_reservation().get_state();
                let proceeding = res.get_base_reservation().get_reservation_proceeding();

                let mut tasks = 1;
                if res.is_workflow() {
                    tasks = res.as_workflow().unwrap().get_all_reservation_ids().len()
                }

                (start, end, name, cap, workload, state, proceeding, tasks)
            };

            let rms_load_metric = self.manager.get_load_metric(start, end, None);

            let node_utilization = rms_load_metric.node_load_metric.as_ref().map(|n| Some(n.utilization)).unwrap_or(None);

            let node_possible_capacity = rms_load_metric.node_load_metric.as_ref().map(|n| Some(n.possible_capacity)).unwrap_or(None);

            let network_utilization = rms_load_metric.link_load_metric.as_ref().map(|n| Some(n.utilization)).unwrap_or(None);

            let network_possible_capacity = rms_load_metric.link_load_metric.as_ref().map(|n| Some(n.possible_capacity)).unwrap_or(None);

            tracing::info!(
                target: ANALYTICS_TARGET,
                Time = now,
                LogDescription = "AcI Operation finished",
                ComponentType = %self.id,
                NodeComponentUtilization = node_utilization,
                NodeComponentCapacity = node_possible_capacity,
                NetworkComponentUtilization = network_utilization,
                NetworkComponentCapacity = network_possible_capacity,
                ComponentFragmentation = self.manager.get_system_satisfaction(None),
                ReservationName = %res_name,
                ReservationCapacity = capacity,
                ReservationWorkload = workload,
                ReservationState = ?state,
                ReservationProceeding = ?proceeding,
                NumberOfTasks = num_tasks,
                Command = command,
                ProcessingTime = processing_time,
            );
        } else {
            // Handling in case reservation is missing (e.g. deleted/cleaned up)

            tracing::warn!(
                target: ANALYTICS_TARGET,
                Time = now,
                LogDescription = "AcI Operation finished (Reservation Missing/Deleted)",
                ComponentType = %self.id,
                ReservationId = ?reservation_id,
                Command = command,
                ProcessingTime = processing_time,
            );
        }
    }
}
