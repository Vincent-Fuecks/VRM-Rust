use crate::{vrm::schedule::{
        schedule_trait::Schedule,
        slotted_schedule::{slotted_schedule_context::SlottedScheduleContext, strategy::strategy_trait::SlottedScheduleStrategy},
    }, vrm::schedule::load_buffer::LoadMetric};
use crate::vrm::reservation::{
        probe_reservations::{ProbeReservationComparator, ProbeReservations},
        reservation::ReservationState,
        reservation_store::ReservationId,
    };

impl<S: SlottedScheduleStrategy> Schedule for SlottedScheduleContext<S> {
    fn clear(&mut self) {
        S::on_clear(self);
        SlottedScheduleContext::clear(self);
        SlottedScheduleContext::update(self);
    }

    fn clone_box(&self) -> Box<dyn Schedule> {
        Box::new(self.clone())
    }

    fn get_fragmentation(&mut self, frag_start_time: i64, frag_end_time: i64) -> f64 {
        S::get_fragmentation(self, frag_start_time, frag_end_time)
    }

    fn get_load_metric(&self, start_time: i64, end_time: i64) -> LoadMetric {
        S::get_load_metric(self, start_time, end_time)
    }

    fn get_load_metric_up_to_date(&mut self, start_time: i64, end_time: i64) -> LoadMetric {
        SlottedScheduleContext::update(self);
        S::get_load_metric(self, start_time, end_time)
    }

    fn get_simulation_load_metric(&mut self) -> LoadMetric {
        S::get_simulation_load_metric(self)
    }

    fn get_system_fragmentation(&mut self) -> f64 {
        S::get_system_fragmentation(self)
    }

    fn probe(&mut self, id: ReservationId) -> ProbeReservations {
        // Early Stop
        if self.reservation_store.get_reserved_capacity(id) < 0 {
            log::error!("SlottedScheduleContextProbeRequestNegativeReserveCapacity: The reserved capacity of Reservation {:?} is below zero.", id);
            self.reservation_store.update_state(id, ReservationState::Rejected);
        }

        SlottedScheduleContext::update(self);
        let mut candidates = self.calculate_schedule(id);
        self.reservation_store.update_state(id, ReservationState::ProbeAnswer);

        if candidates.is_empty() {
            return candidates;
        }

        let frag_before: f64 = self.get_system_fragmentation();
        if self.is_frag_needed {
            for candidate in candidates.get_mut_reservations() {
                let candidate_id = self.reservation_store.add_probe_reservation(candidate.clone());

                // Temporary Reserve
                self.is_frag_cache_up_to_date = false;
                self.reserve_without_check(candidate_id);

                let frag_delta: f64 = self.get_system_fragmentation() - frag_before;
                candidate.set_frag_delta(frag_delta);

                // Delete from Slots and ReservationStore
                SlottedScheduleContext::delete_reservation(self, candidate_id);
                self.reservation_store.delete_probe_reservation(candidate_id);
            }
        }

        return candidates;
    }

    fn probe_best(&mut self, request_id: ReservationId, probe_reservation_comparator: ProbeReservationComparator) -> ProbeReservations {
        // Early Stop
        if self.reservation_store.get_reserved_capacity(request_id) < 0 {
            log::error!(
                "SlottedScheduleContextProbeBestRequestNegativeReserveCapacity: The reserved capacity of Reservation {:?} is below zero.",
                request_id
            );
            self.reservation_store.update_state(request_id, ReservationState::Rejected);
            return ProbeReservations::new(request_id, self.reservation_store.clone());
        }

        let mut probe_reservations = self.probe(request_id);

        if probe_reservations.is_empty() {
            self.reservation_store.update_state(request_id, ReservationState::ProbeAnswer);
            return probe_reservations;
        }

        if let Some(best_probes) = probe_reservations.create_new_probe_reservation_with_best_probe(request_id, probe_reservation_comparator) {
            self.reservation_store.update_state(request_id, ReservationState::ProbeAnswer);
            return best_probes;
        } else {
            log::error!(
                "SlottedScheduleContextProbeBestRequestEmptyProbeReservationErrorInSelectBestProbeReservationLogic: Reservation {:?} on Schedule {:?}",
                request_id,
                self.id
            );

            self.reservation_store.update_state(request_id, ReservationState::Rejected);
            return probe_reservations;
        }
    }

    fn delete_reservation(&mut self, reservation_id: ReservationId) {
        if self.is_reservation_valid_for_deletion(reservation_id) {
            // Bring scheduling window up to date
            SlottedScheduleContext::update(self);
            // Delete Reservation from SlottedSchedule
            SlottedScheduleContext::delete_reservation(self, reservation_id);
        }
    }

    fn reserve(&mut self, reservation_id: ReservationId) -> Option<ReservationId> {
        // Early Stop
        if self.reservation_store.get_reserved_capacity(reservation_id) < 0 {
            log::error!(
                "SlottedScheduleContextReserveRequestNegativeReserveCapacity: The reserved capacity of Reservation {:?} is below zero.",
                reservation_id
            );
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
        }

        SlottedScheduleContext::update(self);

        let mut probe_reservations = self.calculate_schedule(reservation_id);
        if probe_reservations.is_empty() {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            return None;
        }

        if probe_reservations.only_promote_best(reservation_id, ProbeReservationComparator::ESTReservationCompare) {
            self.is_frag_cache_up_to_date = false;
            self.reserve_without_check(reservation_id);
            Some(reservation_id)
        } else {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            return None;
        }
    }

    fn reserve_without_check(&mut self, reservation_id: ReservationId) {
        // Early Stop
        if self.reservation_store.get_reserved_capacity(reservation_id) < 0 {
            log::error!(
                "SlottedScheduleContextReserveWithoutCheckRequestNegativeReserveCapacity: The reserved capacity of Reservation {:?} is below zero.",
                reservation_id
            );
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
        }

        let start_slot = self.get_slot_index(self.reservation_store.get_assigned_start(reservation_id.clone()));
        let end_slot = self.get_slot_index(self.reservation_store.get_assigned_end(reservation_id.clone()) - 1);

        for slot_index in start_slot..=end_slot {
            S::insert_reservation_into_slot(self, self.reservation_store.get_reserved_capacity(reservation_id), slot_index, reservation_id);
        }

        if !self.active_reservations.insert(reservation_id) {
            for slot_index in start_slot..=end_slot {
                if !self.delete_reservation_in_slot(reservation_id, self.reservation_store.get_reserved_capacity(reservation_id), slot_index) {
                    log::error!(
                        "SlottedScheduleContextReserveWithoutCheckCleanUpAfterFailedInsertionFailed: The prior insertion of the reservation {:?} ({:?}) with reserved capacity {:?} failed. 
                        The later invoked clean up process failed to deleted the booked slots of the reservation. 
                        The start slot: {:?}, end slot: {:?} and the slot index: {:?}.", 
                        self.reservation_store.get_name_for_key(reservation_id), 
                        reservation_id, 
                        self.reservation_store.get_reserved_capacity(reservation_id), 
                        start_slot, 
                        end_slot, 
                        slot_index
                    )
                }
            }
        }
        self.reservation_store.update_state(reservation_id, ReservationState::ReserveAnswer);
    }

    fn update(&mut self) {
        SlottedScheduleContext::update(self);
    }

    fn update_capacity(&mut self, capacity: usize) {
        SlottedScheduleContext::update_capacity(self, capacity);
    }
}
