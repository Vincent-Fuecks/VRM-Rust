# VrmComponent — Module Structure

## Module Hierarchy

```
vrm_component/
├── mod.rs                              # Module declarations
├── vrm_component_trait.rs              # VrmComponent trait definition
├── vrm_component_container.rs          # VrmComponentContainer struct
├── vrm_component_order.rs              # VrmComponentOrder enum + comparators
├── order_res_vrm_component_database.rs # OrderResVrmComponentDatabase
├── aci.rs                              # AcI struct + VrmComponent impl
├── adc/
│   ├── mod.rs                          # ADC struct definition
│   ├── vrm_component.rs                # VrmComponent impl for ADC
│   └── helpers.rs                      # ADC helper methods (optimize, logging)
├── vrm_component_manager/
│   ├── mod.rs                          # VrmComponentManager struct + constructor
│   ├── core.rs                         # Registration, lookup, capacity aggregation
│   ├── scheduling.rs                   # Reserve, probe, delete, commit operations
│   ├── metrics.rs                      # Satisfaction, load metric aggregation
│   ├── shadow.rs                       # Shadow schedule lifecycle (create/delete/commit)
│   └── tracking.rs                     # Reservation-to-component mapping updates
├── vrm_component_registry/
│   ├── mod.rs                          # Module declarations
│   ├── registry_client.rs              # Actor spawning + directory
│   ├── vrm_component_proxy.rs          # Actor proxy (mpsc-based)
│   └── vrm_message.rs                  # Message enum for actor communication
├── scheduler/
│   ├── mod.rs                          # Module declarations
│   ├── workflow_scheduler.rs           # WorkflowScheduler trait + WorkflowSchedulerBase
│   ├── workflow_scheduler_type.rs      # WorkflowSchedulerType enum + factory
│   └── heft_sync_workflow_scheduler.rs # HEFTSync implementation
├── comparator/
│   ├── mod.rs                          # Module declarations
│   ├── load_compare.rs                 # Load-based comparator
│   ├── position_compare.rs             # Registration-position comparator
│   └── size_compare.rs                 # Resource-size comparator
└── scheduler_comparator/
    ├── mod.rs                          # Module declarations
    └── eft_reservation_compare.rs      # EFT-based reservation comparator
```

## Module Descriptions

### `vrm_component_trait`
- **Purpose**: Defines the `VrmComponent` trait — the core interface for all distributed resource management operations.
- **Domain**: Three-Level Commitment (Probe → Reserve → Commit), Shadow Scheduling, metrics.
- **Exports**: `VrmComponent` trait.
- **Used by**: `aci`, `adc`, `vrm_component_proxy`, `vrm_component_manager`.

### `vrm_component_container`
- **Purpose**: Wraps a `VrmComponent` with its local `Schedule`, registration index, link capacity, and failure counter.
- **Domain**: Component lifecycle and sorting metadata.
- **Exports**: `VrmComponentContainer` struct.
- **Used by**: `vrm_component_manager`, `vrm_component_order`, all comparators.

### `vrm_component_order`
- **Purpose**: Enumeration of component ordering strategies and a factory to generate comparator closures.
- **Domain**: Component selection priority.
- **Exports**: `VrmComponentOrder` enum.
- **Used by**: `adc`, `vrm_component_manager`.

### `order_res_vrm_component_database`
- **Purpose**: Maps `ReservationId → ComponentId` with custom sort orders for both keys (reservations) and values (components).
- **Domain**: Reservation-to-component tracking with ordered iteration.
- **Exports**: `OrderResVrmComponentDatabase` struct.
- **Used by**: Currently not referenced in the active codebase (legacy utility).

### `aci`
- **Purpose**: Implements the leaf `VrmComponent` that connects to a physical RMS via `AdvanceReservationRms`.
- **Domain**: AcI lifecycle, shadow schedule reservation tracking, logging.
- **Exports**: `AcI` struct, `ShadowScheduleReservations`, `ReservationContainer`.
- **Used by**: `vrm_manager`, `loader` (via DTO), `vrm_component_registry`.
- **Depends on**: `vrm_component_trait`, `reservation`, `rms`, `common`.

### `adc`
- **Purpose**: Implements the composite `VrmComponent` managing child components and workflow scheduling.
- **Domain**: ADC lifecycle, workflow orchestration, shadow optimization.
- **Exports**: `ADC` struct.
- **Used by**: `vrm_manager`, `vrm_component_registry`.
- **Depends on**: `vrm_component_trait`, `vrm_component_manager`, `vrm_component_registry`, `scheduler`, `reservation`.

### `vrm_component_manager`
- **Purpose**: Central registry for an ADC's child components. Manages allocation tracking, shadow schedules, and metric aggregation.
- **Domain**: Component CRUD, reservation lifecycle tracking, metrics, shadow schedule management.
- **Sub-modules**:
  - `core`: Component registration, deletion, lookup, capacity aggregation, random/ordered iteration.
  - `scheduling`: `reserve`, `probe`, `probe_all_components`, `delete_task_at_component`, `commit_at_component`, `reserve_task_at_first_grid_component`, `reserve_reservation_at_best_vrm_component`.
  - `metrics`: `get_satisfaction`, `get_system_satisfaction`, `get_load_metric`, `get_simulation_load_metric`.
  - `shadow`: `create_shadow_schedule`, `delete_shadow_schedule`, `commit_shadow_schedule`.
  - `tracking`: `register_allocation`, `register_workflow_subtasks`, `update_commit_tracking`, `update_reserve_tracking`, `handle_commit_failure`.
- **Used by**: `adc`.
- **Depends on**: `vrm_component_container`, `vrm_component_trait`, `vrm_component_order`, `reservation`, `schedule`, `common`.

### `vrm_component_registry`
- **Purpose**: Actor-system infrastructure for spawning `VrmComponent` instances in dedicated threads and communicating via message passing.
- **Sub-modules**:
  - `registry_client`: Directory (`ComponentId → Sender`) and `spawn_component()` / `run_actor_loop()`.
  - `vrm_component_proxy`: `VrmComponent` trait implementation that serializes calls into `VrmMessage` and sends them over `mpsc`.
  - `vrm_message`: Enum with one variant per `VrmComponent` method plus `Shutdown`.
- **Used by**: `vrm_manager`, `adc`.

### `scheduler`
- **Purpose**: Workflow decomposition and scheduling algorithms.
- **Sub-modules**:
  - `workflow_scheduler`: `WorkflowScheduler` trait defining `reserve`, `probe`, `finalize_commit`, `delete`.
  - `workflow_scheduler_type`: Enum of available algorithms (HEFTSync, ExhaustiveEFT, etc.) with a factory method.
  - `heft_sync_workflow_scheduler`: Full HEFT implementation with co-allocation and data dependency handling.
- **Used by**: `adc`, `vrm_manager`.
- **Depends on**: `workflow`, `reservation`, `vrm_component` (ADC).

### `comparator`
- **Purpose**: Component-level comparison strategies for ordering child `VrmComponent`s.
- **Sub-modules**:
  - `load_compare`: Orders by aggregated utilization (node + link load).
  - `position_compare`: Orders by registration index with start offset.
  - `size_compare`: Orders by total resource capacity.
- **Used by**: `vrm_component_order`, `order_res_vrm_component_database`.

### `scheduler_comparator`
- **Purpose**: Reservation-level comparison strategies for selecting the best probe result.
- **Sub-modules**:
  - `eft_reservation_compare`: Orders by earliest finish time (EFT).
- **Used by**: `heft_sync_workflow_scheduler`, `adc`, `vrm_manager`.

## Cross-Module Dependency Graph

```
vrm_manager ─────────────────────────────────────────────┐
   │                                                     │
   ├── vrm_component_registry (spawn actors)              │
   │     ├── vrm_component_proxy (VrmComponent impl)      │
   │     └── registry_client (actor loop)                 │
   │                                                     │
   └── adc ──────────────────────────────────────────────┤
         ├── vrm_component_manager                        │
         │     ├── vrm_component_container                │
         │     │     ├── vrm_component_trait              │
         │     │     └── schedule                         │
         │     ├── vrm_component_order                    │
         │     │     └── comparator (load, position, size)│
         │     └── vrm_component_registry (proxies)       │
         ├── scheduler                                    │
         │     └── workflow / reservation                 │
         └── aci ─────────────────────────────────────────┘
               └── rms (AdvanceReservationRms)
```

## Key Observations

1. **`OrderResVrmComponentDatabase`** appears to be a legacy utility — it is defined but not referenced by any active code in the current codebase.
2. **`VrmComponentManager`** is the largest and most complex module, spanning six files and handling component CRUD, scheduling, metrics, shadow management, and tracking.
3. **Actor infrastructure** (`vrm_component_registry`) is a self-contained layer that could theoretically be extracted into a separate crate.
4. **Scheduler algorithms** are pluggable via the `WorkflowScheduler` trait, but only `HEFTSync` is currently implemented; four other variants are stubs (`todo!()`).
5. The `adc` module splits struct definition (`mod.rs`) from trait implementation (`vrm_component.rs`) and helper methods (`helpers.rs`) — a clean separation of concerns.
