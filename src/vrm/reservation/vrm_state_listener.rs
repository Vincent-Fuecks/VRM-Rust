use std::{collections::HashSet, sync::Arc};

use parking_lot::RwLock;

use crate::domain::vrm_system_model::utils::id::ReservationName;

use super::reservation::ReservationState;
use super::reservation_notification_listener::ReservationNotificationListener;
use super::reservation_store::ReservationId;
/// A listener responsible for maintaining a synchronized view of active reservations
/// within the VRM.
///
/// The `VrmStateListener` implements a **Subscription (Abo)** pattern. It monitors
/// state transitions from the `ReservationStore` and ensures that the local `open_reservations`
/// set accurately reflects the distributed state of the grid. It specifically handles
/// the lifecycle of resource allocations by removing IDs when they reach terminal
/// states like **Deleted**, **Rejected**, or **Finished**.
#[derive(Debug)]
pub struct VrmStateListener {
    open_reservations: Arc<RwLock<HashSet<ReservationId>>>,
}

impl VrmStateListener {
    pub fn new(open_reservations: Arc<RwLock<HashSet<ReservationId>>>) -> Self {
        Self { open_reservations }
    }

    pub fn new_empty() -> Self {
        Self { open_reservations: Arc::new(RwLock::new(HashSet::new())) }
    }

    pub fn add(&mut self, reservation_id: ReservationId) -> bool {
        let mut guard = self.open_reservations.write();
        guard.insert(reservation_id)
    }
}

impl ReservationNotificationListener for VrmStateListener {
    fn on_reservation_change(
        &mut self,
        reservation_id: ReservationId,
        res_name: ReservationName,
        old_state: ReservationState,
        new_state: ReservationState,
    ) {
        match new_state {
            ReservationState::Open => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
            }
            ReservationState::ProbeAnswer => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
            }
            ReservationState::ReserveAnswer => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
            }
            ReservationState::ProbeReservation => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
            }
            ReservationState::ReserveProbeReservation => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
            }
            ReservationState::Committed => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
            }
            ReservationState::Deleted => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
                let mut guard = self.open_reservations.write();
                guard.remove(&reservation_id);
            }
            ReservationState::Rejected => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
                let mut guard = self.open_reservations.write();
                guard.remove(&reservation_id);
            }
            ReservationState::Finished => {
                log::info!("Reservation {:?} finished successfully.", reservation_id);
                let mut guard = self.open_reservations.write();
                guard.remove(&reservation_id);
            }
            ReservationState::External => {
                log::info!("State Change of Reservation ID: {:?} | Name: {:?} | {:?}->{:?}", reservation_id, res_name, old_state, new_state);
            }
        }
    }
}
