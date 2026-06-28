# RMS Component — Modules

## 1. Module Hierarchy

```
src/vrm/rms/
├── mod.rs                              # Public module declarations
├── rms.rs                              # Core Rms trait + RmsBase struct
├── rms_type.rs                         # Factory (RmsSystemWrapper) + RmsSimulatorType
├── common.rs                           # Shared utilities (RmsSetupContext, topology builders)
├── advance_reservation_trait.rs        # AdvanceReservationRms trait (reservation life cycle)
├── rms_node_network_trait.rs           # Helper trait + RmsNodeNetwork marker + blanket impl
├── rms_simulator/
│   ├── mod.rs
│   ├── rms_simulator.rs                # RmsSimulator (full node+network simulation)
│   ├── rms_node_simulator.rs           # RmsNodeSimulator (node-only simulation)
│   └── rms_network_simulator.rs        # RmsNetworkSimulator (link-only simulation)
└── slurm_rms/
    ├── mod.rs
    ├── slurm_base.rs                   # SlurmRms struct + construction + sync loop
    ├── base_rms.rs                     # Rms + Helper trait impls for SlurmRms
    ├── helper.rs                       # Topology construction utility for SlurmRms
    └── api_client/
        ├── mod.rs
        ├── slurm_rest_api_trait.rs     # SlurmRestApi async trait
        ├── slurm_rest_api_client.rs    # reqwest-based HTTP client
        ├── slurm_endpoint.rs           # REST endpoint path enum
        ├── payload/
        │   ├── mod.rs
        │   └── task_properties.rs      # TaskSubmission / JobProperties DTOs
        └── response/
            ├── mod.rs
            ├── nodes.rs                # SlurmNodesResponse + SlurmNode
            ├── tasks.rs                # SlurmTaskResponse + SlurmTask + SlurmWrapped
            ├── task_submit.rs          # TaskSubmitResponse
            └── delete.rs               # SlurmDeleteResponse
```

## 2. Module Descriptions

### 2.1 `mod.rs`
**Purpose**: Public API surface of the `rms` crate module.  
**Exposes**: All submodules (`rms`, `rms_type`, `common`, `advance_reservation_trait`, `rms_node_network_trait`, `rms_simulator`, `slurm_rms`).

### 2.2 `rms.rs`
**Purpose**: Defines the foundational `Rms` trait and the `RmsBase` struct.  
**Key items**:
- `trait Rms` — core contract with `get_base()`, `commit()`, `delete_task()`, `get_active_schedule()`.
- `struct RmsBase` — holds `RmsId`, `ResourceStore`, `ReservationStore`.
- `struct RmsLoadMetric` — optional node and link load metric wrappers.
- `RmsBase::get_nodes_and_links()` — converts `DummyRmsDto` into `(Vec<Node>, Vec<Link>)`.
- `RmsBase::new()` — constructor with empty-resource logging.

### 2.3 `rms_type.rs`
**Purpose**: Factory for instantiating RMS adapters from configuration DTOs.  
**Key items**:
- `enum RmsSimulatorType` — `RmsNodeSimulator` | `RmsNetworkSimulator`.
- `impl RmsSystemWrapper::get_instance()` — async factory dispatching to `SlurmRms`, `RmsSimulator`, `RmsNodeSimulator`, or `RmsNetworkSimulator`.
- `impl FromStr for RmsSimulatorType` — parses `"RmsNodeSimulator"` / `"RmsNetworkSimulator"`.

### 2.4 `common.rs`
**Purpose**: Shared construction helpers and topology building utilities.  
**Key items**:
- `struct RmsSetupContext` — builder for node schedule, network schedule, and `RmsBase`.
- `fn get_nodes_and_links()` — builds `(Vec<Node>, Vec<Link>, HashMap<ResourceName, Vec<RouterId>>)` from `TopologyDto` and optional `ComputeNodeDto`s (used by `RmsSimulator` and `SlurmRms`).
- `fn add_node_information()` — enriches topology nodes with CPU counts from compute node DTOs.
- `struct ComputeNodeResources` — internal helper with `cpus: i64`.

### 2.5 `advance_reservation_trait.rs`
**Purpose**: The `AdvanceReservationRms` trait — the primary consumer-facing interface.  
**Key items**:
- `trait AdvanceReservationRms: Rms + Send + Sync` — extends `Rms` with shadow schedules, probe/reserve/commit life cycle, fragmentation, and load metrics.
- Default implementations for `probe()`, `reserve()`, `probe_best()` delegate to schedules.

### 2.6 `rms_node_network_trait.rs`
**Purpose**: Internal helper trait and blanket implementation eliminating code duplication.  
**Key items**:
- `trait Helper` — accessors/mutators for node/network schedules and shadow maps.
- `trait RmsNodeNetwork: AdvanceReservationRms + Helper + Rms` — marker supertrait.
- `impl<T: RmsNodeNetwork> AdvanceReservationRms for T` — blanket impl providing shadow schedule management, fragmentation, load metrics, and capability checks for all node+network RMS types.
- `impl RmsNodeNetwork for SlurmRms` / `impl RmsNodeNetwork for RmsSimulator` — marker implementations.

### 2.7 `rms_simulator/` (Simulation RMS Adapters)

| Module | Struct | Purpose |
|---|---|---|
| `rms_simulator.rs` | `RmsSimulator` | Full simulation with both node and network schedules. Uses `RmsSetupContext`. Implements `Rms` + `Helper`. Receives `AdvanceReservationRms` via blanket impl. |
| `rms_node_simulator.rs` | `RmsNodeSimulator` | Node-only simulation. Standalone `AdvanceReservationRms` impl with single node schedule and shadow map. Constructed via `TryFrom<(DummyRmsDto, ...)>`. |
| `rms_network_simulator.rs` | `RmsNetworkSimulator` | Link-only simulation. Standalone `AdvanceReservationRms` impl with single network schedule (includes `NetworkTopology`) and shadow map. Constructed via `TryFrom<(DummyRmsDto, ...)>`. |

### 2.8 `slurm_rms/` (Slurm Integration)

| Module | Key Contents | Purpose |
|---|---|---|
| `slurm_base.rs` | `SlurmRms` struct | Main Slurm adapter with 7 fields: `base`, `component_id`, `simulator`, REST client, node/network schedules, shadow maps, task mapping (`BiMap`), and tokio runtime handle. Constructor queries `/nodes` to initialize resources and schedules. `start_sync()` spawns a background tokio task. `perform_sync()` and `update_reservations()` handle periodic state reconciliation. |
| `base_rms.rs` | `impl Rms for SlurmRms`, `impl Helper for SlurmRms` | Implements `Rms` trait: `commit()` spawns async REST submission with timeout; `delete_task()` handles both schedule-only and RMS-level deletion with timeout; `get_active_schedule()` routes to correct schedule. |
| `helper.rs` | `SlurmRms::get_nodes_and_links()` | Constructs topology `(Vec<Node>, Vec<Link>)` from `SlurmRmsDto` and live `SlurmNodesResponse`, merging physical node data with configured topology. |
| `api_client/` | REST client infrastructure | See subsection below. |

### 2.9 `slurm_rms/api_client/` (REST API Client)

| Module | Key Contents | Purpose |
|---|---|---|
| `slurm_rest_api_trait.rs` | `SlurmRestApi` trait | Async trait with `get_nodes()`, `get_tasks()`, `is_rms_alive()`, `commit()`, `delete()`. |
| `slurm_rest_api_client.rs` | `SlurmRestApiClient` | `reqwest::Client`-based implementation with JWT auth headers. URL builder helper. |
| `slurm_endpoint.rs` | `SlurmEndpoint` enum | Path constants for `/nodes`, `/jobs`, `/job`, `/job/submit`, `/ping`, `/config`. |
| `payload/task_properties.rs` | `TaskSubmission`, `JobProperties` | Serializable DTOs for Slurm job submission. |
| `response/nodes.rs` | `SlurmNodesResponse`, `SlurmNode`, `SlurmMeta`, etc. | Deserializable DTOs for `/nodes` response. Forward-compatible via `#[serde(flatten)]`. |
| `response/tasks.rs` | `SlurmTaskResponse`, `SlurmTask`, `SlurmWrapped`, `SlurmOptionExt` | Deserializable DTOs for `/jobs` response. Custom `SlurmWrapped<T>` handles Slurm's nested value/number JSON pattern. |
| `response/task_submit.rs` | `TaskSubmitResponse` | Response type for job submission (contains `job_id`). |
| `response/delete.rs` | `SlurmDeleteResponse` | Response type for job deletion. |

## 3. Module Interaction Graph

```
                    ┌──────────────────────────┐
                    │      rms_type.rs          │
                    │  (RmsSystemWrapper factory)│
                    └─────┬──────────┬──────────┘
                          │          │
              ┌───────────┘          └───────────────┐
              ▼                                      ▼
   ┌─────────────────────┐              ┌─────────────────────┐
   │  rms_simulator/     │              │    slurm_rms/        │
   │  (simulation adapters)│            │  (Slurm integration)  │
   └──────┬──────┬───────┘              └──────────┬──────────┘
          │      │                                  │
          │      │  (RmsNodeSimulator,              │  (SlurmRms)
          │      │   RmsNetworkSimulator            │
          │      │   have standalone impls)         │
          │      │                                  │
          ▼      ▼                                  ▼
   ┌─────────────────────────────────────────────────────┐
   │          rms_node_network_trait.rs                  │
   │  ┌─────────────┐  ┌──────────────┐                  │
   │  │Helper trait  │  │Blanket impl  │                  │
   │  │(accessors)   │  │AdvanceResRms │                  │
   │  └─────────────┘  │for RmsNodeNet │                  │
   │                    └──────────────┘                  │
   └──────────────────────────┬──────────────────────────┘
                              │
                              ▼
   ┌─────────────────────────────────────────────────────┐
   │         advance_reservation_trait.rs                │
   │    (AdvanceReservationRms trait + default impls)    │
   └──────────────────────────┬──────────────────────────┘
                              │
                              ▼
   ┌─────────────────────────────────────────────────────┐
   │                  rms.rs                             │
   │       (Rms trait, RmsBase, RmsLoadMetric)           │
   └──────────────────────────┬──────────────────────────┘
                              │
                              ▼
   ┌─────────────────────────────────────────────────────┐
   │                  common.rs                          │
   │   (RmsSetupContext, get_nodes_and_links utilities)  │
   └─────────────────────────────────────────────────────┘

External dependencies:
   rms.rs ────────► schedule::schedule_trait::Schedule
   rms.rs ────────► reservation::reservation_store::ReservationStore
   rms.rs ────────► resource::resource_store::ResourceStore
   common.rs ─────► schedule::slotted_schedule::strategy::link::topology
   slurm_base.rs ─► api_client::slurm_rest_api_client
   api_client ────► reqwest (HTTP), serde (JSON)
```

## 4. Key Design Decisions

1. **Blanket implementation**: The `RmsNodeNetwork` + blanket `AdvanceReservationRms` pattern avoids duplicating shadow schedule, fragmentation, and load metric logic across `SlurmRms` and `RmsSimulator`. The single-schedule simulators (`RmsNodeSimulator`, `RmsNetworkSimulator`) have bespoke implementations since the blanket impl assumes both node and network schedules exist.

2. **Trait object return**: The factory returns `Box<dyn AdvanceReservationRms + Send + Sync>`, enabling runtime polymorphism. Consumers (the `AcI`) are completely decoupled from which concrete RMS implementation is active.

3. **Async/Sync split**: `SlurmRms` is the only async-aware implementation. Its `new()` is `async`, and it spawns a background tokio task. The `commit()` and `delete_task()` methods spawn fire-and-forget tokio tasks via `rt_handle.spawn()`, making them effectively async from the caller's perspective while keeping the trait interface synchronous.
