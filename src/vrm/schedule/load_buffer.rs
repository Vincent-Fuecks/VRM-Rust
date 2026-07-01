use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

/// The number of slots to discard from the beginning of the measurement interval.
/// Acts as a "warm-up" period to avoid skewing data with initial system ramp-up.
pub const SLOTS_TO_DROP_ON_START: i64 = 50;

/// The number of slots to discard from the end of the measurement interval.
/// Acts as a "cool-down" period to ensure the tail of the simulation data is valid and consistent with the ring buffer logic.
pub const SLOTS_TO_DROP_ON_END: i64 = 50;

/// This context acts as a thread-safe synchronization point
/// to determine the global start and end times of activity across the AcI.
#[derive(Debug)]
pub struct GlobalLoadContext {
    /// The earliest slot index where load was observed globally.
    first_global_load: AtomicI64,

    /// The latest slot index where load was observed globally.
    last_global_load: AtomicI64,
}

impl Default for GlobalLoadContext {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalLoadContext {
    pub fn new() -> Self {
        Self { first_global_load: AtomicI64::new(i64::MAX), last_global_load: AtomicI64::new(-1) }
    }

    pub fn update_first_load(&self, index: i64) {
        // Ordering::Relaxed is sufficient here because:
        // 1. We only perform monotonic updates (fetch_min) - the value only decreases.
        // 2. There is no critical section that depends on the exact ordering of stores to first_global_load
        //    relative to other memory operations.
        // 3. The load values are only used for statistical/metrics purposes, not for correctness
        //    of scheduling decisions.
        self.first_global_load.fetch_min(index, Ordering::Relaxed);
    }

    pub fn update_last_load(&self, index: i64) {
        // Ordering::Relaxed is sufficient here because:
        // 1. We only perform monotonic updates (fetch_max) - the value only increases.
        // 2. No other atomic operation depends on the ordering of this store.
        // 3. The GlobalLoadContext tracks coarse statistical bounds for metrics,
        //    not for synchronizing concurrent resource access.
        self.last_global_load.fetch_max(index, Ordering::Relaxed);
    }

    pub fn get_first_load(&self) -> i64 {
        // Ordering::Relaxed is safe because:
        // - The load is monotonic (starts at i64::MAX, only decreases via fetch_min).
        // - A slightly stale read (missing a concurrent update) will only produce
        //   a conservative bound for the metrics window, not an incorrect scheduling decision.
        self.first_global_load.load(Ordering::Relaxed)
    }

    pub fn get_last_load(&self) -> i64 {
        // Ordering::Relaxed is safe because:
        // - The load is monotonic (starts at -1, only increases via fetch_max).
        // - A slightly stale read will only produce a conservative bound for the metrics window.
        self.last_global_load.load(Ordering::Relaxed)
    }
}

/// Data Transfer Object (DTO) containing the calculated utilization metrics.
///
/// This struct represents the final "cut" and processed view of the resource usage,
/// excluding warm-up and cool-down periods.
#[derive(Debug, Clone)]
pub struct LoadMetric {
    /// The timestamp representing the start of the valid data interval.
    pub start_time: i64,

    /// The timestamp representing the end of the valid data interval.
    pub end_time: i64,

    /// The average capacity reserved per slot during the valid interval.
    pub avg_reserved_capacity: f64,

    /// The maximum possible capacity of the SlottedSchedule.
    pub possible_capacity: f64,

    /// The ratio of average reserved capacity to possible capacity (0.0 to 1.0).
    pub utilization: f64,
}

impl LoadMetric {
    pub fn new(start_time: i64, end_time: i64, avg_reserved_capacity: f64, possible_capacity: f64, utilization: f64) -> Self {
        LoadMetric { start_time, end_time, avg_reserved_capacity, possible_capacity, utilization }
    }
}

/// A circular buffer implementation used to track resource capacity usage.
///
/// The `LoadBuffer` records load events over time and calculates utilization metrics.
/// It interacts with a [`GlobalLoadContext`] to synchronize the valid time window across
/// multiple resources, ensuring that metrics are calculated over a consistent global timeframe.
#[derive(Debug, Clone)]
pub struct LoadBuffer {
    pub context: Arc<GlobalLoadContext>,

    /// The slot index of the last load added to this specific buffer.
    slot_index_last_local_load: i64,

    /// Counter for empty slots since the last non-empty load.
    slots_since_last_load: i64,

    /// Total reserved capacity (sum of all accepted loads).
    sum_reserved_capacity: i64,

    /// Buffer storing the most recent loads, used to "drop" the tail.
    tail_buffer: VecDeque<i64>,
}

impl LoadBuffer {
    pub fn new(context: Arc<GlobalLoadContext>) -> Self {
        LoadBuffer {
            context,
            slot_index_last_local_load: 0,
            slots_since_last_load: 0,
            sum_reserved_capacity: 0,
            tail_buffer: VecDeque::with_capacity(SLOTS_TO_DROP_ON_END as usize),
        }
    }

    fn add_intern(&mut self, load: i64) {
        self.sum_reserved_capacity += load;

        // Maintain the tail buffer size
        if self.tail_buffer.len() >= SLOTS_TO_DROP_ON_END as usize {
            self.tail_buffer.pop_front();
        }
        self.tail_buffer.push_back(load);
    }

    fn add_zeros_intern(&mut self, count: i64) {
        if count <= 0 {
            return;
        }

        let limit = SLOTS_TO_DROP_ON_END as usize;

        if count as usize >= limit {
            self.tail_buffer.clear();
            for _ in 0..limit {
                self.tail_buffer.push_back(0);
            }
        } else {
            for _ in 0..count {
                if self.tail_buffer.len() >= limit {
                    self.tail_buffer.pop_front();
                }
                self.tail_buffer.push_back(0);
            }
        }
    }

    pub fn add(&mut self, next_slot_load: i64, slot_index: i64) {
        if next_slot_load <= 0 {
            self.slots_since_last_load += 1;
            return;
        }

        // Load occurred > 0
        let effective_index = if slot_index < 0 { 0 } else { slot_index };

        self.context.update_first_load(effective_index);
        self.context.update_last_load(effective_index);
        self.slot_index_last_local_load = effective_index;

        let first_global = self.context.get_first_load();

        if first_global == i64::MAX {
            log::error!("Should not happen if update_first_load worked.");
            return;
        }

        let slots_since_first_load = effective_index - first_global;

        // Do only if warm-up phase is in past
        if slots_since_first_load > SLOTS_TO_DROP_ON_START {
            self.add_zeros_intern(self.slots_since_last_load);
            self.add_intern(next_slot_load);
        }

        self.slots_since_last_load = 0;
    }

    pub fn get_effective_overall_load(&mut self, capacity: f64, start_time_of_first_slot: i64, start_time_of_last_slot: i64) -> LoadMetric {
        let last_global = self.context.get_last_load();
        let empty_slots_to_add = last_global - self.slot_index_last_local_load;

        if empty_slots_to_add > 0 {
            self.add_zeros_intern(empty_slots_to_add);
            self.slot_index_last_local_load = last_global;
        }

        let buffer_reserved_capacity: i64 = self.tail_buffer.iter().sum();
        let reserved_capacity = self.sum_reserved_capacity - buffer_reserved_capacity;
        let first_global = self.context.get_first_load();

        // Handle case where no loads logged
        if first_global == i64::MAX {
            return LoadMetric {
                start_time: start_time_of_first_slot,
                end_time: start_time_of_last_slot,
                avg_reserved_capacity: 0.0,
                possible_capacity: capacity,
                utilization: 0.0,
            };
        }

        let index_of_first_slot = first_global + SLOTS_TO_DROP_ON_START;
        let index_of_last_slot = last_global - SLOTS_TO_DROP_ON_END;
        let number_of_logged_slots = index_of_last_slot - index_of_first_slot;

        let avg_reserved_capacity = if number_of_logged_slots <= 0 { 0.0 } else { (reserved_capacity as f64) / (number_of_logged_slots as f64) };

        let utilization = if capacity > 0.0 { avg_reserved_capacity / capacity } else { 0.0 };

        LoadMetric {
            start_time: start_time_of_first_slot,
            end_time: start_time_of_last_slot,
            avg_reserved_capacity,
            possible_capacity: capacity,
            utilization,
        }
    }
}
