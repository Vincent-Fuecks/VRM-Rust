use crate::domain::vrm_system_model::{
    resource::resource_store::ResourceStore,
    schedule::slotted_schedule::{
        slotted_schedule_context::SlottedScheduleContext,
        strategy::{
            link::topology::{NetworkTopology, Path},
            node::node_strategy::NodeStrategy,
            strategy_trait::SlottedScheduleStrategy,
        },
    },
    utils::load_buffer::LoadMetric,
};
use crate::vrm::reservation::{reservation::ReservationState, reservation_store::ReservationId};
use std::collections::HashMap;

/// Creates the schedule for Networks like RmsNetworkSimulator, SLURM etc.
/// Shares with the SlottedSchedule the SlottedScheduleContext and multiple other function
/// of the implemented Schedule trait.
#[derive(Debug, Clone)]
pub struct LinkStrategy {
    pub topology: NetworkTopology,
    pub reserved_paths: HashMap<ReservationId, HashMap<i64, Path>>,
    pub resource_store: ResourceStore,
    pub max_bandwidth_all_paths: i64,
}

impl LinkStrategy {
    pub fn new(topology: NetworkTopology, resource_store: ResourceStore) -> Self {
        let max_bandwidth_all_paths = topology.max_bandwidth_all_paths;
        Self { topology, reserved_paths: HashMap::new(), resource_store, max_bandwidth_all_paths }
    }
}

impl SlottedScheduleStrategy for LinkStrategy {
    fn get_capacity(ctx: &SlottedScheduleContext<Self>) -> i64 {
        ctx.strategy.max_bandwidth_all_paths
    }
    /// Calculates the maximum assignable capacity for a reservation within a specific network time slot.
    ///  
    /// ### Algorithm Logic
    /// 1. Identifies the **source** and **target** nodes for the given `reservation_id`.
    /// 2. Retrieves pre-calculated paths from the **Topology Path Cache**.
    /// 3. For each path, it performs a "bottleneck analysis" preformed
    /// 4. Returns the full requested capacity if at least one path can satisfy it entirely.
    ///    Otherwise, returns the maximum available partial capacity found across all evaluated paths.
    ///
    /// ### Parameters
    /// * `slot_index`: The Requested slot of the SlottedScheduleContext.
    /// * `reservation_id`: Unique identifier for the Reservation.
    ///
    /// ### Returns
    /// * An `i64` representing the **maximum assignable capacity**.
    /// * `i64` - The maximum assignable capacity. Returns `0` if no connectivity exists or all paths are saturated.
    fn adjust_requirement_to_slot_capacity(
        ctx: &SlottedScheduleContext<Self>,
        slot_index: i64,
        _requirement: i64,
        reservation_id: ReservationId,
    ) -> i64 {
        let start = ctx.reservation_store.get_start_point(reservation_id);
        let end = ctx.reservation_store.get_end_point(reservation_id);

        let available_paths = if let (Some(ref source), Some(ref target)) = (start, end) {
            match ctx.strategy.topology.path_cache.get(&(source.clone(), target.clone())) {
                Some(paths) => paths,
                None => {
                    log::error!(
                        "LinkStrategy::adjust_requirement_to_slot_capacity: No cached paths found between {:?} and {:?} for reservation {:?}.",
                        source,
                        target,
                        reservation_id
                    );
                    return 0;
                }
            }
        } else {
            // No Path between source and target found
            return 0;
        };

        let mut available_capacity = 0;

        // Check if all links can handle the requested capacity

        // Iterate through the K-Shortest Paths
        for path in available_paths {
            // Init with capacity of first link
            let path_first_link_id = path.network_links.first().unwrap();

            let mut path_available_capacity = ctx.strategy.resource_store.with_mut_slotted_schedule_strategy(*path_first_link_id, |schedule| {
                NodeStrategy::adjust_requirement_to_slot_capacity(schedule, slot_index, LinkStrategy::get_capacity(ctx), reservation_id)
            });

            // Check if all links can handle the requested capacity
            for link_id in &path.network_links {
                path_available_capacity = ctx.strategy.resource_store.with_mut_slotted_schedule_strategy(*link_id, |schedule| {
                    NodeStrategy::adjust_requirement_to_slot_capacity(schedule, slot_index, path_available_capacity, reservation_id)
                });

                if path_available_capacity == 0 {
                    break;
                }

                if path_available_capacity < 0 {
                    log::error!("path_available_capacity is below zero should never happen.")
                }
            }
            // Path has enough for the whole capacity
            if path_available_capacity == ctx.strategy.max_bandwidth_all_paths {
                return ctx.strategy.max_bandwidth_all_paths;
            } else if path_available_capacity > available_capacity {
                available_capacity = path_available_capacity
            }
        }

        return available_capacity;
    }

    fn insert_reservation_into_slot(ctx: &mut SlottedScheduleContext<Self>, _requirement: i64, slot_index: i64, reservation_id: ReservationId) {
        let start = ctx.reservation_store.get_start_point(reservation_id);
        let end = ctx.reservation_store.get_end_point(reservation_id);

        let k_shortest_paths = if let (Some(ref source), Some(ref target)) = (start.clone(), end.clone()) {
            match ctx.strategy.topology.path_cache.get(&(source.clone(), target.clone())) {
                Some(paths) => paths,
                None => {
                    log::error!(
                        "NetworkPolicyInsertReservationInSlot: No cached paths found between {:?} and {:?} for reservation {:?}.",
                        source,
                        target,
                        reservation_id
                    );
                    return;
                }
            }
        } else {
            // No Path between source and target found
            log::debug!(
                "NetworkPolicyInsertReservationInSlot: Inserting Reservation {:?} into slot {} failed by NetworkPolicy. Because there was no valid path between Source {:?} and Target {:?} found.",
                ctx.reservation_store.get_name_for_key(reservation_id),
                slot_index,
                start,
                end
            );
            return;
        };

        for path in k_shortest_paths {
            // First test if there is a path free
            let mut free = true;
            for link_id in &path.network_links {
                let link_reserved_capacity = ctx.reservation_store.get_reserved_capacity(reservation_id);

                let path_available_capacity = ctx.strategy.resource_store.with_mut_slotted_schedule_strategy(*link_id, |schedule| {
                    NodeStrategy::adjust_requirement_to_slot_capacity(schedule, slot_index, link_reserved_capacity, reservation_id)
                });

                if path_available_capacity != link_reserved_capacity {
                    free = false;
                    break;
                }
            }

            if free {
                // Found path -> register reservation
                for link_id in &path.network_links {
                    let link_reserved_capacity = ctx.reservation_store.get_reserved_capacity(reservation_id);

                    ctx.strategy.resource_store.with_mut_slotted_schedule_strategy(*link_id, |schedule| {
                        NodeStrategy::insert_reservation_into_slot(schedule, link_reserved_capacity, slot_index, reservation_id)
                    });
                }

                // Remember path for reservation and slot
                ctx.strategy
                    .reserved_paths
                    .entry(reservation_id)
                    .or_insert_with(|| {
                        log::debug!(
                            "NetworkSchedule add new Reservation/Slot/Path object for {:?}",
                            ctx.reservation_store.get_name_for_key(reservation_id)
                        );

                        HashMap::new()
                    })
                    .insert(slot_index, path.clone());

                // Book reserved capacity of Reservation in Slot for Link
                let capacity = ctx.reservation_store.get_reserved_capacity(reservation_id);
                if let Some(slot) = ctx.get_mut_slot(slot_index) {
                    slot.insert_reservation(capacity, reservation_id);
                }

                return;
            }
        }

        log::error!(
            "NetworkSlottedScheduleInsertReservationFailed: Insert Reservation {:?} failed, because committed reservation has no available path in slot index {}.",
            ctx.reservation_store.get_name_for_key(reservation_id),
            slot_index
        );
    }

    fn on_clear(ctx: &mut SlottedScheduleContext<Self>) {
        log::debug!("Clear NetworkSlottedSchedule {}", ctx.id);

        for link_id in &ctx.strategy.topology.link_ids {
            ctx.strategy.resource_store.with_mut_slotted_schedule_strategy(*link_id, |schedule| schedule.clear());
        }

        ctx.strategy.reserved_paths.clear();
    }

    /// Deletes the reserved capacity of the booked path form all affected Links.
    /// Returns true, if the deletion clean up process was a success otherwise return false.
    fn on_delete_reservation(ctx: &mut SlottedScheduleContext<Self>, reservation_id: ReservationId) -> bool {
        let path_per_slot = if let Some(value) = ctx.strategy.reserved_paths.remove(&reservation_id) {
            value
        } else {
            log::error!(
                "NetworkScheduleDeleteReservationFailed: Deletion of booked path of Reservation {:?} failed.",
                ctx.reservation_store.get_name_for_key(reservation_id)
            );

            ctx.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            return false;
        };

        // For each time slot resolve the booked path
        for (slot_index, path) in path_per_slot {
            for link_id in &path.network_links {
                let reserved_capacity = ctx.reservation_store.get_reserved_capacity(reservation_id);
                if let Some(slot) = ctx.get_mut_slot(slot_index) {
                    slot.delete_reservation(reservation_id, reserved_capacity);
                } else {
                    log::error!(
                        "NetworkPolicyDeletionOfReservationFailed: The network path deletion of Reservation {:?} failed. slot_index: {}, path: {:?} the link_id which failed of the processed path {:?}. This link should be empty but part of it is still occupied.",
                        ctx.reservation_store.get_name_for_key(reservation_id),
                        slot_index,
                        path,
                        link_id
                    );
                    return false;
                }
            }
        }
        return true;
    }

    /// Note: LinkStrategy does not currently implement fragmentation analysis for network schedules.
    /// Unimplemented:
    fn get_fragmentation(_ctx: &mut SlottedScheduleContext<Self>, _frag_start_time: i64, _frag_end_time: i64) -> f64 {
        log::warn!("LinkStrategy::get_fragmentation is not yet implemented for network schedules. Returning -1.0.");
        -1.0
    }

    /// Note: LinkStrategy does not currently implement system-wide fragmentation for network schedules.
    /// Unimplemented:
    fn get_system_fragmentation(_ctx: &mut SlottedScheduleContext<Self>) -> f64 {
        log::warn!("LinkStrategy::get_system_fragmentation is not yet implemented for network schedules. Returning -1.0.");
        -1.0
    }

    /// Note: LinkStrategy does not currently implement load metrics for network schedules.
    /// Unimplemented:
    fn get_load_metric(_ctx: &SlottedScheduleContext<Self>, _start_time: i64, _end_time: i64) -> LoadMetric {
        log::warn!("LinkStrategy::get_load_metric is not yet implemented for network schedules. Returning zeroed LoadMetric.");
        LoadMetric::new(-1, -1, -1.0, -1.0, 0.0)
    }

    /// Note: LinkStrategy does not currently implement simulation load metrics for network schedules.
    /// Unimplemented:
    fn get_simulation_load_metric(_ctx: &mut SlottedScheduleContext<Self>) -> LoadMetric {
        log::warn!("LinkStrategy::get_simulation_load_metric is not yet implemented for network schedules. Returning zeroed LoadMetric.");
        LoadMetric::new(-1, -1, -1.0, -1.0, 0.0)
    }
}
