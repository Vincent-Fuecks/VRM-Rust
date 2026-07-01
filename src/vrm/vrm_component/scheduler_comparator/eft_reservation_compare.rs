use std::cmp::Ordering;

use crate::vrm::reservation::reservation_store::{ReservationId, ReservationStore};

pub struct EFTReservationCompare {
    reservation_store: ReservationStore,
}

impl EFTReservationCompare {
    pub fn new(reservation_store: ReservationStore) -> Self {
        Self { reservation_store }
    }

    pub fn compare(&self, reservation_id0: ReservationId, reservation_id1: ReservationId) -> Ordering {
        let assigned_end0 = self.reservation_store.get_assigned_end(reservation_id0);
        let assigned_end1 = self.reservation_store.get_assigned_end(reservation_id1);

        assigned_end0.partial_cmp(&assigned_end1).unwrap()
    }
}

// pub struct EFTReservationCompare {
//     reservation_store: ReservationStore,
// }

// impl EFTReservationCompare {
//     pub fn new(reservation_store: ReservationStore) -> Self {
//         Self { reservation_store }
//     }

//     pub fn compare(&self, res0: Reservation, res1: Reservation) -> Ordering {
//         let assigned_end0 = res0.get_base_reservation().get_assigned_end();
//         let assigned_end1 = res1.get_base_reservation().get_assigned_end();

//         return assigned_end0.partial_cmp(&assigned_end1).unwrap();
//     }
// }
