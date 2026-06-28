# Resource Module — Module Structure

## Directory Layout

```
resource/
├── mod.rs                       # Module re-exports
├── resource_trait.rs            # Resource trait & FeasibilityRequest enum
├── resources.rs                 # BaseResource struct & non-thread-safe Resources collection
├── node_resource.rs             # NodeResource struct
├── link_resource.rs             # LinkResource struct
└── resource_store.rs            # Thread-safe ResourceStore (primary runtime store)
```

## Module Descriptions

### `mod.rs`
Publicly re-exports all submodules: `link_resource`, `node_resource`, `resource_store`, `resource_trait`, `resources`.

### `resource_trait.rs`
**Purpose:** Defines the core `Resource` trait and the `FeasibilityRequest` enum.

**Domain:** Polymorphic resource abstraction.

**Contents:**
- `pub trait Resource: std::fmt::Debug + Send` — requires `get_capacity()`, `as_any()`, `get_name()`, `can_handle_request()`.
- `pub enum FeasibilityRequest` — `Node { capacity, is_moldable }` | `Link { source, target, capacity, is_moldable }`.

### `resources.rs`
**Purpose:** Contains the `BaseResource` composition struct and the `Resources` aggregation container.

**Domain:** Simple resource aggregation (legacy/simplified API).

**Contents:**
- `pub struct BaseResource { name: ResourceName, capacity: i64 }` — shared component for node/link resources.
- `pub struct Resources { inner: Vec<Box<dyn Resource>>, router_list: Vec<RouterId> }` — a flat, non-thread-safe list of heterogeneous resources.

**Used by:** Potentially legacy code paths. `ResourceStore` is the modern replacement.

### `node_resource.rs`
**Purpose:** Concrete `NodeResource` implementation.

**Domain:** Computational node resources (e.g., CPU cores).

**Contents:**
- `pub struct NodeResource { pub base: BaseResource }` — wraps `BaseResource`.
- Implements `Resource` trait — delegates capacity and feasibility checks to `BaseResource`.

### `link_resource.rs`
**Purpose:** Concrete `LinkResource` implementation.

**Domain:** Network link resources (e.g., bandwidth).

**Contents:**
- `pub struct LinkResource { pub base: BaseResource, pub source: RouterId, pub target: RouterId, pub schedule: SlottedScheduleContext<NodeStrategy> }`.
- Implements `Resource` trait — checks topology match AND capacity feasibility.

**Coupling Note:** Tightly coupled with `SlottedScheduleContext<NodeStrategy>` from the schedule domain.

### `resource_store.rs`
**Purpose:** The primary runtime repository for all resources in the VRM system.

**Domain:** Thread-safe, indexed resource storage and admission control.

**Contents:**
- `NodeResourceId` / `LinkResourceId` — slotmap keys.
- `ResourceStore { inner: Arc<RwLock<StoreInner>> }` — public handle.
- `StoreInner { nodes, links, k_shortest_paths, node_index, router_list }` — internal state.
- Public API for CRUD on nodes/links, path management, feasibility checks, router management, and diagnostics.

## Module Interaction Hierarchy

```
┌────────────────────────────────────────────────────────────────┐
│                        External Consumers                      │
│          (ADC, AcI, Scheduler, ReservationStore)               │
└────────────────────────┬───────────────────────────────────────┘
                         │
                         ▼
┌────────────────────────────────────────────────────────────────┐
│                      ResourceStore                             │
│  (Thread-safe, slotmap-indexed, path-cached, admission logic)  │
└──────┬──────────────────────────────┬──────────────────────────┘
       │                              │
       ▼                              ▼
┌──────────────┐            ┌──────────────────────┐
│ NodeResource  │            │ LinkResource          │
│              │            │ (with embedded        │
│ (wraps       │            │  SlottedSchedule-     │
│ BaseResource)│            │  Context<NodeStrategy>)│
└──────────────┘            └──────────┬───────────┘
       │                               │
       ▼                               ▼
┌──────────────────────────────────────────────┐
│              BaseResource                     │
│   (name: ResourceName, capacity: i64)         │
└──────────────────────────────────────────────┘
```

### Cross-Module Dependencies

| Resource Module | External Module | Dependency Type |
|---|---|---|
| `resource_trait.rs` | `utils::id` (ResourceName, RouterId) | Type import |
| `link_resource.rs` | `schedule::slotted_schedule::slotted_schedule_context::SlottedScheduleContext` | Structural composition |
| `link_resource.rs` | `schedule::slotted_schedule::strategy::node::node_strategy::NodeStrategy` | Generic parameter |
| `resources.rs` | `utils::id` (ResourceName, RouterId) | Type import |
| `resource_store.rs` | `reservation::reservation::Reservation` | Admission control |
| `resource_store.rs` | `reservation::reservation_store::ReservationStore` | Admission control (ACI path) |
| `resource_store.rs` | `schedule::slotted_schedule::strategy::link::topology::Path` | Path caching |
| `resource_store.rs` | `schedule::slotted_schedule::slotted_schedule_context::SlottedScheduleContext` | Link schedule access |
| `resource_store.rs` | `schedule::slotted_schedule::strategy::node::node_strategy::NodeStrategy` | Strategy parameter |
| `resources.rs` | `resource::link_resource::LinkResource` | Type filtering |
| `resources.rs` | `resource::node_resource::NodeResource` | Type filtering |
| `resources.rs` | `resource::resource_trait::Resource` | Trait implementation |

### Key Data Dependencies

```
ResourceStore
  ├── nodes: SlotMap<NodeResourceId, Arc<RwLock<NodeResource>>>
  │     └── NodeResource.base: BaseResource
  ├── links: SlotMap<LinkResourceId, Arc<RwLock<LinkResource>>>
  │     ├── LinkResource.base: BaseResource
  │     ├── LinkResource.source: RouterId
  │     ├── LinkResource.target: RouterId
  │     └── LinkResource.schedule: SlottedScheduleContext<NodeStrategy>
  ├── k_shortest_paths: Arc<RwLock<HashMap<(RouterId, RouterId), Vec<Path>>>>
  ├── node_index: HashMap<ResourceName, NodeResourceId>
  └── router_list: Arc<RwLock<HashSet<RouterId>>>
```
