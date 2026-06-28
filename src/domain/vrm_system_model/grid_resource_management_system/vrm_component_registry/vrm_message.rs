use crate::vrm::commons::id::{ComponentId, ShadowScheduleId};
use crate::vrm::reservation::probe_reservations::{ProbeReservationComparator, ProbeReservations};
use crate::vrm::reservation::reservation::Reservation;
use crate::vrm::reservation::reservation_store::ReservationId;
use crate::vrm::rms::rms::RmsLoadMetric;

use std::sync::mpsc;

/// Messages representing every method in the VrmComponent trait.
/// These allow us to serialize method calls across threads.
pub enum VrmMessage {
    GetId(mpsc::Sender<ComponentId>),
    GetTotalCapacity(mpsc::Sender<i64>),
    GetTotalLinkCapacity(mpsc::Sender<i64>),
    GetTotalNodeCapacity(mpsc::Sender<i64>),
    GetLinkResourceCount(mpsc::Sender<usize>),

    CanHandel {
        reservation: Reservation,
        reply_to: mpsc::Sender<bool>,
    },

    Probe {
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        reply_to: mpsc::Sender<ProbeReservations>,
    },

    ProbeBest {
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        probe_reservation_comparator: ProbeReservationComparator,
        reply_to: mpsc::Sender<ProbeReservations>,
    },

    Reserve {
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        reply_to: mpsc::Sender<ReservationId>,
    },

    Commit {
        reservation_id: ReservationId,
        reply_to: mpsc::Sender<bool>,
    },

    DeleteTask {
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        reply_to: mpsc::Sender<ReservationId>,
    },

    GetSatisfaction {
        start: i64,
        end: i64,
        shadow_schedule_id: Option<ShadowScheduleId>,
        reply_to: mpsc::Sender<f64>,
    },

    GetSystemSatisfaction {
        shadow_schedule_id: Option<ShadowScheduleId>,
        reply_to: mpsc::Sender<f64>,
    },

    CreateShadowSchedule {
        id: ShadowScheduleId,
        reply_to: mpsc::Sender<bool>,
    },

    DeleteShadowSchedule {
        id: ShadowScheduleId,
        reply_to: mpsc::Sender<bool>,
    },

    CommitShadowSchedule {
        id: ShadowScheduleId,
        reply_to: mpsc::Sender<bool>,
    },

    GetLoadMetricUpToDate {
        start: i64,
        end: i64,
        shadow_schedule_id: Option<ShadowScheduleId>,
        reply_to: mpsc::Sender<RmsLoadMetric>,
    },

    GetLoadMetric {
        start: i64,
        end: i64,
        shadow_schedule_id: Option<ShadowScheduleId>,
        reply_to: mpsc::Sender<RmsLoadMetric>,
    },

    GetSimulationLoadMetric {
        shadow_schedule_id: Option<ShadowScheduleId>,
        reply_to: mpsc::Sender<RmsLoadMetric>,
    },

    Shutdown,
}
