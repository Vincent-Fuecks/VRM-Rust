use crate::domain::vrm_system_model::{
    grid_resource_management_system::{adc::ADC, vrm_component_trait::VrmComponent},
    reservation::{
        probe_reservations::{ProbeReservationComparator, ProbeReservations},
        reservation::{Reservation, ReservationState},
        reservation_store::ReservationId,
    },
    rms::rms::RmsLoadMetric,
    utils::id::{ComponentId, ShadowScheduleId},
};

impl VrmComponent for ADC {
    fn get_id(&self) -> ComponentId {
        ComponentId::new(self.id.to_string())
    }

    fn get_total_capacity(&self) -> i64 {
        self.manager.get_total_capacity()
    }

    fn get_total_link_capacity(&self) -> i64 {
        self.manager.get_total_link_capacity()
    }

    fn get_total_node_capacity(&self) -> i64 {
        self.manager.get_total_node_capacity()
    }

    fn get_link_resource_count(&self) -> usize {
        self.manager.get_link_resource_count()
    }

    fn can_handel(&self, res: Reservation) -> bool {
        for component_id in self.manager.get_random_ordered_vrm_components() {
            if self.manager.can_component_handel(component_id, res.clone()) {
                return true;
            }
        }
        false
    }

    fn commit(&mut self, reservation_id: ReservationId) -> bool {
        let arrival_time = self.simulator.get_system_time_s();
        log::info!("ADC {} commits reservation {:?}.", self.id, self.reservation_store.get_name_for_key(reservation_id));
        // Get ComponentId where Reservation is reserved, for Workflows was the reservation by the WorkflowScheduler performed.
        // Most like likely happen before if not reserve now.
        if !self.manager.is_reservation_reserved(reservation_id) && !self.reservation_store.is_workflow(reservation_id) {
            log::info!(
                "There was no reserve performed for the commit of reservation {:?}, try to reserve reservation now.",
                self.reservation_store.get_name_for_key(reservation_id)
            );

            // Can VrmManagerHandel request
            if !self.manager.can_handel(reservation_id) {
                self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                log::debug!(
                    "Commit at ADC {} failed of Reservation {:?} was rejected, because VrmComponents can not handel reservation and no reservation was done prior.",
                    self.id,
                    self.reservation_store.get_name_for_key(reservation_id)
                );
                self.log_stat("Commit".to_string(), reservation_id, arrival_time);
                return false;
            }

            // Reserve now reservation
            self.reserve(reservation_id, None);

            if !self.reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::ReserveAnswer) {
                log::debug!(
                    "Commit at ADC {} failed of Reservation {:?} was rejected, because VrmComponents can not handel reservation and no reservation was done prior.",
                    self.id,
                    self.reservation_store.get_name_for_key(reservation_id)
                );

                self.log_stat("Commit".to_string(), reservation_id, arrival_time);
                return false;
            }
        }

        // Perform Commit at VrmComponentManager (Single or Workflow Reservation?)
        if self.reservation_store.is_workflow(reservation_id) {
            // Commit of the workflow only the entry tasks.
            let w_entry_jobs = self.reservation_store.get_workflow_entry_res_ids(reservation_id).unwrap();
            for w_entry_job in w_entry_jobs.clone() {
                match self.reservation_store.get_state(w_entry_job) {
                    ReservationState::ReserveAnswer => {
                        // Get ComponentId where Reservation was reserved
                        let component_id = if self.manager.is_reservation_reserved(w_entry_job) {
                            self.manager.get_reserved_component(w_entry_job).unwrap()
                        } else {
                            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                            log::error!(
                                "ErrorInReservationProcess: Commit at ADC {} failed of Reservation {:?}. There was no reserve at a VrmComponent for the reservation found. Should happen before.",
                                self.id,
                                self.reservation_store.get_name_for_key(w_entry_job)
                            );
                            return false;
                        };
                        let component_answer = self.manager.commit_at_component(w_entry_job, component_id.clone());

                        // Check if this specific sub-component succeeded
                        if self.reservation_store.is_reservation_state_at_least(w_entry_job, ReservationState::Committed) || !component_answer {
                            log::error!("Sub-task {:?} failed in workflow {:?}", w_entry_job, reservation_id);
                            self.manager.handle_commit_failure(w_entry_jobs);
                            return false;
                        }
                    }

                    // Dummy Dependencies (see WorkflowScheduler) or DataDependencies without payload.
                    ReservationState::Committed => {}

                    _ => {
                        log::error!(
                            "VrmComponentCommitInValidReservationState: The ReservationState of Reservation {:?} is {:?}, only accepted states are ReserveAnswer and Committed.",
                            self.reservation_store.get_name_for_key(w_entry_job),
                            self.reservation_store.get_state(w_entry_job)
                        );
                    }
                }
            }

            self.workflow_scheduler.as_mut().unwrap().finalize_commit(reservation_id);
        } else {
            // Non-workflow atomic job
            // Get ComponentId where Reservation was reserved
            let component_id = if self.manager.is_reservation_reserved(reservation_id) {
                self.manager.get_reserved_component(reservation_id).unwrap()
            } else {
                self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                log::error!(
                    "ErrorInReservationProcess: Commit at ADC {} failed of Reservation {:?}. There was no reserve at a 
                    VrmComponent for the reservation found. Should happen before.",
                    self.id,
                    self.reservation_store.get_name_for_key(reservation_id)
                );
                return false;
            };

            let is_committed = self.manager.commit_at_component(reservation_id, component_id);

            // Check if this specific sub-component succeeded
            if self.reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::Committed) || !is_committed {
                log::debug!("Commit for Task {:?} failed. Perform clean up.", reservation_id);

                self.manager.handle_commit_failure(vec![reservation_id]);
                return false;
            }
        }

        log::debug!("Success: Committed at ADC {} Reservation {:?}.", self.id, self.reservation_store.get_name_for_key(reservation_id));

        self.log_stat("Commit".to_string(), reservation_id, arrival_time);
        return true;
    }

    fn commit_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        self.manager.commit_shadow_schedule(shadow_schedule_id)
    }

    fn create_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        self.manager.create_shadow_schedule(shadow_schedule_id)
    }

    fn delete_shadow_schedule(&mut self, _shadow_schedule_id: ShadowScheduleId) -> bool {
        todo!()
    }

    fn delete(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId {
        log::info!("ADC Delete: Delete on ADC {} the Reservation {:?}", self.id, self.reservation_store.get_name_for_key(reservation_id));

        // Handel the clean up of a Workflow
        if self.reservation_store.is_workflow(reservation_id) {
            let mut is_deleted = true;
            for workflow_res_id in self.reservation_store.get_workflow_res_ids(reservation_id).unwrap().iter() {
                if !self.manager.delete_task_at_component(*workflow_res_id, shadow_schedule_id.clone()) {
                    log::error!("ErrorAdcWorkflowDeletion: No handler found for reservation {:?}", reservation_id);
                    self.reservation_store.update_state(*workflow_res_id, ReservationState::Rejected);
                    self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                    is_deleted = false;
                }
            }

            if is_deleted {
                self.reservation_store.update_state(reservation_id, ReservationState::Deleted);
            } else {
                self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                log::error!(
                    "ErrorAdcWorkflowDeletion: The process of deleting all related reservations of the Workflow: {:?} was not successful.",
                    reservation_id
                );
            }

            return reservation_id;
        }

        // Handle cleanup of atomic Reservation
        self.manager.delete_task_at_component(reservation_id, shadow_schedule_id);
        return reservation_id;
    }

    fn get_load_metric(&self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.manager.get_load_metric(start, end, shadow_schedule_id)
    }

    fn get_load_metric_up_to_date(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.manager.get_load_metric(start, end, shadow_schedule_id)
    }

    fn get_satisfaction(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        self.manager.get_satisfaction(start, end, shadow_schedule_id)
    }

    fn get_simulation_load_metric(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.manager.get_simulation_load_metric(shadow_schedule_id)
    }

    fn get_system_satisfaction(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        self.manager.get_system_satisfaction(shadow_schedule_id)
    }

    fn probe(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ProbeReservations {
        let arrival_time = self.simulator.get_system_time_s();
        let probe_request_answer = self.manager.probe_all_components(reservation_id);

        if probe_request_answer.is_empty() {
            if shadow_schedule_id.is_none() {
                self.log_state_probe(0, arrival_time);
            }
            return probe_request_answer;
        }

        if shadow_schedule_id.is_none() {
            self.log_state_probe(probe_request_answer.len() as i64, arrival_time);
        }

        return probe_request_answer;
    }

    fn probe_best(
        &mut self,
        _reservation_id: ReservationId,
        _shadow_schedule_id: Option<ShadowScheduleId>,
        _probe_reservation_comparator: ProbeReservationComparator,
    ) -> ProbeReservations {
        todo!()
    }

    fn reserve(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId {
        let arrival_time = self.simulator.get_system_time_s();
        log::debug!(
            "Reserve: At VrmComponent {:?}, ReservationId {:?}, ShadowSchedule {:?}",
            self.id,
            self.reservation_store.get_name_for_key(reservation_id),
            shadow_schedule_id
        );

        // Can VrmComponents handle Request?
        if !self.manager.can_handel(reservation_id) {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);

            if shadow_schedule_id.is_none() {
                self.log_stat("Reserve".to_string(), reservation_id, arrival_time);
            }
            return reservation_id;
        }

        // Perform Reserve
        if self.reservation_store.is_workflow(reservation_id) {
            // "Option Dance" with WorkflowScheduler
            if let Some(mut workflow_scheduler) = self.workflow_scheduler.take() {
                // Performs all reservation tracking like self.manager.not_committed_reservations
                workflow_scheduler.reserve(reservation_id, self);
                if !self.reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::ReserveAnswer) {
                    workflow_scheduler.delete(reservation_id);
                }

                self.workflow_scheduler = Some(workflow_scheduler);
            } else {
                log::error!("WorkflowScheduler is missing or currently in use (recursive call?) for ADC {:?}", self.id);
                self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            }
        } else {
            // Atomic Job
            self.manager.reserve_task_at_first_grid_component(reservation_id, shadow_schedule_id.clone(), self.vrm_component_order);
        }

        // Check reservation
        if !self.reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::ReserveAnswer) {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);

            if shadow_schedule_id.is_none() {
                self.log_stat("Reserve".to_string(), reservation_id, arrival_time);
            }

            return reservation_id;
        }

        if shadow_schedule_id.is_none() {
            self.log_stat("Reserve".to_string(), reservation_id, arrival_time);
        }
        return reservation_id;
    }
}
