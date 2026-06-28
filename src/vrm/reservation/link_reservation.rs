use std::any::Any;

use serde::{Deserialize, Serialize};

use crate::vrm::commons::id::RouterId;

use super::reservation::{ReservationBase, ReservationTrait, ReservationTyp};
/// This structure extends [`ReservationBase`] to include fields specific to
/// network connectivity.
///
/// Link reservations typically have two use cases:
/// 1. **Data Transfer:** Reserving bandwidth for file transfer between two sites.
///    In this case, the reservation may be **moldable**, meaning the duration
///    can be adjusted based on available bandwidth.
/// 2. **Co-allocated Communication:** Reserving a specific, fixed amount of
///    bandwidth for short-term coordination and communication between tasks
///    associated with co-allocated compute reservations. The specified bandwidth
///    **must** be provided for the entire duration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkReservation {
    /// The common base properties shared by all reservations.
    pub base: ReservationBase,

    // Link specific fields
    /// Unique identifier of the start router for the link.
    pub start_point: Option<RouterId>,
    /// Unique identifier of the end router for the link.
    pub end_point: Option<RouterId>,
}

impl LinkReservation {
    pub fn get_start_point(&self) -> Option<RouterId> {
        self.start_point.clone()
    }

    pub fn get_end_point(&self) -> Option<RouterId> {
        self.end_point.clone()
    }

    pub fn set_start_point(&mut self, start_point: Option<RouterId>) {
        self.start_point = start_point;
    }

    pub fn set_end_point(&mut self, end_point: Option<RouterId>) {
        self.end_point = end_point;
    }
}

impl ReservationTrait for LinkReservation {
    fn get_base(&self) -> &ReservationBase {
        &self.base
    }

    fn get_base_mut(&mut self) -> &mut ReservationBase {
        &mut self.base
    }

    fn box_clone(&self) -> Box<dyn ReservationTrait> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_type(&self) -> ReservationTyp {
        ReservationTyp::Link
    }
}
