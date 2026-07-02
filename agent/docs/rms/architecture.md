# RMS Component — Architecture

## 1. High-Level Architectural Pattern

The RMS (Resource Management System) component follows a **trait-based strategy pattern** with a layered abstraction hierarchy. It serves as the bridge between the VRM meta-scheduler and the underlying physical or simulated HPC cluster. The component is organized into three distinct abstraction layers:

1. **Core Trait Layer** (`Rms` → `AdvanceReservationRms`): Defines the contract that all RMS implementations must fulfill.
2. **Generic Implementation Layer** (`rms_node_network_trait.rs`): Provides a blanket implementation of `AdvanceReservationRms` for any type that implements `Rms + Helper + RmsNodeNetwork`, eliminating code duplication between node+network-capable RMS variants.
3. **Concrete RMS Implementations**: Four adapters that realize the trait contract for different deployment scenarios.

The factory pattern (`RmsSystemWrapper::get_instance()`) decouples configuration deserialization from instantiation, allowing runtime selection of the appropriate RMS adapter based on input DTOs.

## 2. Core Components and Responsibilities

### 2.1 `Rms` Trait (base contract)

Defined in `rms.rs`. The fundamental trait that all RMS adapters implement. It provides:

- Access to the `RmsBase` (identity, resource store, reservation store).
- `commit()` — finalizes a reservation (default: marks `Committed` in `ReservationStore`; overridden by `SlurmRms` for actual REST submission).
- `delete_task()` — removes a reservation from the active schedule.
- `get_active_schedule()` — routes to the correct schedule (node or link, master or shadow) based on reservation type.
- `set_reservation_state()` — state transition helper.

Requires `std::fmt::Debug + Any` for downcasting and debugging.

### 2.2 `AdvanceReservationRms` Trait (reservation life cycle)

Defined in `advance_reservation_trait.rs`. Extends `Rms` with the full reservation management life cycle:

| Method | Purpose |
|---|---|
| `create_shadow_schedule()` | Clones the master schedule for isolated simulation |
| `commit_shadow_schedule()` | Promotes a shadow schedule to replace the master |
| `delete_shadow_schedule()` | Cleans up a shadow schedule |
| `probe()` | Queries for feasible reservation candidates (read-only) |
| `reserve()` | Temporarily reserves a slot on a schedule |
| `probe_best()` | Selects the single best candidate via a comparator |
| `get_fragmentation()` / `get_system_fragmentation()` | Schedule fragmentation metrics |
| `get_load_metric*()` | Load/utilization metrics for time ranges |
| `can_handle_adc_request()` / `can_handle_aci_request()` | Capability checks |

Default implementations for `probe()`, `reserve()`, and `probe_best()` delegate to the appropriate schedule via `get_active_schedule()`.

### 2.3 `Helper` Trait (internal)

Defined in `rms_node_network_trait.rs`. Provides internal accessor/mutator methods for node and network schedules plus their shadow schedule maps. This is an implementation detail used by the blanket `AdvanceReservationRms` impl; not exposed to consumers.

### 2.4 `RmsNodeNetwork` Marker Trait

A supertrait alias: `AdvanceReservationRms + Helper + Rms`. Types implementing this marker automatically receive the blanket `AdvanceReservationRms` implementation defined in `rms_node_network_trait.rs`. Currently implemented by `SlurmRms` and `RmsSimulator`.

### 2.5 `RmsBase` Struct

Holds core identity and data:
- `id: RmsId` — unique identifier.
- `resource_store: ResourceStore` — local resource inventory (nodes, links).
- `reservation_store: ReservationStore` — global reservation registry.

### 2.6 `RmsSetupContext`

A builder-style context (in `common.rs`) that encapsulates parameters needed to construct node schedules, network schedules (with topology), and the `RmsBase` instance. Used by `RmsSimulator::new()` and reusable by other constructors.

### 2.7 Concrete Implementations

| Implementation | File | Capabilities |
|---|---|---|
| `SlurmRms` | `slurm_rms/slurm_base.rs` | Full node+network, live Slurm REST API, background sync loop |
| `RmsSimulator` | `rms_simulator/rms_simulator.rs` | Full node+network simulation |
| `RmsNodeSimulator` | `rms_simulator/rms_node_simulator.rs` | Node-only simulation (standalone `AdvanceReservationRms` impl) |
| `RmsNetworkSimulator` | `rms_simulator/rms_network_simulator.rs` | Link-only simulation (standalone `AdvanceReservationRms` impl) |

Note: `RmsNodeSimulator` and `RmsNetworkSimulator` implement `AdvanceReservationRms` directly without using the blanket impl, because they manage only one schedule type each.

### 2.8 Factory: `RmsSystemWrapper`

Defined in `rms_type.rs`. An enum-based DTO wrapper that dispatches to the appropriate constructor:

```
RmsSystemWrapper::Slurm(dto)       → SlurmRms::new()
RmsSystemWrapper::RmsSimulator(dto) → RmsSimulator::new()
RmsSystemWrapper::DummyRms(dto)    → RmsNodeSimulator or RmsNetworkSimulator via TryFrom
```

### 2.9 Slurm REST API Client

Located in `slurm_rms/api_client/`. A `SlurmRestApi` trait (async) with a `reqwest`-based `SlurmRestApiClient` implementation. Handles:

- `GET /nodes` → `SlurmNodesResponse`
- `GET /jobs` → `SlurmTaskResponse`
- `POST /job/submit` → task submission (commit)
- `DELETE /job/{id}` → task cancellation
- `GET /ping` → health check

Payload and response types are deserialized via `serde` with forward-compatibility markers (`#[serde(flatten)]` for unknown fields).

## 3. Interfaces Between Layers

### 3.1 RMS → Schedule

The `Rms` trait communicates with the `Schedule` trait via `Arc<RwLock<Box<dyn Schedule>>>`. Each RMS holds:
- A **node schedule** (manages `NodeReservation`s)
- A **network schedule** (manages `LinkReservation`s, includes `NetworkTopology` for routing)
- Shadow schedule maps (`HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>`) for both node and network.

The `get_active_schedule()` method routes reservations to the correct schedule based on reservation type (link vs. node) and shadow schedule ID.

### 3.2 RMS → ReservationStore / ResourceStore

- `ReservationStore`: Shared across the system; the RMS reads reservation details and updates states (Open → Committed, Deleted, Rejected, etc.).
- `ResourceStore`: RMS-local; holds the inventory of nodes and links belonging to the underlying cluster.

### 3.3 SlurmRms → Physical Cluster

`SlurmRms` communicates with the Slurm REST API via `SlurmRestApiClient`. A background tokio task (`start_sync()`) periodically polls node and job state to:
- Update the local `ResourceStore` with current node availability.
- Adjust the node schedule's capacity when hardware changes.
- Detect externally submitted/deleted jobs and reflect them in the `ReservationStore`.
- Update reservation states based on Slurm job state transitions.

### 3.4 RMS → Consumer (AcI / VrmComponent)

Consumers interact with the RMS exclusively through the `AdvanceReservationRms` trait (returned as `Box<dyn AdvanceReservationRms + Send + Sync>`). They call `probe()`, `reserve()`, `commit()`, `delete_task()`, etc.

## 4. Error Handling Strategy

The component uses a **mixed error handling approach**:

- **Constructor errors**: `Box<dyn std::error::Error>` (in `RmsSimulator::new()`, `RmsSetupContext`), `ConversionError` (in `TryFrom` impls), `anyhow::Result` (in `SlurmRms::new()` and REST API client).
- **Runtime operations**: Return `bool` (shadow schedule ops), `Option<ReservationId>` (reserve), or `ProbeReservations` (probe). Errors are logged.
- **Fatal assertions**: `expect()` calls in shadow schedule access paths and `panic!()` in `get_active_schedule()` and factory initialization. These represent potential crash vectors in production.

## 5. State Management

The RMS does not own reservation state transitions directly. It mediates between:
- The **Schedule** (which owns slot-level booking state).
- The **ReservationStore** (which owns reservation life cycle state).

State updates flow: RMS operation → Schedule mutation → ReservationStore state update. The `set_reservation_state()` helper provides a direct path for state transitions without schedule interaction.

## 6. Concurrency and Threading

- All concrete RMS types are `Send + Sync`.
- Schedules are wrapped in `Arc<RwLock<Box<dyn Schedule>>>` for concurrent access.
- Shadow schedule maps use `HashMap` (not concurrent), but are accessed only through `&mut self` methods, guaranteeing exclusive access.
- `SlurmRms` spawns a background tokio task for the sync loop. The task holds cloned `Arc`s to shared state (schedules, stores, task mapping).
- The `BiMap<ReservationId, u32>` (task mapping) is wrapped in `Arc<RwLock<>>` for concurrent access from both the sync task and commit/delete operations.
- `parking_lot` is used for all `RwLock` and `Mutex` types.

### Deadlock Potential

- **Low risk in normal operation**: Lock acquisition is shallow (single `read()` or `write()` per operation).
- **Fragmentation metric methods** (`get_fragmentation`, `get_system_fragmentation` in the blanket impl) acquire write locks on both node and network schedules sequentially. If another code path acquires these locks in reverse order, deadlock could occur. Currently, the schedules are always accessed node-first-then-network, but this invariant is not formally enforced.
- **Shadow schedule commit**: The `commit_shadow_schedule` method removes entries from both shadow maps under `&mut self`. No lock nesting with external locks.
