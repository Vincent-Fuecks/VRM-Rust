use std::fmt::Debug;

use crate::domain::vrm_system_model::utils::id::ReservationName;

use super::reservation::ReservationState;
use super::reservation_store::ReservationId;

/// Defines the contract for entities that must react to reservation lifecycle events.
///
/// Allows components, that implemented this trait to react to changes.
pub trait ReservationNotificationListener: Send + Sync + Debug {
    fn on_reservation_change(
        &mut self,
        reservation_id: ReservationId,
        res_name: ReservationName,
        old_state: ReservationState,
        new_state: ReservationState,
    );
}
