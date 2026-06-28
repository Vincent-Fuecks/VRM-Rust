use crate::vrm::commons::id::{ComponentId, ShadowScheduleId};
use crate::vrm::reservation::probe_reservations::{ProbeReservationComparator, ProbeReservations};
use crate::vrm::reservation::reservation::Reservation;
use crate::vrm::reservation::reservation_store::ReservationId;
use crate::vrm::rms::rms::RmsLoadMetric;

/// A specialized interface for a fully-featured **Distributed Resource Management System**.
///
/// This trait is designed for **Grid Resource Management Systems (GRMS)** and adapters
/// connecting to local resource management systems. It extends basic reservation logic
/// by introducing **Shadow Schedules** and a sophisticated **Three-Level Commitment** model.
///
/// ### The Three Levels of Commitment
///
/// In a distributed Grid environment, managing resource state requires a clear lifecycle:
/// 1. **Probe**: A non-binding inquiry to discover possible resource configurations.
/// 2. **Reserve**: A preliminary commitment. The system guarantees resource availability
///    for a specific **Commit Timeout** period. Also is the Deletion of reservation
///    without additional cost is possible.
/// 3. **Commit**: The final handshake. Both the requester and the resource provider
///    fix the reservation parameters.
/// 4. **Delete**: Cancels a reservation. This is free during the "Reserved" state but
///    may impose penalties if the reservation was already "Committed."
///
/// ### Shadow Schedules
/// This interface allows operations to be performed against a "Shadow Schedule" a sandbox
/// environment used to simulate scheduling changes without affecting the live production
/// resource flow.
pub trait VrmComponent: std::fmt::Debug {
    /// Returns the unique identifier of the component (AcI or ADC).
    fn get_id(&self) -> ComponentId;

    /// Returns the total capacity of the component.
    fn get_total_capacity(&self) -> i64;

    /// Returns the total link capacity of the component.
    fn get_total_link_capacity(&self) -> i64;

    /// Returns the number of distinct link resources.
    fn get_link_resource_count(&self) -> usize;

    /// Returns the total node capacity (often same as total capacity depending on model).
    fn get_total_node_capacity(&self) -> i64;

    // Return true, if the provided reservation can be scheduled on teh GridComponent
    fn can_handel(&self, res: Reservation) -> bool;

    /// Sends a **Probe Request** to the resource management system.
    ///
    /// This is a read-only operation used to gather potential configurations for a
    /// reservation based on the system's current information-hiding policy.
    ///
    /// **Note**: A successful probe does not guarantee that a subsequent `reserve`
    /// call will succeed, though it is highly likely.
    ///
    /// # Arguments
    /// * `reservation_id` - The reservation id. Fields like `assigned_start` are ignored
    ///   in favor of `booking_interval`, `is_moldable` and `task_duration`.
    /// * `shadow_schedule_id` - If `Some`, utilize specified shadow schedule.
    ///                          If `None`, utilize the master schedule.
    ///
    /// # Returns
    /// A `Reservations` object, of which all contained reservation are set to the state
    /// `ReservationState::ProbeAnswer`.
    fn probe(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ProbeReservations;

    /// Finds the optimal reservation configuration based on a custom comparison logic.
    ///
    /// This utility method probes the system and automatically selects the "best"
    /// option (e.g., earliest start time or lowest cost) as defined by the `comparator`.
    fn probe_best(
        &mut self,
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        probe_reservation_comparator: ProbeReservationComparator,
    ) -> ProbeReservations;

    /// Sends a **Reserve Request** to initiate a preliminary commitment.
    ///
    /// The system will submit the task to the local Resource Management System (RMS)
    /// and hold the resources for a predefined timeout period. The requester must
    /// either `commit` or `delete` the reservation before this timeout expires.
    ///
    /// # Arguments
    /// * `reservation_id` - The reservation_id to reserve.
    /// * `shadow_schedule_id` - If `Some`, utilize specified shadow schedule.
    ///                          If `None`, utilize the master schedule.
    ///
    /// # Returns
    /// A `Reservation` object. Success is indicated by `ReservationState::ReserveAnswer`.
    /// If resources cannot be held, returns `ReservationState::Rejected`.
    fn reserve(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId;

    /// Sends a **Commit Request** to finalize a reservation.
    ///
    /// This informs the local Resource Management System (RMS) that the task is
    /// formally committed. Once committed, the task is protected from deletion
    /// under normal operating conditions and may be subject to fines if canceled.
    ///
    /// **Note**: Shadow schedules cannot be committed at the individual task level.
    /// To apply changes from a shadow schedule, use [`Self::commit_shadow_schedule`].
    ///
    /// # Arguments
    /// * `reservation_id` - The task to commit.
    ///
    /// # Returns
    ///  `true`     if the reservation of the provided `ReservationId` was committed and the
    ///             state of the reservation is now `ReservationState::Committed`.
    ///  `false`    if during the reservation process an error was encountered or the reservation state is not
    ///             `ReservationState::Committed`.
    fn commit(&mut self, reservation_id: ReservationId) -> bool;

    /// Sends a **Delete Request** to remove a task from the schedule.
    ///
    /// This removes a formerly submitted or reserved task from the local RMS.
    ///
    /// # Arguments
    /// * `reservation_id` - The task id to be removed.
    /// * `shadow_schedule_id` - If `Some`, utilize specified shadow schedule.
    ///                          If `None`, utilize the master schedule.
    ///
    /// # Returns
    /// A `ReservationId` indicating the final status. Success is confirmed if
    /// the state of the corresponding reservation is `ReservationState::Deleted`.
    fn delete(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId;

    /// Calculates the **Satisfaction Index** for a specific time window.
    ///
    /// The satisfaction index is a value between **0.0** and **1.0** based on
    /// schedule fragmentation and resource load.
    /// * **0.0**: Optimal scheduling/high satisfaction.
    /// * **1.0**: Worst-case fragmentation/lowest satisfaction.
    ///
    /// # Arguments
    /// * `start` - Unix timestamp for the start of the analysis window.
    /// * `end` - Unix timestamp for the end of the analysis window.
    /// * `shadow_schedule_id` - If `Some`, utilize specified shadow schedule.
    ///                          If `None`, utilize the master schedule.
    fn get_satisfaction(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> f64;

    /// Calculates the **System-Wide Satisfaction Index** across the full schedule range.
    ///
    /// The satisfaction index is a value between **0.0** and **1.0** based on
    /// schedule fragmentation and resource load.
    /// * **0.0**: Optimal scheduling/high satisfaction.
    /// * **1.0**: Worst-case fragmentation/lowest satisfaction.
    ///
    ///  # Arguments
    /// * `shadow_schedule_id` - If `Some`, utilize specified shadow schedule.
    ///                          If `None`, utilize the master schedule.
    fn get_system_satisfaction(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> f64;

    /// Creates a **Secondary Shadow Schedule**.
    ///
    /// This creates an identical copy of the current live schedule. Operations performed
    /// on this ID will not affect live production until [`Self::commit_shadow_schedule`] is called.
    ///
    /// # Arguments
    /// * `shadow_schedule_id` - A unique identifier for the new sandbox environment.
    fn create_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool;

    /// Destroys a shadow schedule and discards all pending changes (**Rollback**).
    ///
    /// The live schedule remains untouched. After this call, the provided ID
    /// is no longer valid.
    fn delete_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool;

    /// Performs an **Atomic Switch** from a shadow schedule to the live schedule.
    ///
    /// Replaces all reservations in the normal schedule with those defined in the
    /// shadow schedule. This is typically used after running a series of simulation
    /// optimizations in the shadow environment.
    ///
    /// # Returns
    /// `true` if the switch was successful and the live schedule has been updated.
    /// Returns `false` if the switch failed, in which case the original live
    /// schedule remains active.
    fn commit_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool;

    /// Returns the current **Resource Load Metric** for a given time window.
    fn get_load_metric_up_to_date(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric;

    /// Returns the **Resource Load Metric** without an update for a given time window.
    /// Note: This **Resource Load Metric** could be outed.
    fn get_load_metric(&self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric;

    /// Retrieves **Simulation Load Metric** for the **overall simulation period**.
    fn get_simulation_load_metric(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric;
}
