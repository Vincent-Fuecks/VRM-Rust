# Schedule Component Architecture

**Component:** `src/domain/vrm_system_model/schedule/`  
**Last Updated:** 2025-01-XX

---

## 1. Purpose

The Schedule component provides a **time-slotted resource scheduling** abstraction for distributed Virtual Resource Management (VRM). It tracks physical resource capacity (CPU cores, network bandwidth) over discrete time intervals and enables:

- **Feasibility probing**: Finding viable time slots for reservation requests.
- **Capacity reservation**: Committing capacity for accepted requests.
- **Fragmentation analysis**: Measuring how fragmented free capacity is across the timeline.
- **Load metrics**: Calculating utilization and average reserved capacity.
- **Sliding window management**: Advancing the scheduling window over time.

---

## 2. Architectural Principles

### 2.1 Strategy Pattern (Compile-time)

The component uses a **generic strategy pattern** via `SlottedScheduleContext<S: SlottedScheduleStrategy>`. The strategy type parameter `S` is resolved at compile time, avoiding dynamic dispatch overhead while allowing different resource types to share the core slot management infrastructure.

### 2.2 Separation of Concerns

```
┌─────────────────────────────────────┐
│           Schedule Trait            │  ← Public API contract
├─────────────────────────────────────┤
│       SlottedScheduleContext<S>     │  ← Core data structure & operations
├─────────────────────────────────────┤
│    SlottedScheduleStrategy trait     │  ← Extension point for resource types
├──────────────────┬──────────────────┤
│  NodeStrategy    │  LinkStrategy    │  ← Concrete implementations
└──────────────────┴──────────────────┘
```

### 2.3 Sliding Window Design

Time is divided into fixed-width slots (e.g., 1 hour). The schedule maintains a **sliding window** of active slots. As simulation time advances:

1. Expired slots are cleaned (reservations ending before new start time are removed).
2. Historical load data is moved to the `LoadBuffer` for long-term metrics.
3. New slot positions become available at the window's end.

### 2.4 Fragmentation Caching

Fragmentation (a relatively expensive O(n) calculation) is cached with an `is_frag_cache_up_to_date` flag. The cache is invalidated whenever:
- The schedule window advances (`update()`)
- A reservation is added or removed
- Capacity is updated

---

## 3. Core Abstractions

### 3.1 `Schedule` Trait

The public API for all scheduling operations. Defines:

| Method | Purpose |
|--------|---------|
| `probe()` | Find all feasible time slots for a reservation request |
| `probe_best()` | Find the single best time slot using a comparator |
| `reserve()` | Commit a reservation (feasibility-checked) |
| `reserve_without_check()` | Commit a reservation without feasibility check |
| `delete_reservation()` | Remove a committed reservation |
| `clear()` | Remove all reservations and reset state |
| `update()` | Advance the scheduling window |
| `update_capacity()` | Adjust total resource capacity |
| `get_fragmentation()` | Fragmentation score for a time range |
| `get_system_fragmentation()` | Cached system-wide fragmentation |
| `get_load_metric()` | Load/utilization over a time range |
| `get_load_metric_up_to_date()` | Load metric with window update |
| `get_simulation_load_metric()` | Load metric across entire simulation period |

### 3.2 `SlottedScheduleContext<S>`

The core data structure. Key fields:

- **`slots: Vec<Slot>`**: Circular buffer of time slots.
- **`slot_width: i64`**: Duration of one slot in seconds.
- **`start_slot_index` / `end_slot_index`**: Virtual window bounds.
- **`scheduling_window_start_time` / `scheduling_window_end_time`**: Absolute time bounds.
- **`active_reservations: Reservations`**: Set of currently active reservation IDs.
- **`load_buffer: LoadBuffer`**: Historical load for long-term metrics.
- **`fragmentation_cache` / `is_frag_cache_up_to_date`**: Cached fragmentation state.
- **`reservation_store: ReservationStore`**: Shared repository for reservation data.

### 3.3 `Slot`

A unit of capacity tracking for a single time interval:

- **`capacity: i64`**: Total physical capacity (constant).
- **`load: i64`**: Currently reserved/used capacity.
- **`reservation_ids: HashSet<ReservationId>`**: Tracking IDs for capacity accounting.

### 3.4 `SlottedScheduleStrategy` Trait

Extension point for resource-specific behavior:

| Method | Purpose |
|--------|---------|
| `adjust_requirement_to_slot_capacity()` | Check/limit available capacity in a slot |
| `insert_reservation_into_slot()` | Book capacity in a slot |
| `on_delete_reservation()` | Hook for cleanup on deletion |
| `on_clear()` | Hook for cleanup on clear |
| `get_fragmentation()` | Strategy-specific fragmentation |
| `get_system_fragmentation()` | Strategy-specific system fragmentation |
| `get_load_metric()` | Strategy-specific load metrics |
| `get_simulation_load_metric()` | Strategy-specific simulation metrics |
| `get_capacity()` | Strategy-specific capacity value |
| `get_fragmentation_power()` | Returns exponent for fragmentation calculation (default 2.0) |

---

## 4. Concrete Strategies

### 4.1 `NodeStrategy`

- Manages **single-resource capacity** (e.g., CPU cores on one compute node).
- Fragmentation uses per-slot load vs. capacity.
- Capacity is uniform across all slots (representing identical resource).
- Simple: one slot = one resource dimension.

### 4.2 `LinkStrategy`

- Manages **network bandwidth** across a grid topology.
- Uses **K-shortest paths** between source-target routers.
- Capacity checking involves iterating through all links on each candidate path.
- Fragmentation and load metrics are **not yet implemented** (stubs return 0.0/zeroed LoadMetric with warning log).
- Path cache access now uses proper error handling (match instead of unwrap).
- Removed dead code (commented-out `adjust_start_end` method).

---

## 5. Fragmentation Algorithms

### 5.1 Quadratic Mean Method (`get_fragmentation_quadratic_mean()`)

Tracks free capacity blocks of each size across the timeline. For each capacity level (1..max_capacity), computes:

```
frag_level = 1 - (Σ block_len² / (Σ block_len)²)
```

Averages across all capacity levels. **0.0 = best, 1.0 = worst**.

### 5.2 Resubmission Method (`get_fragmentation_resubmit()`)

Simulates releasing all active reservations that overlap the target range and attempting to re-book them. The ratio of rejected capacity to total free capacity provides the fragmentation score. **Expensive** — clones the schedule and iterates.

---

## 6. State Transitions

```
ReservationId (initial)
    │
    ▼
State: ProbeRequest ────► State: ProbeAnswer
    │                              │
    ▼                              ▼
State: ReserveRequest ────► State: ReserveAnswer
    │                              │
    ▼                              ▼
State: Rejected              State: Committed
                                    │
                                    ▼
                              (Expires / Deleted)
```

---

## 7. Error Handling Policy

- **Recoverable errors**: `Result<T, E>` not used in Schedule directly. Instead, errors are logged and fallback values (0, empty set) are returned.
- **Panics**: Previously `unwrap()` / `expect()` were used in several locations. These have been replaced with proper error handling (match statements with log + return/fallback).
- **Negative capacity**: If `get_reserved_capacity()` returns a negative value, the reservation is rejected immediately with an error log.

---

## 8. Thread Safety

- `Schedule` trait requires `Send + Sync`.
- `SlottedScheduleStrategy` trait requires `Send + Sync`.
- `SlottedScheduleContext` is `Clone` but not `Sync` — each schedule context is owned by a single scheduler thread.
- The `ReservationStore` uses internal `Arc<RwLock<>>` for concurrent read access across components.
- `GlobalClock` is `Arc<GlobalClock>` ensuring time synchronization.
