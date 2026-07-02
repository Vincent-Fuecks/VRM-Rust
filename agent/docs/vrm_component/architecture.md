# VrmComponent — Architecture

## High-Level Pattern

The **VrmComponent** sub-system follows an **Actor Model** combined with a **Hierarchical Composite Pattern**. It forms the central decision-making layer of the VRM-Rust system, responsible for orchestrating resource reservations across a distributed tree of components.

- **Actor Model**: Each `VrmComponent` (AcI or ADC) runs in its own dedicated thread spawned by `RegistryClient`. Communication between components occurs exclusively via `mpsc` channels through the `VrmComponentProxy`, serializing method calls into `VrmMessage` variants.
- **Composite Pattern**: `ADC` acts as a composite/internal node that aggregates and manages child `VrmComponent`s. `AcI` is the leaf node that interfaces directly with a physical RMS. Both implement the same `VrmComponent` trait, enabling arbitrary nesting.
- **Centralized Manager**: Each `ADC` owns a `VrmComponentManager` that tracks reservation-to-component mappings, committed/not-committed states, shadow schedules, and workflow subtask relationships.

## Core Components and Responsibilities

### 1. `VrmComponent` (Trait — `vrm_component_trait.rs`)
- The **unified interface** for all distributed resource management operations.
- Defines the Three-Level Commitment model: **Probe → Reserve → Commit**.
- Supports **Shadow Scheduling**: create, operate on, commit, or delete sandbox schedules.
- Provides load metrics and satisfaction index queries.

### 2. `AcI` (Struct — `aci.rs`)
- **Administrative Client Interface**: Leaf node connecting to a physical RMS via `AdvanceReservationRms`.
- Owns `ShadowScheduleReservations` for tracking reservations in shadow contexts.
- Separates `committed_reservations`, `not_committed_reservations`, and `open_probe_reservations`.
- Includes logging helpers (`log_base_info`, `log_probe_info`, `log_detail_info`).

### 3. `ADC` (Struct — `adc/mod.rs`, impl in `adc/vrm_component.rs`)
- **Administrative Domain Controller**: Internal node that manages a collection of child `VrmComponent`s.
- Uses a `WorkflowScheduler` to decompose complex workflows into atomic sub-reservations.
- Implements `optimize_schedule()` for shadow-schedule-based fragmentation optimization.
- Delegates most `VrmComponent` trait methods to `VrmComponentManager`.

### 4. `VrmComponentManager` (Struct — `vrm_component_manager/mod.rs`)
- Central registry and aggregator for an ADC's child components.
- Tracks reservation-to-component mappings (`res_to_vrm_component`, `committed_reservations`, `not_committed_reservations`).
- Manages shadow schedule reservations with per-shadow `ReservationStore` snapshots.
- Tracks workflow subtask relationships (`workflow_subtasks`, `reverse_workflow_subtasks`).
- Sub-modules: `core` (CRUD), `scheduling` (reserve/probe/delete), `metrics` (satisfaction/load), `shadow` (shadow schedule lifecycle), `tracking` (mapping updates).
- **Information Hiding:** `get_component_router_list()` has been removed. The ADC only knows each component's gateway RouterId via `get_component_gateway_router_id()`. Internal router enumeration is delegated to the AcI.

### 5. `VrmComponentContainer` (Struct — `vrm_component_container.rs`)
- Wraps a `Box<dyn VrmComponent + Send>` with its local `Schedule`, registration metadata, link capacity, and failure counter.
- Created during `VrmComponentManager::new()` for each registered component.

### 6. `VrmComponentProxy` (Struct — `vrm_component_registry/vrm_component_proxy.rs`)
- Actor proxy that serializes `VrmComponent` trait method calls into `VrmMessage` variants and sends them over an `mpsc::Sender`.
- Uses a synchronous `call()` pattern: sends a message containing a oneshot `mpsc::Sender<R>` and blocks on the receiver.

### 7. `RegistryClient` (Struct — `vrm_component_registry/registry_client.rs`)
- Directory mapping `ComponentId → mpsc::Sender<VrmMessage>`.
- `spawn_component()`: registers the sender, spawns a named thread running `run_actor_loop()`, returns a `VrmComponentProxy`.
- `run_actor_loop()`: loops on `rx.recv()`, dispatching each `VrmMessage` to the corresponding trait method.

### 8. `VrmMessage` (Enum — `vrm_component_registry/vrm_message.rs`)
- One variant per `VrmComponent` trait method, each carrying a reply `mpsc::Sender`.
- Includes a `Shutdown` variant for graceful actor termination.

### 9. `WorkflowScheduler` (Trait — `scheduler/workflow_scheduler.rs`)
- Interface for workflow decomposition algorithms.
- Defines `reserve()`, `probe()`, `finalize_commit()`, `delete()`, and utility methods.

### 10. `HEFTSyncWorkflowScheduler` (Struct — `scheduler/heft_sync_workflow_scheduler.rs`)
- Implements the **Heterogeneous Earliest Finish Time with Synchronization** algorithm.
- Two-phase: Upward Rank prioritization, then EFT-based processor selection.
- Handles co-allocation groups (sync dependencies) and data dependencies (file transfers).
- Uses placeholder/dummy dependencies when source and target are on the same component or when capacity is zero.
- **Gateway-based routing:** Same-RMS dependencies use gateway RouterIds as endpoints; the AcI handles internal routing per the information hiding principle (AD-5).
- **Cross-RMS 4-segment chain:** Dependencies spanning different RMS components are split into four virtual link reservations with atomic rollback via `cancel_all_reservations()` (AD-3).
- `schedule_cross_rms_dependency()` creates virtual reservations tracked in `ReservationStore.original_to_virtual` for cascade-delete on parent removal.

### 11. Comparators (`comparator/`)
- `LoadCompare`: Orders components by aggregated utilization (node + link).
- `PositionCompare`: Orders by registration index with a configurable start offset.
- `SizeCompare`: Orders by total resource capacity.
- `EFTReservationCompare` (`scheduler_comparator/`): Orders probe reservations by earliest finish time.

### 12. `VrmComponentOrder` (Enum — `vrm_component_order.rs`)
- Defines six ordering strategies for component selection (e.g., `OrderStartFirst`, `OrderLoad`, `OrderResourceSize`).
- Factory method `get_comparator()` returns a boxed closure.

### 13. `OrderResVrmComponentDatabase` (Struct — `order_res_vrm_component_database.rs`)
- Maps reservations to the `ComponentId` handling them, with customizable sort orders for both reservations and components.

## Interfaces Between Layers

```
┌──────────────────────────────────────────────────────────┐
│                   External Consumers                      │
│         VrmManager, Client, higher-level ADC              │
├──────────────────────────────────────────────────────────┤
│                   VrmComponent Trait                      │
│   probe / probe_best / reserve / commit / delete          │
│   create_shadow_schedule / commit_shadow_schedule         │
│   get_satisfaction / get_load_metric / ...                │
├──────────────────────┬───────────────────────────────────┤
│         ADC          │              AcI                   │
│  ┌────────────────┐  │  ┌─────────────────────────────┐  │
│  │VrmCompManager  │  │  │ AdvanceReservationRms       │  │
│  │ ├─ components  │  │  │ ShadowScheduleReservations  │  │
│  │ ├─ tracking    │  │  │ committed/not_committed      │  │
│  │ ├─ shadow      │  │  └─────────────────────────────┘  │
│  │ └─ metrics     │  │                                   │
│  │ WorkflowSched  │  │                                   │
│  └────────────────┘  │                                   │
├──────────────────────┴───────────────────────────────────┤
│              VrmComponentProxy / RegistryClient           │
│         (Actor message passing via mpsc channels)         │
├──────────────────────────────────────────────────────────┤
│              VrmComponentContainer / Schedule             │
└──────────────────────────────────────────────────────────┘
```

## Error Handling

- The `VrmComponent` trait methods return **concrete types** (`bool`, `ReservationId`, `ProbeReservations`, `f64`) rather than `Result<T, E>`.
- **Panics are used extensively** for invariant violations: missing components in maps, double-reservation detection, shadow schedule desynchronization, etc. These are prefixed with descriptive error names (e.g., `ErrorFailedToGetVrmComponentContainer`, `ErrorVrmManagerDuplicateReserveReservationInNotCommittedReservations`).
- `log::error!()` / `log::debug!()` / `log::info!()` are used for diagnostic logging throughout.
- Failures propagate by setting `ReservationState::Rejected` on affected reservations.
- Several methods use `todo!()` for unimplemented paths (ADC `probe_best`, ADC `delete_shadow_schedule`, several `WorkflowSchedulerType` variants).

## State Management

- **Distributed State**: Each ADC's `VrmComponentManager` holds the ground truth for its domain. AcI holds its own local tracking. The `ReservationStore` is the system-wide source of truth for reservation data.
- **Shadow State Isolation**: Shadow schedules have dedicated `ReservationStore` snapshots and mapping clones stored in `VrmComponentManager.shadow_schedule_reservations`.
- **Mutable State Flow**: State changes propagate top-down (ADC → child components) and results aggregate bottom-up (children → ADC → parent).

## Deadlock Potential and Thread Management

### Actor Model Safety
- Each `VrmComponent` (AcI/ADC) runs in its own thread with a single `mpsc::Receiver`.
- The actor loop processes one message at a time, eliminating data races on component state.
- Threads are named descriptively via `thread::Builder::new().name(format!("Actor-{}", id))`.

### Identified Risks

1. **Synchronous Proxy Calls**: `VrmComponentProxy::call()` blocks the calling thread on `reply_rx.recv()`. If two actors call each other simultaneously, a **deadlock** occurs — each is waiting for the other to reply. This is the classic actor deadlock anti-pattern.

2. **`workflow_scheduler.take()` Dance**: In `ADC::reserve()`, the `Option<Box<dyn WorkflowScheduler>>` is temporarily taken out to obtain a `&mut` reference. If a recursive or re-entrant call to `reserve` occurs while the scheduler is `None`, the method logs an error and rejects. This is a workaround for Rust's borrow-checker, not a proper re-entrancy guard.

3. **No Locks on VrmComponentManager**: `VrmComponentManager` uses plain `HashMap`s without synchronization. It is safe because it is only accessed from a single actor thread (the owning ADC). However, if future changes introduce concurrent access, data races will occur.

4. **Consistent Lock Primitives**: The codebase uses `parking_lot` for synchronization elsewhere, but the `VrmComponent` layer primarily relies on actor-model message passing, avoiding locks entirely for its core state.

### Thread Safety
- `VrmComponent` trait requires `Debug` but does not explicitly require `Send + Sync` at the trait level — instead, the concrete usage sites specify `Box<dyn VrmComponent + Send>`.
- `VrmComponentProxy` is `Clone + Debug`, enabling multiple handles to the same actor.
