use crate::vrm::reservation::probe_reservations::{ProbeReservationComparator, ProbeReservations};
use crate::vrm::reservation::reservation::Reservation;
use crate::vrm::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::rms::rms::{Rms, RmsLoadMetric};
use crate::vrm::commons::id::ShadowScheduleId;

/// Direct interface to a local Resource Management System (RMS) capable of making advance reservations.
///
/// This trait serves as the bridge between the high-level Virtual Resource Manager (VRM) and the
/// underlying local RMS. It facilitates the core reservation lifecycle: **Probe**, **Reserve**, and **Commit**.
///
/// # Core Functions:
/// 1.  **[probe](Self::probe)**: Queries the RMS for available reservation candidates based on constraints.
///     This operation is read-only and does not reserve resources.
/// 2.  **[reserve](Self::reserve)**: Temporarily reserves a job. This submits the request to the real RMS
///     but marks it as temporarily.
/// 3.  **[commit](Self::commit)**: Finalizes the reservation. Once committed, the reservation is fixed
///     and should not be cancelled via `delete_task` during normal operation.
/// 4.  **[delete_task](Self::delete_task)**: Cancels a reservation. This typically occurs after
///     `reserve` but before `commit`, or if the user explicitly cancels a job.
///
/// # Shadow Schedules
/// This interface supports **Shadow Schedules** isolated copies of the actual booking schedule.
/// Operations performed on a shadow schedule (identified by a [`ShadowScheduleId`]) do not affect
/// the live RMS until [commit_shadow_schedule](Self::commit_shadow_schedule) is called. This is critical
/// for distributed transactions and "what-if" planning phases in the Grid/VRM system.
pub trait AdvanceReservationRms: Rms + Send + Sync {
    /// Creates a secondary **Shadow Schedule**.
    ///
    /// Initially, this schedule is an exact clone of the master schedule. It allows for
    /// manipulative operations (like testing reservations) without affecting the actual
    /// live reservations.
    ///
    /// # Arguments
    ///
    /// * `shadow_schedule_id` - A unique identifier for the new shadow schedule.
    ///
    /// # Errors
    ///
    /// Logs an error if a shadow schedule with the given ID already exists.
    fn create_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool;

    /// Commits a specific **Shadow Schedule**, replacing the master schedule.
    ///
    /// This operation applies all changes made in the simulation (shadow) phase to the live system.
    /// Since operations on the shadow schedule (like `reserve` or `delete_task`) validate constraints
    /// incrementally, the switch is generally expected to succeed.
    ///
    /// # Arguments
    ///
    /// * `shadow_schedule_id` - The shadow schedule with the provided identifier is promote to master.
    ///
    /// # Returns
    ///
    /// * `true` if the changes were successfully applied.
    /// * `false` if the shadow schedule could not be found, leaving the old master schedule valid.
    ///
    /// # Note
    ///
    /// After a successful commit, the `shadow_schedule_id` is consumed and no longer available.
    fn commit_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool;

    /// Calculates the fragmentation of the schedule within a specific time range.
    ///
    /// Returns a value between `0.0` and `1.0`, where `0.0` represents optimal continuity
    /// (lowest fragmentation) and `1.0` represents high fragmentation.
    ///
    /// # Arguments
    ///
    /// * `start` - The start of the time window in VRM time (seconds).
    /// * `end` - The end of the time window in VRM time (seconds).
    /// * `shadow_schedule_id` - If `Some`, analyzes the specified shadow schedule.
    ///                          If `None`, analyzes the master schedule.
    fn get_fragmentation(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> f64;

    /// Calculates the global fragmentation of the entire schedule.
    ///
    /// # Arguments
    ///
    /// * `shadow_schedule_id` - If `Some`, analyzes the specified shadow schedule.
    ///                          If `None`, analyzes the master schedule.
    ///
    /// # Returns
    ///
    /// A value between `0.0` (best case) and `1.0` (worst case).
    fn get_system_fragmentation(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> f64;

    /// Retrieves load metrics for a specific time range.
    ///
    /// # Arguments
    ///
    /// * `start` - The start of the time window in VRM time (seconds).
    /// * `end` - The end of the time window in VRM time (seconds).
    /// * `shadow_schedule_id` - If `Some`, queries the specified shadow schedule.
    ///                          If `None`, queries the master schedule.
    ///
    /// # Returns
    ///
    /// A [`LoadMetric`] containing the calculated utilization metrics.
    fn get_load_metric_up_to_date(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric;

    /// Retrieves load metrics for a specific time range.
    ///
    /// # Arguments
    ///
    /// * `start` - The start of the time window in VRM time (seconds).
    /// * `end` - The end of the time window in VRM time (seconds).
    /// * `shadow_schedule_id` - If `Some`, queries the specified shadow schedule.
    ///                          If `None`, queries the master schedule.
    ///
    /// # Returns
    ///
    /// A [`LoadMetric`] containing the calculated utilization metrics.
    fn get_load_metric(&self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric;

    /// Retrieves load metrics for the total simulation.
    ///
    /// # Arguments
    ///
    /// * `shadow_schedule_id` - If `Some`, queries the specified shadow schedule.
    ///                          If `None`, queries the master schedule.
    ///
    /// # Returns
    ///
    /// A [`LoadMetric`] containing the calculated utilization metrics.
    fn get_simulation_load_metric(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric;

    /// Probes the RMS for possible reservation configurations.
    ///
    /// This method checks the schedule for slots that satisfy the constraints of the given
    /// reservation request (e.g., task_duration, booking interval, is_moldable). It does **not** modify the schedule.
    ///
    /// # Arguments
    ///
    /// * `reservation_id` - The ID of the reservation to check. The check is based on
    ///   booking intervals and duration, ignoring currently assigned start/end times.
    /// * `shadow_schedule_id` - If `Some`, probes the specified shadow schedule.
    ///                          If `None`, probes the master schedule.
    ///
    /// # Returns
    ///
    /// A [`Reservations`] object containing a list of valid configuration candidates.
    /// Each candidate will have its state set to `ReservationState::ProbeAnswer`.
    /// If no candidates are found, an empty list is returned.
    fn probe(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ProbeReservations {
        let active_scheduler = self.get_active_schedule(shadow_schedule_id, reservation_id);
        active_scheduler.write().probe(reservation_id)
    }

    /// Submits a reservation request to the local RMS.
    ///
    /// This attempts to book the resource. If successful, the reservation is recorded
    /// in the schedule (shadow or master).
    ///
    /// # Arguments
    ///
    /// * `reservation_id` - The ID of the task to reserve.
    /// * `shadow_schedule_id` - If `Some`, reserves on the specified shadow schedule.
    ///                          If `None`, reserves on the master schedule.
    ///
    /// # Returns
    ///
    /// * `Some(ReservationId)` if the reservation was successful. The state will be
    ///   `ReservationState::ReserveAnswer`.
    /// * `None` if the reservation was rejected (e.g., due to conflicts). The state
    ///   will be `ReservationState::Rejected`
    fn reserve(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> Option<ReservationId> {
        let active_scheduler = self.get_active_schedule(shadow_schedule_id, reservation_id);
        active_scheduler.write().reserve(reservation_id)
    }

    /// Destroys the specified **Shadow Schedule**.
    ///
    /// This is used to clean up simulation data. The master schedule remains active and unaffected.
    ///
    /// # Arguments
    ///
    /// * `shadow_schedule_id` - The unique identifier of the shadow schedule to remove.
    fn delete_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool;

    /// Probes for the single best reservation candidate based on a comparator.
    ///
    /// # Arguments
    ///
    /// * `request_id` - The ID of the reservation request.
    /// * `comparator` - A closure defining the ordering logic to determine the "best" candidate.
    /// * `shadow_schedule_id` - If `Some`, probes the specified shadow schedule.
    ///                          If `None`, probes the master schedule.
    ///
    /// # Returns
    ///
    /// `Some(ReservationId)` of the best candidate, or `None` if no valid slots exist.
    fn probe_best(
        &mut self,
        request_id: ReservationId,
        probe_reservation_comparator: ProbeReservationComparator,
        shadow_schedule_id: Option<ShadowScheduleId>,
    ) -> ProbeReservations {
        let active_scheduler = self.get_active_schedule(shadow_schedule_id, request_id);
        active_scheduler.write().probe_best(request_id, probe_reservation_comparator)
    }

    fn can_handle_adc_request(&self, res: Reservation) -> bool;

    fn can_handle_aci_request(&self, reservation_store: ReservationStore, reservation_id: ReservationId) -> bool;

    fn get_total_link_capacity(&self) -> i64 {
        self.get_base().resource_store.get_total_link_capacity()
    }

    fn get_total_node_capacity(&self) -> i64 {
        self.get_base().resource_store.get_total_node_capacity()
    }

    fn get_total_capacity(&self) -> i64 {
        self.get_base().resource_store.get_total_capacity()
    }

    fn get_link_resource_count(&self) -> usize {
        self.get_base().resource_store.get_num_of_links()
    }
}
