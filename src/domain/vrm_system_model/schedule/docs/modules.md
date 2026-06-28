# Schedule Component — Module Reference

**Last Updated:** 2025-01-XX

---

## Module Tree

```
schedule/
├── mod.rs                                    # Public module re-exports
├── schedule_trait.rs                         # Schedule trait definition
└── slotted_schedule/
    ├── mod.rs                                # Type aliases (SlottedNodeSchedule, SlottedLinkSchedule)
    ├── schedule_base.rs                      # Schedule trait impl (dispatch to strategy)
    ├── slot.rs                               # Time-slot capacity unit
    ├── slotted_schedule_context.rs           # Core context & operations
    ├── fragmentation.rs                      # Fragmentation algorithms
    └── strategy/
        ├── mod.rs                            # Submodule re-exports
        ├── strategy_trait.rs                 # SlottedScheduleStrategy trait
        ├── node/
        │   ├── mod.rs
        │   └── node_strategy.rs              # Node (compute) scheduling strategy
        └── link/
            ├── mod.rs
            ├── link_strategy.rs              # Link (network) scheduling strategy
            └── topology.rs                   # Network graph, paths, routers
```

---

## `schedule::mod.rs`

**Public API:**
- `pub mod schedule_trait`
- `pub mod slotted_schedule`

---

## `schedule::schedule_trait`

**Depends on:**
- `crate::vrm::reservation::probe_reservations::{ProbeReservationComparator, ProbeReservations}`
- `crate::vrm::reservation::reservation_store::ReservationId`
- `crate::vrm::commons::load_buffer::LoadMetric`

**Defines:**
- `pub trait Schedule: Debug + Send + Sync` — 13 methods for scheduling operations
- `impl Clone for Box<dyn Schedule>` — enables cloning trait objects

---

## `schedule::slotted_schedule::mod`

**Depends on:**
- `slotted_schedule_context::SlottedScheduleContext`
- `strategy::node::node_strategy::NodeStrategy`
- `strategy::link::link_strategy::LinkStrategy`

**Defines:**
- `pub type SlottedNodeSchedule = SlottedScheduleContext<NodeStrategy>`
- `pub type SlottedLinkSchedule = SlottedScheduleContext<LinkStrategy>`

---

## `schedule::slotted_schedule::schedule_base`

**Depends on:**
- `schedule_trait::Schedule`
- `slotted_schedule_context::SlottedScheduleContext`
- `strategy_trait::SlottedScheduleStrategy`
- `reservation::{ProbeReservations, ReservationState, ReservationId, ProbeReservationComparator}`
- `LoadMetric`

**Implements:** `Schedule for SlottedScheduleContext<S>` (all 13 trait methods)

**Key Logic:**
- `probe()`: Updates window, calculates candidates, computes fragmentation deltas
- `probe_best()`: Probes then selects best candidate by comparator
- `reserve()`: Probes, then commits the best candidate via `only_probe_best()`
- `reserve_without_check()`: Directly inserts into slots without feasibility check
- `update()`: Delegates to `SlottedScheduleContext::update()`

---

## `schedule::slotted_schedule::slot`

**No dependencies on other schedule modules.**

**Defines:**
- `pub struct Slot` — capacity tracking unit
  - `load: i64`
  - `capacity: i64`
  - `reservation_ids: HashSet<ReservationId>`

**Methods:**
- `new(capacity: i64) -> Self`
- `get_adjust_requirement(requirements: i64) -> i64`
- `reset()`
- `insert_reservation(requirement: i64, id: ReservationId) -> bool`
- `delete_reservation(id: ReservationId, capacity: i64) -> bool`

---

## `schedule::slotted_schedule::slotted_schedule_context`

**Depends on:**
- `std::sync::Arc`
- `std::collections::HashSet`
- `crate::domain::simulator::GlobalClock`
- `reservation::{Reservation, ReservationState, ReservationTrait, ReservationId, ReservationStore, ProbeReservations, Reservations}`
- `Slot`
- `SlottedScheduleStrategy`
- `id::SlottedScheduleId`
- `load_buffer::{GlobalLoadContext, LoadBuffer}`

**Defines:**
- `pub struct SlottedScheduleContext<S: SlottedScheduleStrategy>` — 18 fields

**Public Methods:**
| Method | Description |
|--------|-------------|
| `new(...)` | Constructor; creates slots, initializes state |
| `clear()` | Resets all slots and active reservations |
| `get_real_slot_index(index)` | Virtual → real index mapping |
| `get_slot(index)` | Immutable slot accessor |
| `get_mut_slot(index)` | Mutable slot accessor |
| `get_slot_index(time)` | Time → virtual index conversion |
| `get_slot_start_time(index)` | Virtual index → absolute start time |
| `get_slot_end_time(index)` | Virtual index → absolute end time |
| `get_effective_slot_index(index)` | Clamp index to scheduling window |
| `update()` | Advance scheduling window to current time |
| `is_reservation_valid_for_deletion(id)` | Validation before deletion |
| `delete_reservation_in_slot(id, capacity, index)` | Single-slot deletion |
| `delete_reservation(id)` | Full reservation removal |
| `is_time_in_scheduling_window(time)` | Time bounds check |
| `get_slot_load(index)` | Load query with bounds checking |
| `calculate_schedule(id)` | Core probe: iterate feasible start times |
| `try_fit_reservation(id, start_index, end_boundary)` | Single-slot feasibility test |
| `update_capacity(capacity)` | Redistribute capacity across all slots |

---

## `schedule::slotted_schedule::fragmentation`

**Depends on:**
- `reservation::ReservationId`
- `Schedule`
- `SlottedScheduleContext`, `SlottedScheduleStrategy`

**Methods (on `SlottedScheduleContext<S>`) with `S: Clone + 'static`:**

| Method | Description |
|--------|-------------|
| `get_fragmentation_quadratic_mean(start, end)` | Quadratic mean fragmentation |
| `get_fragmentation_resubmit(start, end)` | Simulation-based resubmission fragmentation |

**Private helpers:**
- `add_block_which_end_in_range()`
- `add_block_which_are_cut_by_range_end()`
- `calculate_avg_fragmentation()`

**Constant:** `FRAGMENTATION_POWER: f64 = 2.0`

---

## `schedule::slotted_schedule::strategy::strategy_trait`

**Defines:**
- `pub trait SlottedScheduleStrategy: Send + Sync + Debug + Clone + Sized + 'static`
  - 9 associated methods (all take `&SlottedScheduleContext<Self>` or `&mut SlottedScheduleContext<Self>`)

---

## `schedule::slotted_schedule::strategy::node::node_strategy`

**Depends on:**
- `reservation::ReservationId`
- `SlottedScheduleContext`
- `SlottedScheduleStrategy`
- `load_buffer::{LoadMetric, SLOTS_TO_DROP_ON_START, SLOTS_TO_DROP_ON_END}`

**Defines:**
- `pub struct NodeStrategy` — unit struct, `Debug + Clone + Default`

**Implements:** All 9 `SlottedScheduleStrategy` methods.
- Capacity derived from first slot's capacity.
- Fragmentation: delegates to quadratic mean or resubmit based on `use_quadratic_mean_fragmentation`.
- Load metric: averages slot loads over queried range.

---

## `schedule::slotted_schedule::strategy::link::link_strategy`

**Depends on:**
- `std::collections::HashMap`
- `reservation::{ReservationState, ReservationId}`
- `resource::ResourceStore`
- `SlottedScheduleContext`
- `NetworkTopology`, `Path`
- `NodeStrategy`
- `config::RMS_GATEWAY_NAME`, `id::RouterId`, `load_buffer::LoadMetric`

**Defines:**
- `pub struct LinkStrategy` — 4 fields:
  - `topology: NetworkTopology`
  - `reserved_paths: HashMap<ReservationId, HashMap<i64, Path>>`
  - `resource_store: ResourceStore`
  - `max_bandwidth_all_paths: i64`

**Implements:** All 9 `SlottedScheduleStrategy` methods.
- **Note:** `get_fragmentation`, `get_system_fragmentation`, `get_load_metric`, `get_simulation_load_metric` are **unimplemented stubs** returning sentinel values.

---

## `schedule::slotted_schedule::strategy::link::topology`

**Depends on:**
- `std::collections::{HashMap, HashSet, VecDeque}`
- `std::sync::Arc`
- `GlobalClock`
- `reservation::ReservationStore`
- `resource::{LinkResource, ResourceStore, LinkResourceId}`
- `SlottedNodeSchedule`, `NodeStrategy`
- `id::{ComponentId, ResourceName, RouterId, SlottedScheduleId}`

**Defines:**
| Struct | Description |
|--------|-------------|
| `Link` | Physical link DTO (id, source, target, capacity) |
| `Node` | Grid node DTO (name, cpus, connected routers) |
| `Router` | Network router (id, is_grid_access_point) |
| `Path` | Ordered sequence of LinkResourceIds |
| `VirtualLinkResource` | Aggregated virtual capacity between router pair |
| `NetworkTopology` | Full graph: routers, adjacency, path cache, virtual resources |

**Constant:** `K_NUMBER_OF_PATHS: usize = 10`

**NetworkTopology methods:**
| Method | Description |
|--------|-------------|
| `new(...)` | Full initialization: links, routers, adjacency, path cache |
| `calc_k_shortest_paths(source, target)` | BFS-based K-shortest path search |
| `calc_all_paths()` | Iterate all grid-access-point pairs, cache paths |
| `setup_adjacency_matrix()` | Build adjacency from links and routers |
| `setup_routers()` | Derive routers from nodes and links |
| `setup_network_links()` | Create LinkResource instances and their schedules |
