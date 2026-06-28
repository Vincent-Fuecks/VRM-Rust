use std::sync::{ mpsc};

use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_registry::vrm_message::VrmMessage;
use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_trait::VrmComponent;
use crate::domain::vrm_system_model::reservation::probe_reservations::{ProbeReservationComparator, ProbeReservations};
use crate::domain::vrm_system_model::reservation::reservation::Reservation;
use crate::domain::vrm_system_model::reservation::reservation_store::ReservationId;
use crate::domain::vrm_system_model::rms::rms::RmsLoadMetric;
use crate::domain::vrm_system_model::utils::id::{ComponentId, ShadowScheduleId};

/// Proxy forwards everything to the thread owning the real component.
#[derive(Debug, Clone)]
pub struct VrmComponentProxy {
    pub id: ComponentId,
    pub tx: mpsc::Sender<VrmMessage>,
}

impl VrmComponentProxy {
    fn call<R, F>(&self, msg_builder: F) -> R
    where
        F: FnOnce(mpsc::Sender<R>) -> VrmMessage,
    {
        let (reply_tx, reply_rx) = mpsc::channel();
        let msg = msg_builder(reply_tx);

        match self.tx.send(msg) {
            Ok(_) => reply_rx.recv().expect("Remote component thread died unexpectedly"),
            Err(_) => panic!("Failed to send message to component {}", self.id),
        }
    }
}

impl VrmComponent for VrmComponentProxy {
    fn get_id(&self) -> ComponentId {
        self.id.clone()
    }

    fn get_total_capacity(&self) -> i64 {
        self.call(VrmMessage::GetTotalCapacity)
    }

    fn get_total_link_capacity(&self) -> i64 {
        self.call(VrmMessage::GetTotalLinkCapacity)
    }

    fn get_total_node_capacity(&self) -> i64 {
        self.call(|tx| VrmMessage::GetTotalNodeCapacity(tx))
    }

    fn get_link_resource_count(&self) -> usize {
        self.call(VrmMessage::GetLinkResourceCount)
    }

    fn can_handel(&self, res: Reservation) -> bool {
        self.call(|tx| VrmMessage::CanHandel { reservation: res, reply_to: tx })
    }

    fn probe(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ProbeReservations {
        self.call(|tx| VrmMessage::Probe { reservation_id, shadow_schedule_id, reply_to: tx })
    }

    fn probe_best(
        &mut self,
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        probe_reservation_comparator: ProbeReservationComparator,
    ) -> ProbeReservations {
        self.call(|tx| VrmMessage::ProbeBest { reservation_id, shadow_schedule_id, probe_reservation_comparator, reply_to: tx })
    }

    fn reserve(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId {
        self.call(|tx| VrmMessage::Reserve { reservation_id, shadow_schedule_id, reply_to: tx })
    }

    fn commit(&mut self, reservation_id: ReservationId) -> bool {
        self.call(|tx| VrmMessage::Commit { reservation_id, reply_to: tx })
    }

    fn delete(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId {
        self.call(|tx| VrmMessage::DeleteTask { reservation_id, shadow_schedule_id, reply_to: tx })
    }

    fn get_satisfaction(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        self.call(|tx| VrmMessage::GetSatisfaction { start, end, shadow_schedule_id, reply_to: tx })
    }

    fn get_system_satisfaction(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        self.call(|tx| VrmMessage::GetSystemSatisfaction { shadow_schedule_id, reply_to: tx })
    }

    fn create_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        self.call(|tx| VrmMessage::CreateShadowSchedule { id: shadow_schedule_id, reply_to: tx })
    }

    fn delete_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        self.call(|tx| VrmMessage::DeleteShadowSchedule { id: shadow_schedule_id, reply_to: tx })
    }

    fn commit_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        self.call(|tx| VrmMessage::CommitShadowSchedule { id: shadow_schedule_id, reply_to: tx })
    }

    fn get_load_metric_up_to_date(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.call(|tx| VrmMessage::GetLoadMetricUpToDate { start, end, shadow_schedule_id, reply_to: tx })
    }

    fn get_load_metric(&self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.call(|tx| VrmMessage::GetLoadMetric { start, end, shadow_schedule_id, reply_to: tx })
    }

    fn get_simulation_load_metric(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.call(|tx| VrmMessage::GetSimulationLoadMetric { shadow_schedule_id, reply_to: tx })
    }
}
