# Resource Module — Architecture

## High-Level Architectural Pattern

The Resource module follows a **Layered Domain Model** pattern integrated within the broader VRM-Rust hexagonal architecture. It functions as the **Resource Management kernel** within the domain, encapsulating:

- Resource representation (nodes and links)
- Resource aggregation and querying (`Resources`, `ResourceStore`)
- Feasibility checking (capacity and topology-aware admission control)

The module is not a standalone service but rather a **domain entity cluster** that both the **ADC** (Advanced Desicion Component) and **AcI** (Advanced Controller Interface) components depend on for resource-aware scheduling.

## Core Components and Responsibilities

### 1. `Resource` Trait (`resource_trait.rs`)

The polymorphic root of the resource domain. It defines:

- `get_capacity()` — Returns total capacity of the resource.
- `as_any()` — Downcasting to concrete types (`NodeResource` / `LinkResource`).
- `get_name()` — Returns the resource identifier (`ResourceName`).
- `can_handle_request()` — Feasibility test against a `FeasibilityRequest`.

**Key design choice:** Type erasure via `dyn Resource` and `as_any()` is used instead of a closed `enum` for resource types. This enables extensibility but introduces runtime type-checks and downcasting risks.

### 2. `FeasibilityRequest` Enum

A discriminated union:

```rust
pub enum FeasibilityRequest {
    Node { capacity: i64, is_moldable: bool },
    Link { source: RouterId, target: RouterId, capacity: i64, is_moldable: bool },
}
```

Request types are strictly separated at compile time. A `Link` request cannot be matched against a `NodeResource` and vice versa (returns `false`).

### 3. `BaseResource` (`resources.rs`)

A shared component struct holding `name: ResourceName` and `capacity: i64`. It is embedded (`base` field) in both `NodeResource` and `LinkResource`. Provides a `can_handle()` method that implements moldable capacity logic:

- If **non-moldable** and capacity > 0: requires `reserved_capacity <= self.capacity`
- If **moldable** or capacity == 0: always returns `true`

**Issue:** `can_handle()` takes `res_reserved_capacity: i64` as a parameter name that is confusing — `BaseResource` has no concept of "reserved" capacity. The name implies state it does not hold.

### 4. `NodeResource` (`node_resource.rs`)

A thin wrapper around `BaseResource` implementing the `Resource` trait. The `can_handle_request()` logic delegates to `BaseResource::can_handle()`. The `FeasibilityRequest::Link` variant is explicitly rejected.

### 5. `LinkResource` (`link_resource.rs`)

Extends `BaseResource` with topology metadata (`source: RouterId`, `target: RouterId`) and a **schedule** (`SlottedScheduleContext<NodeStrategy>`). The `can_handle_request()` method performs:

1. **Topology check:** Does the request's `source`/`target` match this link's endpoints?
2. **Capacity check:** Delegates to `BaseResource::can_handle()`.

**Concern:** `LinkResource` holds an owned `SlottedScheduleContext<NodeStrategy>` schedule, creating a **tight coupling** between the resource model and the scheduling mechanism. This means a `LinkResource` cannot exist without its schedule, and cloning a `LinkResource` clones the entire schedule data.

### 6. `Resources` (`resources.rs`)

A flat collection aggregating `Vec<Box<dyn Resource>>` and a router list (`Vec<RouterId>`). Provides:

- Aggregated capacity queries (`get_total_capacity`, `get_total_link_capacity`, `get_total_node_capacity`)
- Type-filtered counting (`get_node_resource_count`, `get_link_resource_count`)
- Router list membership testing

**Note:** This type appears to be a **legacy abstraction** that is now mostly superseded by `ResourceStore`. It lacks concurrency support and is not thread-safe.

### 7. `ResourceStore` (`resource_store.rs`)

A **thread-safe**, slotmap-indexed repository for resources. This is the primary runtime data structure used by the VRM system. Key characteristics:

- Inner state wrapped in `Arc<RwLock<StoreInner>>` (uses `std::sync::RwLock`)
- Separate `SlotMap<NodeResourceId, Arc<RwLock<NodeResource>>>` and `SlotMap<LinkResourceId, Arc<RwLock<LinkResource>>>`
- K-shortest paths cache for network topology
- Router list management
- Feasibility check entry points: `can_handle_adc_request()` and `can_handle_aci_request()` for Admission Control (ADC) and Advance Controller Interface (AcI) request paths.

## Interfaces (Traits / APIs)

| Interface | Provider | Consumer | Description |
|---|---|---|---|
| `Resource` trait | `NodeResource`, `LinkResource` | `Resources`, `ResourceStore` | Polymorphic resource access |
| `ResourceStore` public methods | `ResourceStore` | ADC, AcI, Schedule | Thread-safe resource lifecycle |
| `can_handle_adc_request(Reservation)` | `ResourceStore` | ADC component | Admission check by value |
| `can_handle_aci_request(ReservationStore, ReservationId)` | `ResourceStore` | AcI component | Admission check by reference |
| `with_mut_slotted_schedule_strategy()` | `ResourceStore` | Scheduler | Access link schedules |

## Error Handling and State Management

### Error Handling
- **Panics:** Several `ResourceStore` methods use `panic!()` when a `LinkResourceId` is not found (`get_source`, `get_target`, `get_name`, `get_capacity`, `with_mut_slotted_schedule_strategy`).
- **Logging:** Errors are logged via `log::error!()` but execution continues, potentially leading to inconsistent state.
- **RwLock handling:** `read().unwrap()` / `write().unwrap()` is used ubiquitously. In case of a poisoned lock, the system panics.

### State Management
- Resources themselves are **immutable with respect to capacity** (capacity is set at construction).
- `ResourceStore` is the stateful owner; its inner state is protected by `RwLock`.
- Node synchronization with external RMS is handled by `update_nodes()`.
- Path caching is done once during topology initialization and is read-only afterwards.

## Deadlock Potential and Thread Management

### Deadlock Risks

1. **`std::sync::RwLock` usage in `ResourceStore`:**
   - The `inner: Arc<RwLock<StoreInner>>` uses `std::sync::RwLock`, **not** `parking_lot::RwLock`.
   - Violates the project convention established in `ReservationStore` which uses `parking_lot::RwLock`.
   - `std::sync::RwLock` does not support `try_read_for()` or deadlock detection.

2. **Double-lock pattern in path feasibility:**
   In `can_handle_link_request()`:
   - Outer: `self.inner.read()` (holds `StoreInner` read lock)
   - Inner: `guard.links.get(link_resource_id)` then `link_lock.read()` (accesses individual link's `Arc<RwLock<LinkResource>>`)
   
   While these are different locks (StoreInner vs LinkResource), the pattern creates complex lock ordering concerns.

3. **No deadlock detection:**
   The module does not use `parking_lot`'s deadlock detection features.

### Thread Management
- No explicit threads are spawned by this module.
- Concurrency is handled entirely through shared state (`Arc<RwLock<...>>`).
- No descriptive thread naming is present.

## Key Architectural Observations

1. **Two Coexisting Abstractions:** `Resources` (non-thread-safe, trait-object-based) and `ResourceStore` (thread-safe, slotmap-based) coexist with overlapping responsibilities. This indicates a mid-refactoring state.

2. **Lock Mismatch:** `ResourceStore` uses `std::sync::RwLock` while the sibling `ReservationStore` uses `parking_lot::RwLock` (convention defined in project rules).

3. **Schedule Embedding:** `LinkResource` directly embeds a full `SlottedScheduleContext<NodeStrategy>`, creating a tight coupling between resource and schedule domains.

4. **Asymmetric Access Patterns:** Node resources are accessed via `SlotMap<NodeResourceId>` with an additional `HashMap<ResourceName, NodeResourceId>` index, while link resources have no name-based index.
