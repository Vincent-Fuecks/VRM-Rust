use crate::vrm::commons::id::ShadowScheduleId;
use crate::vrm::reservation::{
    reservation::{Reservation, ReservationTrait},
    reservation_store::{ReservationId, ReservationStore},
};
use crate::vrm::schedule::schedule_trait::Schedule;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use super::advance_reservation_trait::AdvanceReservationRms;
use super::rms::{Rms, RmsLoadMetric};
use super::rms_simulator::rms_simulator::RmsSimulator;
use super::slurm_rms::slurm_base::SlurmRms;

pub trait Helper {
    fn get_network_shadow_schedule(&self) -> &HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>;
    fn get_mut_network_shadow_schedule(&mut self) -> &mut HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>;

    fn get_node_shadow_schedule(&self) -> &HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>;
    fn get_mut_node_shadow_schedule(&mut self) -> &mut HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>;

    fn get_node_schedule(&self) -> Arc<RwLock<Box<dyn Schedule>>>;
    fn get_network_schedule(&self) -> Arc<RwLock<Box<dyn Schedule>>>;

    fn set_node_schedule(&mut self, new_node_schedule: Arc<RwLock<Box<dyn Schedule>>>);
    fn set_network_schedule(&mut self, new_network_schedule: Arc<RwLock<Box<dyn Schedule>>>);
}

trait RmsNodeNetwork: AdvanceReservationRms + Helper + Rms {}

impl<T: RmsNodeNetwork> AdvanceReservationRms for T {
    fn create_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool {
        if self.get_mut_network_shadow_schedule().contains_key(shadow_schedule_id)
            || self.get_mut_node_shadow_schedule().contains_key(shadow_schedule_id)
        {
            log::error!(
                "Creating new shadow schedule is not possible because shadow schedule id ({}) does already exist. Please first delete the old shadow schedule.",
                shadow_schedule_id
            );
            return false;
        }

        let node_schedule_clone = self.get_node_schedule().read().clone_box();
        let network_schedule_clone = self.get_network_schedule().read().clone_box();

        if !self.get_mut_node_shadow_schedule().insert(shadow_schedule_id.clone(), Arc::new(RwLock::new(node_schedule_clone))).is_none()
            || !self.get_mut_network_shadow_schedule().insert(shadow_schedule_id.clone(), Arc::new(RwLock::new(network_schedule_clone))).is_none()
        {
            log::error!("ErrorShadowScheduleAlreadyExists: ShadowSchedule is now curupted.");
            return false;
        }

        return true;
    }

    fn commit_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool {
        if self.get_mut_network_shadow_schedule().contains_key(shadow_schedule_id)
            && self.get_mut_node_shadow_schedule().contains_key(shadow_schedule_id)
        {
            let new_node_schedule = self.get_mut_node_shadow_schedule().remove(shadow_schedule_id);
            let new_network_schedule = self.get_mut_network_shadow_schedule().remove(shadow_schedule_id);

            if let (Some(node), Some(net)) = (new_node_schedule, new_network_schedule) {
                self.set_node_schedule(node);
                self.set_network_schedule(net);
                return true;
            }
        }

        log::error!("Finding and removing of shadow schedule with id {} was not possible", shadow_schedule_id.clone());
        return false;
    }

    fn delete_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool {
        if self.get_mut_network_shadow_schedule().contains_key(shadow_schedule_id)
            && self.get_mut_node_shadow_schedule().contains_key(shadow_schedule_id)
        {
            let removed_node_schedule = self.get_mut_node_shadow_schedule().remove(shadow_schedule_id);
            let removed_network_schedule = self.get_mut_network_shadow_schedule().remove(shadow_schedule_id);

            if removed_node_schedule.is_none() && removed_network_schedule.is_none() {
                return true;
            }
        }

        log::error!("Removing shadow schedule was not possible. Shadow schedule id ({}) was not found", shadow_schedule_id.clone());
        return false;
    }

    fn get_fragmentation(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        match shadow_schedule_id {
            Some(id) => {
                self.get_network_shadow_schedule()
                    .get(&id)
                    .expect("network_shadow_schedule contains ShadowSchedule.")
                    .write()
                    .get_fragmentation(start, end)
                    + self
                        .get_node_shadow_schedule()
                        .get(&id)
                        .expect("node_shadow_schedule contains ShadowSchedule.")
                        .write()
                        .get_fragmentation(start, end)
            }
            None => {
                self.get_network_schedule().write().get_fragmentation(start, end) + self.get_node_schedule().write().get_fragmentation(start, end)
            }
        }
    }

    fn get_system_fragmentation(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        match shadow_schedule_id {
            Some(id) => {
                self.get_network_shadow_schedule()
                    .get(&id)
                    .expect("network_shadow_schedule contains ShadowSchedule.")
                    .write()
                    .get_system_fragmentation()
                    + self
                        .get_node_shadow_schedule()
                        .get(&id)
                        .expect("node_shadow_schedule contains ShadowSchedule.")
                        .write()
                        .get_system_fragmentation()
            }
            None => self.get_network_schedule().write().get_system_fragmentation() + self.get_node_schedule().write().get_system_fragmentation(),
        }
    }

    fn can_handle_adc_request(&self, res: Reservation) -> bool {
        if res.is_link() || res.is_node() {
            return self.get_base().resource_store.can_handle_adc_request(res);
        }

        log::debug!(
            "The rms {:?} can not process Reservations of Type {:?} (ReservationName: {:?}) the rms can only process LinkReservations and NodeReservations.",
            self.get_base().id,
            res.get_type(),
            res.get_name()
        );
        return false;
    }

    fn can_handle_aci_request(&self, reservation_store: ReservationStore, reservation_id: ReservationId) -> bool {
        if reservation_store.is_link(reservation_id) || reservation_store.is_node(reservation_id) {
            return self.get_base().resource_store.can_handle_aci_request(reservation_store, reservation_id);
        }

        log::debug!(
            "The rms {:?} can not process Reservations of Type {:?} (ReservationName: {:?}) the rms can only process LinkReservations and NodeReservations.",
            self.get_base().id,
            reservation_store.get_type(reservation_id),
            reservation_store.get_name_for_key(reservation_id)
        );
        return false;
    }

    fn get_load_metric(&self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        match shadow_schedule_id {
            Some(id) => RmsLoadMetric {
                node_load_metric: Some(
                    self.get_node_shadow_schedule()
                        .get(&id)
                        .expect("network_shadow_schedule contains ShadowSchedule.")
                        .read()
                        .get_load_metric(start, end),
                ),
                link_load_metric: Some(
                    self.get_network_shadow_schedule()
                        .get(&id)
                        .expect("node_shadow_schedule contains ShadowSchedule.")
                        .read()
                        .get_load_metric(start, end),
                ),
            },
            None => RmsLoadMetric {
                node_load_metric: Some(self.get_node_schedule().read().get_load_metric(start, end)),
                link_load_metric: Some(self.get_network_schedule().read().get_load_metric(start, end)),
            },
        }
    }

    fn get_load_metric_up_to_date(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        match shadow_schedule_id {
            Some(id) => RmsLoadMetric {
                node_load_metric: Some(
                    self.get_node_shadow_schedule()
                        .get(&id)
                        .expect("network_shadow_schedule contains ShadowSchedule.")
                        .write()
                        .get_load_metric_up_to_date(start, end),
                ),
                link_load_metric: Some(
                    self.get_network_shadow_schedule()
                        .get(&id)
                        .expect("node_shadow_schedule contains ShadowSchedule.")
                        .write()
                        .get_load_metric_up_to_date(start, end),
                ),
            },
            None => RmsLoadMetric {
                node_load_metric: Some(self.get_node_schedule().write().get_load_metric_up_to_date(start, end)),
                link_load_metric: Some(self.get_network_schedule().write().get_load_metric_up_to_date(start, end)),
            },
        }
    }

    fn get_simulation_load_metric(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        match shadow_schedule_id {
            Some(id) => RmsLoadMetric {
                node_load_metric: Some(
                    self.get_node_shadow_schedule()
                        .get(&id)
                        .expect("network_shadow_schedule contains ShadowSchedule.")
                        .write()
                        .get_simulation_load_metric(),
                ),
                link_load_metric: Some(
                    self.get_network_shadow_schedule()
                        .get(&id)
                        .expect("node_shadow_schedule contains ShadowSchedule.")
                        .write()
                        .get_simulation_load_metric(),
                ),
            },
            None => RmsLoadMetric {
                node_load_metric: Some(self.get_node_schedule().write().get_simulation_load_metric()),
                link_load_metric: Some(self.get_network_schedule().write().get_simulation_load_metric()),
            },
        }
    }
}

impl RmsNodeNetwork for SlurmRms {}
impl RmsNodeNetwork for RmsSimulator {}
