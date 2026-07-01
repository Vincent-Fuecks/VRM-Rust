# Workflow Modules

## Core Modules

### `workflow.rs`
Defines the `Workflow` struct — the top-level container for a workflow reservation.
- `WorkflowNode`s map
- `DataDependency`s and `SyncDependency`s maps
- `CoAllocation`s and `CoAllocationDependency`s
- Entry/exit node and co-allocation tracking
- `calculate_upward_rank()` for HEFT prioritization
- `update_reservation()` for atomic state updates

### `workflow_node.rs`
Defines `WorkflowNode` — a single task in the workflow graph.
- References to reservation ID
- Incoming/outgoing data and sync dependency lists
- Co-allocation group membership

### `dependency.rs`
Defines `DataDependency` and `SyncDependency` structs.
- Source/target workflow node references
- Reservation IDs for underlying link reservations
- Size (data) or bandwidth (sync) parameters

### `co_allocation.rs`
Defines `CoAllocation` and `CoAllocationDependency`.
- Group membership tracking
- Inter-group dependency edges

## Scheduler Modules

### `heft_sync_workflow_scheduler.rs`
Implements the HEFTSync scheduling algorithm.
- `reserve()` — main scheduling entry point
- `schedule_co_allocation_node_reservations()` — schedules co-allocated compute tasks
- `schedule_data_dependencies()` — schedules data transfer links
- `schedule_sync_dependencies()` — schedules sync communication links
- `schedule_dependency()` — dispatches to dummy, real, or cross-RMS scheduling
- `schedule_real_dependency()` — schedules internal (same-RMS) real link dependencies using gateway router IDs
- `schedule_cross_rms_dependency()` — 4-segment virtual reservation chain for cross-RMS links
- `schedule_dummy_dependency()` — handles zero-capacity or same-component dependencies
- `schedule_node_reservation_eft()` — EFT-based processor selection
- `cancel_all_reservations()` — atomic rollback with virtual reservation cascade-delete

## Key Design Decisions

- **AD-5: Information Hiding** — The ADC does not enumerate internal routers. Only gateway RouterIds are exposed.
- **AD-3: Virtual Reservation Chain** — Cross-RMS links are split into 4 segments with atomic rollback.
- **AD-8: Config Toggle** — `USE_FULL_INTER_GATEWAY_PATH_FINDING` controls inter-gateway routing strategy.
