use parking_lot::RwLock;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;

use crate::domain::vrm_system_model::rms::rms::{Rms, RmsBase};
use crate::domain::vrm_system_model::rms::rms_node_network_trait::Helper;
use crate::vrm::commons::config::{MEMORY_PER_NODE, SLURM_RMS_COMMIT_TIMEOUT_S, SLURM_RMS_DELETE_TIMEOUT_S};
use crate::vrm::commons::id::ShadowScheduleId;
use crate::vrm::reservation::reservation::{ReservationState, ReservationTrait};
use crate::vrm::reservation::reservation_store::ReservationId;
use crate::vrm::schedule::schedule_trait::Schedule;

use super::api_client::payload::task_properties::{JobProperties, TaskSubmission};
use super::api_client::slurm_rest_api_trait::SlurmRestApi;
use super::slurm_base::SlurmRms;

impl Rms for SlurmRms {
    fn get_base(&self) -> &RmsBase {
        &self.base
    }

    fn get_base_mut(&mut self) -> &mut RmsBase {
        &mut self.base
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn commit(&self, reservation_id: ReservationId) {
        let payload;
        let reservation_store = self.get_reservation_store().clone();
        let client = self.slurm_rest_client.clone();
        let base_id = self.base.id.clone();
        let task_mapping = Arc::clone(&self.task_mapping);

        if let Some(reservation) = self.get_reservation_store().get_reservation_snapshot(reservation_id) {
            if let Some(node_res) = reservation.as_node() {
                payload = TaskSubmission {
                    job: JobProperties {
                        name: format!(
                            "{}, VRM-Res-ID: {:?}, Job-VRM-Name: {:?}",
                            base_id.id.clone(),
                            reservation_id,
                            self.get_reservation_store().get_name_for_key(reservation_id)
                        ),
                        cpus_per_task: node_res.base.reserved_capacity as u32,
                        nodes: None,
                        memory_per_node: MEMORY_PER_NODE,
                        begin_time: node_res.base.assigned_start as u64,
                        time_limit: node_res.base.assigned_end as u64,
                        current_working_directory: node_res.current_working_directory.clone(),
                        standard_output: node_res.output_path.clone(),
                        standard_error: node_res.error_path.clone(),
                        environment: node_res.environment.clone(),
                        dependency: None, // Is handled by the VrmManager
                    },

                    script: node_res.task_path.clone(),
                };
            } else {
                log::warn!(
                    "SlurmRmsCommitFalseReservationTypeError: Commit is only for NodeReservations possible instead a reservation of type {:?} was submitted.",
                    reservation.get_type()
                );
                self.get_reservation_store().update_state(reservation_id, ReservationState::Rejected);
                return;
            }
        } else {
            log::warn!("SlurmRmsCommitInValidReservationError: The reservation {:?} was not found.", reservation_id);
            return;
        }

        // Send NodeReservation to RMS
        self.rt_handle.spawn(async move {
            let result = timeout(Duration::from_secs(SLURM_RMS_COMMIT_TIMEOUT_S), client.commit(payload)).await;

            match result {
                Ok(Ok(task_id)) => {
                    task_mapping.write().insert(reservation_id, task_id);
                    log::info!(
                        "The reservation {:?} was successfully submitted to the local RMS {:?}",
                        reservation_store.get_name_for_key(reservation_id),
                        base_id
                    );
                    reservation_store.update_state(reservation_id, ReservationState::Committed);
                }
                Ok(Err(e)) => {
                    log::info!(
                        "The reservation {:?} submission failed to the local RMS {:?} the failure  was: {:?}",
                        reservation_store.get_name_for_key(reservation_id),
                        base_id,
                        e
                    );
                    reservation_store.update_state(reservation_id, ReservationState::Rejected);
                }
                Err(_) => {
                    log::info!(
                        "The reservation {:?} submission failed to the local RMS {:?} because the request exceeded the timeout of {:?} s.",
                        reservation_store.get_name_for_key(reservation_id),
                        base_id,
                        SLURM_RMS_COMMIT_TIMEOUT_S
                    );
                    reservation_store.update_state(reservation_id, ReservationState::Rejected);
                }
            }
        });
    }

    fn delete_task(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) {
        if self.get_reservation_store().get_state(reservation_id) != ReservationState::Committed {
            let active_scheduler = self.get_active_schedule(shadow_schedule_id, reservation_id);
            active_scheduler.write().delete_reservation(reservation_id);
            return;
        }

        if shadow_schedule_id.is_some() {
            log::error!(
                "SlurmRmsReservationStateDeletionError: The reservation {:?} has ReservationState::Committed and shadow_schedule_id is not None. 
                But reservations of the ShadowSchedule should not be committed to the local RMS.",
                reservation_id
            );

            return;
        }

        let reservation_store = self.get_reservation_store().clone();
        let client = self.slurm_rest_client.clone();
        let base_id = self.base.id.clone();
        let task_mapping = Arc::clone(&self.task_mapping);
        let active_scheduler = Arc::clone(&self.get_active_schedule(shadow_schedule_id, reservation_id));
        let slurm_task_id = task_mapping.read().get_by_left(&reservation_id).cloned();

        if let Some(slurm_task_id) = slurm_task_id {
            self.rt_handle.spawn(async move {
                let result = timeout(Duration::from_secs(SLURM_RMS_DELETE_TIMEOUT_S), client.delete(slurm_task_id)).await;

                match result {
                    Ok(Ok(_)) => {
                        task_mapping.write().remove_by_right(&slurm_task_id);
                        active_scheduler.write().delete_reservation(reservation_id);

                        if reservation_store.get_state(reservation_id) == ReservationState::Deleted {
                            log::info!(
                                "Deletion of the reservation {:?} was successfully for both local RMS {:?} and Schedule",
                                reservation_store.get_name_for_key(reservation_id),
                                base_id
                            );
                        } else {
                            log::error!(
                                "SlurmRmsDeletionCleanupError: The reservation {:?} was not successfully deleted from schedule, but the reservation was successfully deleted from the Rms system.",
                                reservation_id
                            );
                            reservation_store.update_state(reservation_id, ReservationState::Rejected);
                        }
                    }
                    Ok(Err(e)) => {
                        log::error!(
                            "Deletion of the reservation {:?} failed for both local RMS {:?} and Schedule. Error: {:?}",
                            reservation_store.get_name_for_key(reservation_id),
                            base_id,
                            e
                        );
                        reservation_store.update_state(reservation_id, ReservationState::Rejected);
                    }
                    Err(_) => {
                        log::error!(
                            "Deletion of reservation {:?} at local RMS {:?} and Schedule failed, because the request exceeded the timeout of {:?} s.",
                            reservation_store.get_name_for_key(reservation_id),
                            base_id,
                            SLURM_RMS_DELETE_TIMEOUT_S
                        );
                        reservation_store.update_state(reservation_id, ReservationState::Rejected);
                    }
                }
            });
        } else {
            log::warn!(
                "The reservation {:?} submission failed to the local RMS {:?} because the reservation id {:?} was not valid. 
                Set State to ReservationState::Deleted. Maybe the reservation was already deleted in a previous attempted?",
                reservation_store.get_name_for_key(reservation_id),
                base_id,
                reservation_id,
            );
            reservation_store.update_state(reservation_id, ReservationState::Deleted);
        }
    }

    fn get_active_schedule(&self, shadow_schedule_id: Option<ShadowScheduleId>, reservation_id: ReservationId) -> Arc<RwLock<Box<dyn Schedule>>> {
        if self.base.reservation_store.is_link(reservation_id) {
            match shadow_schedule_id {
                Some(id) => self.network_shadow_schedule.get(&id).expect("network_shadow_schedule contains ShadowSchedule.").clone(),
                None => self.network_schedule.clone(),
            }
        } else if self.base.reservation_store.is_node(reservation_id) {
            match shadow_schedule_id {
                Some(id) => self.node_shadow_schedule.get(&id).expect("node_shadow_schedule contains ShadowSchedule.").clone(),
                None => self.node_schedule.clone(),
            }
        } else {
            panic!(
                "RmsSimulatorErrorNoScheduleForReservation: The rms RmsSimulator has no Scheduler for Reservation type {:?}. ReservationName: {:?} ShadowScheduleId {:?}",
                self.base.reservation_store.get_type(reservation_id),
                self.base.reservation_store.get_name_for_key(reservation_id),
                shadow_schedule_id
            );
        }
    }
}

impl Helper for SlurmRms {
    fn get_node_shadow_schedule(&self) -> &HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>> {
        &self.node_shadow_schedule
    }

    fn get_mut_network_shadow_schedule(&mut self) -> &mut HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>> {
        &mut self.network_shadow_schedule
    }

    fn get_network_shadow_schedule(&self) -> &HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>> {
        &self.network_shadow_schedule
    }

    fn get_mut_node_shadow_schedule(&mut self) -> &mut HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>> {
        &mut self.node_shadow_schedule
    }

    fn get_node_schedule(&self) -> Arc<RwLock<Box<dyn Schedule>>> {
        self.node_schedule.clone()
    }

    fn get_network_schedule(&self) -> Arc<RwLock<Box<dyn Schedule>>> {
        self.network_schedule.clone()
    }

    fn set_node_schedule(&mut self, new_node_schedule: Arc<RwLock<Box<dyn Schedule>>>) {
        self.node_schedule = new_node_schedule;
    }

    fn set_network_schedule(&mut self, new_network_schedule: Arc<RwLock<Box<dyn Schedule>>>) {
        self.network_schedule = new_network_schedule;
    }
}
