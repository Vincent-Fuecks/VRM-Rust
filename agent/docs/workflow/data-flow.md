# Workflow Data Flow

## Workflow Submission Flow

1. Client submits workflow JSON → `VrmManager`
2. `VrmManager` parses JSON into DTOs → builds `Workflow` in `ReservationStore`
3. `VrmManager` sends workflow to Master ADC
4. Master ADC delegates to `WorkflowScheduler`

## Workflow Scheduling Flow

1. `HEFTSyncWorkflowScheduler::reserve()` is called with workflow ID
2. Upward rank calculation prioritizes tasks
3. For each task (in rank order):
   a. Calculate earliest start time based on data dependencies
   b. Schedule co-allocation node reservations
   c. Schedule data dependencies (link reservations)
   d. Schedule sync dependencies
4. If all tasks succeed → workflow state = `ReserveAnswer`
5. If any task fails → `cancel_all_reservations()` + workflow state = `Rejected`

## Cross-RMS Dependency Flow

When a data dependency spans two different RMS components:
1. `schedule_dependency()` detects `source_component_id != target_component_id`
2. `schedule_cross_rms_dependency()` is called
3. Four virtual link reservations are created:
   - Seg 1: Internal on source AcI (gateway endpoints)
   - Seg 2: Virtual source_gateway → ADC-System
   - Seg 3: Virtual ADC-System → target_gateway
   - Seg 4: Internal on target AcI (gateway endpoints)
4. Each segment is submitted via `submit_task_at_first_grid_component()`
5. If any segment fails → `cancel_all_reservations()` rolls back all
6. Virtual reservations tracked in `original_to_virtual` for cleanup
