use crate::vrm::reservation::reservation_store::ReservationId;
use std::fmt::Debug;

use crate::vrm::commons::load_buffer::LoadMetric;
use crate::vrm::schedule::slotted_schedule::slotted_schedule_context::SlottedScheduleContext;
pub trait SlottedScheduleStrategy: Send + Sync + Debug + Clone + Sized + 'static {
    fn adjust_requirement_to_slot_capacity(
        ctx: &SlottedScheduleContext<Self>,
        slot_index: i64,
        requirement: i64,
        reservation_id: ReservationId,
    ) -> i64;

    fn insert_reservation_into_slot(ctx: &mut SlottedScheduleContext<Self>, requirement: i64, slot_index: i64, reservation_id: ReservationId);

    fn on_delete_reservation(ctx: &mut SlottedScheduleContext<Self>, reservation_id: ReservationId) -> bool;

    fn on_clear(ctx: &mut SlottedScheduleContext<Self>);

    fn get_fragmentation(ctx: &mut SlottedScheduleContext<Self>, frag_start_time: i64, frag_end_time: i64) -> f64;

    fn get_load_metric(ctx: &SlottedScheduleContext<Self>, start_time: i64, end_time: i64) -> LoadMetric;

    fn get_simulation_load_metric(ctx: &mut SlottedScheduleContext<Self>) -> LoadMetric;

    fn get_system_fragmentation(ctx: &mut SlottedScheduleContext<Self>) -> f64;

    fn get_capacity(ctx: &SlottedScheduleContext<Self>) -> i64;

    /// Returns the fragmentation power exponent used in quadratic mean fragmentation calculation.
    /// Higher values give more weight to large free blocks. Default is 2.0.
    fn get_fragmentation_power() -> f64 {
        2.0
    }
}
