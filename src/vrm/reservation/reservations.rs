use rand::seq::IndexedRandom;
use std::collections::hash_set;
use std::{collections::HashSet, i64};

use crate::vrm::reservation::reservation::ReservationState;

use super::reservation_store::{ReservationId, ReservationStore};

/// This structure tracks a local subset of active `ReservationId`s while maintaining
/// a reference to the global `ReservationStore` for metadata persistence and
/// state synchronization for the schedule.
#[derive(Debug, Clone)]
pub struct Reservations {
    reservations: HashSet<ReservationId>,
    reservation_store: ReservationStore,
}

impl Reservations {
    pub fn new_empty(reservation_store: ReservationStore) -> Self {
        Reservations { reservations: HashSet::new(), reservation_store }
    }

    /// Clears all local reservation mappings.
    ///
    /// **Note:** This does not purge the data from the global `ReservationStore`,
    /// but removes the scheduler's tracking interest in these IDs.
    pub fn clear(&mut self) {
        self.reservations = HashSet::new();
    }

    /// Inserts a `ReservationId` into the local management set.
    /// This operation ensures the reservation is tracked for scheduling logic.
    /// Returns `true` if the ID was newly inserted, `false` if it was already present.
    pub fn insert(&mut self, id: ReservationId) -> bool {
        if !self.reservations.insert(id) {
            log::error!(
                "ErrorSchedulerReservationWasSubmittedMultipleTimes: The Reservation {:?} was already present in the schedule.",
                self.reservation_store.get_name_for_key(id)
            );
            return false;
        }
        true
    }

    /// Deletes a reservation from the local set and updates the global state to `Deleted`.
    /// This effectively cancels the reservation and notifies the distributed store
    /// that the resources associated with this ID are no longer reserved.
    pub fn delete_reservation(&mut self, id: &ReservationId) -> bool {
        if self.reservations.remove(id) {
            log::debug!("Reservation was updated to ReservationState::Deleted, by the schedule.");
            self.reservation_store.update_state(*id, ReservationState::Deleted);
            return true;
        }
        return false;
    }

    /// Checks if a specific `ReservationId` is currently managed in this collection.
    pub fn contains_key(&self, id: &ReservationId) -> bool {
        self.reservations.contains(id)
    }

    /// Returns the number of reservations currently tracked by this local manager.
    pub fn len(&self) -> usize {
        self.reservations.len()
    }

    /// Returns `true` if no reservations are currently being tracked.
    pub fn is_empty(&self) -> bool {
        self.reservations.is_empty()
    }

    /// Selects a random `ReservationId` from the current collection.
    pub fn get_random_id(&self) -> Option<ReservationId> {
        let ids: Vec<ReservationId> = self.reservations.iter().into_iter().cloned().collect();
        let mut rng = rand::rng();

        return ids.choose(&mut rng).cloned();
    }

    /// Identifies the reservation with the earliest assigned start time.
    /// This is an O(n) operation used to determine the next pending task in the queue.
    pub fn get_id_with_first_start_slot(&self) -> Option<ReservationId> {
        let mut earliest_start_time: i64 = i64::MAX;
        let mut reservation_of_earliest_start_time = None;

        for id in self.reservations.iter() {
            let start_time = self.reservation_store.get_assigned_start(*id);
            if start_time < earliest_start_time {
                earliest_start_time = start_time;
                reservation_of_earliest_start_time = Some(*id);
            }
        }
        return reservation_of_earliest_start_time;
    }

    /// Returns an iterator over the managed `ReservationId`s.
    pub fn iter(&self) -> hash_set::Iter<'_, ReservationId> {
        self.reservations.iter()
    }
}
