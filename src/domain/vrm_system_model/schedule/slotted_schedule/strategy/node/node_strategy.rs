use crate::domain::vrm_system_model::schedule::slotted_schedule::{
    slotted_schedule_context::SlottedScheduleContext, strategy::strategy_trait::SlottedScheduleStrategy,
};
use crate::vrm::commons::load_buffer::{LoadMetric, SLOTS_TO_DROP_ON_END, SLOTS_TO_DROP_ON_START};
use crate::vrm::reservation::reservation_store::ReservationId;

#[derive(Debug, Clone, Default)]
pub struct NodeStrategy {}

impl SlottedScheduleStrategy for NodeStrategy {
    fn get_capacity(ctx: &SlottedScheduleContext<Self>) -> i64 {
        ctx.slots.first().map(|s| s.capacity).unwrap_or(0)
    }

    fn on_clear(_ctx: &mut SlottedScheduleContext<Self>) {}
    /// Adjusts the requested resource requirement (**capacity**) to ensure it does not exceed the
    /// **remaining available capacity** in a specific slot.
    /// If the requested capacity is too high, the maximum available capacity for that slot is returned.
    fn adjust_requirement_to_slot_capacity(
        ctx: &SlottedScheduleContext<Self>,
        slot_index: i64,
        requirement: i64,
        reservation_id: ReservationId,
    ) -> i64 {
        if let Some(slot) = ctx.get_slot(slot_index) {
            return slot.get_adjust_requirement(requirement);
        } else {
            log::error!(
                "SlottedSchedule: {}: requested slot outside of scheduling window. Slot index: {}, window start: {}  window width: {} ReservationId: {:?}",
                ctx.id,
                slot_index,
                ctx.start_slot_index,
                ctx.slots.len() as i64,
                reservation_id,
            );

            return 0;
        }
    }

    fn get_fragmentation(ctx: &mut SlottedScheduleContext<Self>, frag_start_time: i64, frag_end_time: i64) -> f64 {
        ctx.update();
        // Normalize sentinel value: i64::MIN indicates an unset boundary (treat as "no end restriction").
        let frag_end_time = if frag_end_time == i64::MIN { i64::MAX } else { frag_end_time };

        if frag_end_time <= frag_start_time && frag_end_time != i64::MAX {
            log::error!(
                "Request to get fragmentation of Schedule: {}, the fragmentation start time {} was before the fragmentation end time {}.",
                ctx.id,
                frag_start_time,
                frag_end_time,
            )
        }

        let mut start_slot_index = ctx.get_slot_index(frag_start_time);
        start_slot_index = ctx.get_effective_slot_index(start_slot_index);

        let mut end_slot_index = ctx.get_slot_index(frag_end_time);
        end_slot_index = ctx.get_effective_slot_index(end_slot_index);

        if ctx.use_quadratic_mean_fragmentation {
            return ctx.get_fragmentation_quadratic_mean(start_slot_index, end_slot_index);
        }

        return ctx.get_fragmentation_resubmit(start_slot_index, end_slot_index);
    }

    fn get_system_fragmentation(ctx: &mut SlottedScheduleContext<Self>) -> f64 {
        if !ctx.is_frag_cache_up_to_date {
            ctx.fragmentation_cache = Self::get_fragmentation(ctx, ctx.scheduling_window_start_time, ctx.scheduling_window_end_time);
            ctx.is_frag_cache_up_to_date = true;
        }

        return ctx.fragmentation_cache;
    }

    fn get_load_metric(ctx: &SlottedScheduleContext<Self>, start_time: i64, end_time: i64) -> LoadMetric {
        // Normalize sentinel value: i64::MIN indicates an unset boundary (treat as "no end restriction").
        let end_time = if end_time == i64::MIN { i64::MAX } else { end_time };

        if end_time < start_time {
            log::error!("Start time must be before end time: SlottedSchedule id: {} is end_time: {} < start_time: {}", ctx.id, end_time, start_time)
        }

        let mut start_slot_nr = ctx.get_slot_index(start_time);
        start_slot_nr = ctx.get_effective_slot_index(start_slot_nr);

        let mut end_slot_nr = ctx.get_slot_index(end_time);
        end_slot_nr = ctx.get_effective_slot_index(end_slot_nr);

        let mut reserved_capacity_sum: i64 = 0;

        for slot_index in start_slot_nr..=end_slot_nr {
            reserved_capacity_sum += ctx.get_slot_load(slot_index);
        }

        let mut number_of_slots = 0;

        if ctx.slots.len() > 0 {
            number_of_slots = end_slot_nr - start_slot_nr + 1;
        }

        if number_of_slots < 0 {
            log::error!("The number of slots should never be negative.")
        }

        let avg_reserved_capacity: f64 =
            if number_of_slots != 0 { (reserved_capacity_sum as f64) / (number_of_slots as f64) } else { NodeStrategy::get_capacity(ctx) as f64 };

        LoadMetric {
            start_time,
            end_time,
            avg_reserved_capacity: avg_reserved_capacity,
            possible_capacity: NodeStrategy::get_capacity(ctx) as f64,
            utilization: avg_reserved_capacity / (NodeStrategy::get_capacity(ctx) as f64),
        }
    }

    fn get_simulation_load_metric(ctx: &mut SlottedScheduleContext<Self>) -> LoadMetric {
        let index_of_first_slot: i64 = ctx.load_buffer.context.get_first_load() + SLOTS_TO_DROP_ON_START;
        let start_time_of_first_slot: i64 = ctx.get_slot_start_time(index_of_first_slot);

        let index_of_last_slot: i64 = ctx.load_buffer.context.get_last_load() - SLOTS_TO_DROP_ON_END;
        let start_time_of_last_slot: i64 = ctx.get_slot_start_time(index_of_last_slot);

        return ctx.load_buffer.get_effective_overall_load(NodeStrategy::get_capacity(ctx) as f64, start_time_of_first_slot, start_time_of_last_slot);
    }

    /// Inserts a new reservation requirement into the specified slot.
    fn insert_reservation_into_slot(ctx: &mut SlottedScheduleContext<Self>, requirement: i64, slot_index: i64, reservation_id: ReservationId) {
        match ctx.get_mut_slot(slot_index) {
            Some(slot) => {
                slot.insert_reservation(requirement, reservation_id);
            }
            None => {
                log::error!(
                    "NodeStrategy::insert_reservation_into_slot: Slot not found at index {} for reservation {:?}. This indicates a programming error.",
                    slot_index,
                    reservation_id
                );
            }
        }
    }

    fn on_delete_reservation(_ctx: &mut SlottedScheduleContext<Self>, _reservation_id: ReservationId) -> bool {
        true
    }
}
