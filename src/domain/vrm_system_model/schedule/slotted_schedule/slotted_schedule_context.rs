use std::cmp::Ordering;
use std::collections::HashSet;
use std::i64;
use std::sync::Arc;

use crate::domain::simulator::simulator::GlobalClock;
use crate::domain::vrm_system_model::reservation::probe_reservations::ProbeReservations;
use crate::domain::vrm_system_model::reservation::reservation::{Reservation, ReservationState, ReservationTrait};
use crate::domain::vrm_system_model::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::reservation::reservations::Reservations;
use crate::domain::vrm_system_model::schedule::slotted_schedule::slot::Slot;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::strategy_trait::SlottedScheduleStrategy;
use crate::domain::vrm_system_model::utils::id::SlottedScheduleId;
use crate::domain::vrm_system_model::utils::load_buffer::{GlobalLoadContext, LoadBuffer};

const FRAGMENTATION_POWER: f64 = 2.0;

/// The core context for managing a time-slotted resource schedule within a distributed **VRM (Virtual Resource Management)** system.
///
/// `SlottedScheduleContext` acts as the primary data structure for timeline-based resource allocation.
/// It utilizes a sliding window mechanism (represented by a sequence of discrete time slots) to track
/// physical resource capacities (like CPU cores or network bandwidth) over time.
///
/// Note: The end of the scheduling window is defined by: Current_Unix_timestamp + slot_width * num_of_slots
/// For Example : NOW + (60*60) * (24) --> From the current start time (NOW) is Advanced Reservation for the next 24h possible for the system.
/// If a request is issued exceeding this time window, is the Reservation state set to ReservationState::Rejected
#[derive(Debug, Clone)]
pub struct SlottedScheduleContext<S: SlottedScheduleStrategy> {
    pub strategy: S,

    /// **Unique identifier** for this SlottedSchedule.
    pub id: SlottedScheduleId,

    /// A list of all time **Slots** defined for this schedule.
    /// Represents the actual capacity of the physical resource (e.g. number of cpus of the Node average bandwidth of the link).
    pub slots: Vec<Slot>,

    /// The duration of a single time slot in s.
    /// Example If the billing period of the cluster is hour a slot width of 60*60 = 3600s could be used
    pub slot_width: i64,

    /// The index of the earliest possible slot that can be used for scheduling.
    pub start_slot_index: i64,

    /// The index of the latest possible slot that defines the scheduling window's end.
    pub end_slot_index: i64,

    /// The **absolute start time** (e.g., Unix timestamp) of the current scheduling window being viewed.
    pub scheduling_window_start_time: i64,

    /// The **absolute end time** (e.g., Unix timestamp) of the current scheduling window being viewed.
    pub scheduling_window_end_time: i64,

    /// Internal buffer used to track transient or potential resource load.
    pub load_buffer: LoadBuffer,

    /// A map of all currently **active reservations** associated with this schedule.
    pub active_reservations: Reservations,

    /// Flag indicating if the stored **fragmentation_cache** value is valid and up-to-date.
    pub is_frag_cache_up_to_date: bool,

    /// The cached value of the system **fragmentation**.
    pub fragmentation_cache: f64,

    /// A configuration flag to determine if the system should utilize the **quadratic mean**
    /// or the standard formula for fragmentation calculation.
    pub use_quadratic_mean_fragmentation: bool,

    /// A flag indicating whether fragmentation calculation is required for the **prob requests**.
    pub is_frag_needed: bool,

    pub reservation_store: ReservationStore,
    pub simulator: Arc<GlobalClock>,
}

impl<S: SlottedScheduleStrategy> SlottedScheduleContext<S> {
    pub fn new(
        id: SlottedScheduleId,
        number_of_real_slots: i64,
        slot_width: i64,
        capacity: i64,
        use_quadratic_mean_fragmentation: bool,
        strategy: S,
        reservation_store: ReservationStore,
        simulator: Arc<GlobalClock>,
    ) -> Self {
        let mut slots: Vec<Slot> = Vec::new();

        // number_of_real_slots is the number of slots in the considered scheduling window
        // slot_width = 1h number_of_real_slots = 10 -> 10h scheduling window
        for _ in 0..number_of_real_slots {
            slots.push(Slot::new(capacity));
        }

        let mut slotted_context = SlottedScheduleContext {
            strategy,
            id: SlottedScheduleId::new(id),
            slots: slots,
            slot_width: slot_width,
            start_slot_index: simulator.get_system_time_s(),
            end_slot_index: number_of_real_slots,
            scheduling_window_start_time: simulator.get_system_time_s(),
            scheduling_window_end_time: (number_of_real_slots * slot_width) - 1, // End of the valid scheduling window
            load_buffer: LoadBuffer::new(Arc::new(GlobalLoadContext::new())),
            active_reservations: Reservations::new_empty(reservation_store.clone()),
            is_frag_cache_up_to_date: true,
            fragmentation_cache: 0.0,
            use_quadratic_mean_fragmentation: use_quadratic_mean_fragmentation,
            is_frag_needed: false,
            reservation_store,
            simulator,
        };

        slotted_context.update();

        return slotted_context;
    }

    pub fn clear(&mut self) {
        log::warn!("In SlottedSchedule id: {}, where all Slots cleared.", self.id);

        for slot in self.slots.iter_mut() {
            slot.reset();
        }

        self.active_reservations.clear();
    }

    /// Computes a  **real index** in `slots` to a corresponding **virtual slot index** in the
    /// overall schedule timeline.
    pub fn get_real_slot_index(&self, index: i64) -> i64 {
        return (index % (self.slots.len() as i64)) as i64;
    }

    /// Retrieves the `Slot` corresponding to the given **virtual index**, if it exists within the current window.
    /// The index is virtual, meaning it represents a point in the schedule's timeline, and must be
    /// mapped to a real index in the internal slot vector via `get_real_slot_index`.
    pub fn get_slot(&self, index: i64) -> Option<&Slot> {
        if index < 0 {
            return None;
        }

        if index < self.start_slot_index || index > self.end_slot_index {
            return None;
        }

        let real_index: i64 = self.get_real_slot_index(index);
        return self.slots.get(real_index as usize);
    }

    /// Retrieves the `Slot` corresponding to the given **virtual index**, if it exists within the current window.
    /// The index is virtual, meaning it represents a point in the schedule's timeline, and must be
    /// mapped to a real index in the internal slot vector via `get_real_slot_index`.
    pub fn get_mut_slot(&mut self, index: i64) -> Option<&mut Slot> {
        if index < 0 {
            return None;
        }

        if index < self.start_slot_index || index > self.end_slot_index {
            return None;
        }

        let real_index: i64 = self.get_real_slot_index(index);
        return self.slots.get_mut(real_index as usize);
    }

    /// Calculates the **virtual index** of the time slot that contains the given point in time.
    ///
    /// This index is an abstract representation based on the `slot_width` and may exceed the actual number
    /// of allocated slots (real slots) (`self.slots.len()`).
    ///
    /// **Note:** A negative input time will always yield an index of $0$.
    pub fn get_slot_index(&self, time: i64) -> i64 {
        let index: i64 = (time as f64 / self.slot_width as f64).floor() as i64;

        if index < 0 {
            log::debug!("The requested slot index is negative ({}), because the requested time was negative: {}", index, time,);

            return 0;
        }

        return index;
    }

    /// Computes the **absolute start time** in seconds of a virtual slot.
    pub fn get_slot_start_time(&self, index: i64) -> i64 {
        return index * self.slot_width;
    }

    /// Computes the **absolute end time** in seconds of a virtual slot.
    pub fn get_slot_end_time(&self, index: i64) -> i64 {
        return index * self.slot_width + self.slot_width - 1;
    }

    /// Limits a given **virtual slot index** to ensure it is bounded by the current schedule window.
    /// This is used to constrain searches and operations to the slots that are currently managed
    /// and tracked by the schedule, preventing out-of-bounds index access in the virtual timeline.
    pub fn get_effective_slot_index(&self, slot_index: i64) -> i64 {
        let mut effective_slot_index: i64 = slot_index;

        if effective_slot_index < self.start_slot_index {
            effective_slot_index = self.start_slot_index;
        }

        if effective_slot_index > self.end_slot_index {
            effective_slot_index = self.end_slot_index;
        }

        return effective_slot_index;
    }
    /// **Updates the scheduling window** by advancing the internal time pointers based on the current simulation time.
    ///
    /// This process deletes all reservations that have expired (assigned end time is past the new start time)
    /// and moves the load from the now-expired slots into the `load_buffer` for historical tracking.
    /// Note: Utilized by the SlottedSchedule and NetworkSlottedSchedule
    /// Optimization: This functions is prior to every probe and reserve request called
    pub fn update(&mut self) {
        let current_time = self.simulator.get_system_time_s();
        let new_start_slot_index = self.get_slot_index(current_time);
        let effective_cleanup_end = new_start_slot_index.min(self.end_slot_index + 1);

        if self.start_slot_index < new_start_slot_index {
            self.is_frag_cache_up_to_date = false;
        }

        // key are used to: remove reservation which end earlier than the new start time
        let mut ids_to_remove: HashSet<ReservationId> = HashSet::new();

        for clean_index in self.start_slot_index..effective_cleanup_end {
            if let Some(slot) = self.get_slot(clean_index) {
                for id in &slot.reservation_ids {
                    let last_slot_of_reservation = self.get_slot_index(self.reservation_store.get_assigned_end(id.clone()) - 1);
                    if last_slot_of_reservation == clean_index {
                        ids_to_remove.insert(id.clone());
                    }
                }
            }
        }

        for key in ids_to_remove {
            self.active_reservations.delete_reservation(&key);
        }

        for clean_index in self.start_slot_index..new_start_slot_index {
            let load = if let Some(slot) = self.get_slot(clean_index) { slot.load } else { 0 };
            self.load_buffer.add(load, clean_index);

            if let Some(slot) = self.get_mut_slot(clean_index) {
                slot.reset();
            } else {
                log::error!(
                    "In SlottedSchedule: {} Happened an error during the update process. Slots are now invalid due to fail reset of slot {}.",
                    self.id,
                    clean_index
                )
            }
        }

        // set new Pointer to start and end of the new scheduling window
        self.start_slot_index = new_start_slot_index;
        self.end_slot_index = new_start_slot_index + (self.slots.len() as i64) - 1;

        // set corresponding time borders for the scheduling window
        self.scheduling_window_start_time = self.get_slot_start_time(self.start_slot_index);
        self.scheduling_window_end_time = self.get_slot_end_time(self.end_slot_index);
    }

    /// Validates, if deletion of reservation is possible, sets reservation in state `ReservationState::Rejected` if
    /// Reservation was not reserved before deletion request
    /// Returns true, if deletion process an proceed otherwise false is returned
    pub fn is_reservation_valid_for_deletion(&mut self, id: ReservationId) -> bool {
        // Can not Del unreserved reservation
        if !self.active_reservations.contains_key(&id) {
            log::error!("DEL Reservation form Schedule: {}, However Schedule does not contain reservation with id: {:?}", self.id, id);
            self.reservation_store.update_state(id.clone(), ReservationState::Rejected);
            return false;
        }
        return true;
    }

    /// Deletes the provided ReservationId form the specified slot.
    pub fn delete_reservation_in_slot(&mut self, reservation_id: ReservationId, reservation_reserved_capacity: i64, slot_index: i64) -> bool {
        let slot = self.get_mut_slot(slot_index).expect("Slot was not found.");
        return slot.delete_reservation(reservation_id, reservation_reserved_capacity);
    }

    /// Performs the actual deletion of the reservation in the SlottedScheduleContext
    pub fn delete_reservation(&mut self, id: ReservationId) {
        let current_time = self.simulator.get_system_time_s();
        // Can not delete already finished reservations
        let task_finished: bool = self.reservation_store.get_assigned_end(id.clone()) <= current_time;

        if task_finished {
            log::error!("Can't deleted reservation {:?} form Schedule: {}, because reservation is already finished.", id, self.id,);
            return;
        }

        let del_res_assigned_start = self.reservation_store.get_assigned_start(id.clone());
        let del_res_assigned_end = self.reservation_store.get_assigned_end(id.clone());
        let del_res_reserved_capacity = self.reservation_store.get_reserved_capacity(id.clone());

        // Delete reservation from schedule
        if !self.active_reservations.delete_reservation(&id) {
            log::error!("Del reservation (id: {:?}) was not possible.", id);
            return;
        }

        // Delete reservation from all occupied slots
        let mut reservation_start_slot_index: i64 = self.get_slot_index(del_res_assigned_start);
        let reservation_end_slot_index: i64 = self.get_effective_slot_index(self.get_slot_index(del_res_assigned_end - 1));

        // Delete only parts that are in the scheduling window
        if reservation_start_slot_index < self.start_slot_index {
            reservation_start_slot_index = self.start_slot_index;
        }

        let slotted_schedule_id = self.id.clone();
        for slot_index in reservation_start_slot_index..=reservation_end_slot_index {
            let slot = self
                .get_mut_slot(slot_index)
                .expect(&format!("In the SlottedSchedule id: {} was the slot with index: {} not found.", slotted_schedule_id, slot_index));

            slot.delete_reservation(id.clone(), del_res_reserved_capacity);
        }

        self.is_frag_cache_up_to_date = false;
        return;
    }

    /// Checks if a given point in time falls within the schedule's defined **scheduling window**.
    pub fn is_time_in_scheduling_window(&self, time: i64) -> bool {
        if time > self.scheduling_window_end_time || time < self.scheduling_window_start_time {
            return false;
        }

        return true;
    }

    /// Retrieves the current resource load (reserved capacity) for a slot at a given index.
    /// **Note:** If the slot is not found, an error is logged, and **0** is returned.
    pub fn get_slot_load(&self, index: i64) -> i64 {
        match self.get_slot(index) {
            Some(slot) => slot.load,
            None => {
                log::error!(
                    "In the SlottedSchedule {} was of a with the index {} the load requested. However with the slot index exists not slot.",
                    self.id,
                    index,
                );
                return 0;
            }
        }
    }
}

impl<S: SlottedScheduleStrategy> SlottedScheduleContext<S> {
    /// Searches for all possible time slots in the schedule where the given reservation request can be fully satisfied.
    ///
    /// This method performs the core **scheduling probe** for resource availability. It iterates through
    /// possible start times within the request's booking interval, clips the search to the scheduling window,
    /// and check for feasibility.
    ///
    /// # Returns
    /// Returns a `Reservations` object containing a map of all feasible reservations (candidates) found.
    /// Each candidate represents a valid assignment time within the schedule's constraints.
    pub fn calculate_schedule(&mut self, id: ReservationId) -> ProbeReservations {
        let mut request_start_boundary: i64 = self.reservation_store.get_booking_interval_start(id.clone());
        let mut request_end_boundary: i64 = self.reservation_store.get_booking_interval_end(id.clone());
        let initial_duration: i64 = self.reservation_store.get_task_duration(id.clone());

        if request_start_boundary == i64::MIN {
            request_start_boundary = 0;
        }

        if request_end_boundary == i64::MIN {
            request_end_boundary = i64::MAX;
        }

        let mut search_results = ProbeReservations::new(id, self.reservation_store.clone());

        if request_start_boundary > request_end_boundary || request_start_boundary < 0 {
            log::error!(
                "ErrorSlottedScheduleContextInValidReservationStartAndEndRequest: The reservation with id {:?} has in valid start ({}) or end ({}).",
                id,
                request_start_boundary,
                request_end_boundary
            );
            return search_results;
        }

        if !self.reservation_store.is_moldable(id.clone())
            && S::get_capacity(self) > 0
            && S::get_capacity(self) < self.reservation_store.get_reserved_capacity(id.clone())
        {
            return search_results;
        }

        let mut earliest_start_index: i64 = self.get_slot_index(request_start_boundary);
        earliest_start_index = self.get_effective_slot_index(earliest_start_index);

        let mut latest_start_index: i64 = self.get_slot_index(request_end_boundary - initial_duration);
        latest_start_index = self.get_effective_slot_index(latest_start_index);

        for slot_start_index in earliest_start_index..=latest_start_index {
            if let Some(res_candidate) = self.try_fit_reservation(id, slot_start_index, request_end_boundary) {
                search_results.add_reservation(res_candidate);
            }
        }
        return search_results;
    }

    fn try_fit_reservation(&mut self, candidate_id: ReservationId, slot_start_index: i64, request_end_boundary: i64) -> Option<Reservation> {
        let mut candidate =
            self.reservation_store.get_reservation_snapshot(candidate_id.clone()).expect("ReservationStore snapshot should handle potential errors.");

        let mut current_required_capacity = self.reservation_store.get_reserved_capacity(candidate_id.clone());
        let mut current_duration: i64 = self.reservation_store.get_task_duration(candidate_id.clone());
        let mut start_time = self.get_slot_start_time(slot_start_index);

        let booking_interval_start = candidate.get_booking_interval_start();

        if start_time < booking_interval_start {
            start_time = booking_interval_start;
        }

        let mut is_feasible: bool = true;
        let mut end_time = start_time + current_duration;
        let mut current_end_slot_index = self.get_slot_index(end_time - 1);
        let mut current_slot_index: i64 = slot_start_index;

        while current_slot_index <= current_end_slot_index {
            let available_capacity: i64 = S::adjust_requirement_to_slot_capacity(self, current_slot_index, current_required_capacity, candidate_id);

            if available_capacity == 0 && current_required_capacity != 0 {
                is_feasible = false;
                break;
            }

            if !candidate.is_moldable() && available_capacity != current_required_capacity {
                is_feasible = false;
                break;
            }

            if available_capacity < current_required_capacity {
                candidate.adjust_capacity(available_capacity);
                current_required_capacity = available_capacity;
                current_duration = candidate.get_task_duration();

                end_time = start_time + current_duration;

                if false == self.is_time_in_scheduling_window(end_time) || end_time > request_end_boundary {
                    is_feasible = false;
                    break;
                }

                current_end_slot_index = self.get_slot_index(end_time - 1);
            }

            current_slot_index += 1;
        }

        if is_feasible {
            candidate.set_booking_interval_start(start_time);
            candidate.set_booking_interval_end(end_time);
            candidate.set_assigned_start(start_time);
            candidate.set_assigned_end(end_time);
            candidate.set_state(ReservationState::ProbeReservation);
            return Some(candidate);
        }

        return None;
    }

    /// Updates the total resource capacity for all time slots within the schedule.
    ///
    /// This method performs a global capacity adjustment across all slots of the schedule.
    /// If a slot's current **occupied load** exceeds the requested `capacity`, the system
    /// will aggressively prune existing reservations from that slot until the load
    /// satisfies the new constraint.
    ///
    /// # Side Effects
    /// * **Reservation Eviction**: Reservations are removed based on their presence in
    ///   the slot's internal tracking list until the load is lowered.
    pub fn update_capacity(&mut self, capacity: usize) {
        let new_capacity = capacity as i64;

        for slot in &mut self.slots {
            if slot.load < new_capacity {
                slot.capacity = new_capacity;
                continue;
            }

            // Start reservation pruning
            for res_in_slot in slot.reservation_ids.clone().iter() {
                if !slot.delete_reservation(res_in_slot.clone(), self.reservation_store.get_reserved_capacity(res_in_slot.clone())) {
                    log::error!(
                        "ErrorSlottedScheduleContextUpdateCapacity: In schedule {:?} with reservation {:?}",
                        self.id,
                        self.reservation_store.get_name_for_key(res_in_slot.clone())
                    );
                }

                // Success
                if slot.load < new_capacity {
                    slot.capacity = new_capacity;
                    break;
                }
            }
        }
    }
}
