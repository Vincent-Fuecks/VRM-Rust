# Decoupling Workflow Scheduling from VRM Hardware Topology

## Motivation

Currently, the `WorkflowScheduler` (specifically `HEFTSyncWorkflowScheduler`) is tightly coupled to the VRM hardware topology through its dependency on `ADC` and `VrmComponentManager`. This coupling creates several issues:

1. **Testing difficulty**: Workflow scheduling logic cannot be unit-tested without a full VRM system setup (ADCs, AcIs, RMS instances, ResourceStores).
2. **Violation of Separation of Concerns**: The `WorkflowScheduler` trait's `reserve()` method takes `&mut ADC` as a parameter, meaning the scheduler must understand the VRM component topology.
3. **Poor extensibility**: Adding new scheduling algorithms (e.g., ExhaustiveEFT, ExhaustiveFrag) requires duplication of topology-dependent code.
4. **Performance bottlenecks**: Every probe/reserve operation during scheduling traverses the VrmComponentManager hierarchy.

## Proposed Architecture

Introduce a **Workflow Scheduling Abstraction Layer** that decouples scheduling algorithms from the VRM hardware topology.

### New Components

#### 1. `WorkflowExecutionContext` (new module)

A context object that encapsulates all hardware-specific interactions needed by the workflow scheduler. It provides a simplified API for:

- Probing available resources
- Reserving nodes/links
- Committing/cancelling reservations
- Querying resource capacities and link speeds

```rust
/// Encapsulates the hardware-specific operations needed by workflow scheduling algorithms.
/// This decouples the scheduling logic from the VRM hardware topology.
pub struct WorkflowExecutionContext {
    // Internal reference to the ADC manager
    manager: Arc<RwLock<VrmComponentManager>>,
    reservation_store: ReservationStore,
    average_link_speed: i64,
    workflow_booking_interval_end: i64,
    grid_component_res_database: HashMap<ReservationId, ComponentId>,
}
```

#### 2. `WorkflowSchedulingResult` (new struct)

A result type that captures the output of a scheduling operation without exposing hardware details.

```rust
/// The result of a workflow scheduling operation.
pub struct WorkflowSchedulingResult {
    /// Whether scheduling was successful
    pub success: bool,
    /// The mapping of reservation IDs to component IDs
    pub component_allocations: HashMap<ReservationId, ComponentId>,
}
```

### Key Changes

#### A. Refactor `WorkflowScheduler` Trait

**Before:**
```rust
pub trait WorkflowScheduler: std::fmt::Debug + Any + Send {
    fn reserve(&mut self, workflow_res_id: ReservationId, adc: &mut ADC) -> bool;
    fn probe(&mut self, workflow_res_id: ReservationId, adc: &mut ADC) -> Reservations;
}
```

**After:**
```rust
pub trait WorkflowScheduler: std::fmt::Debug + Any + Send {
    fn schedule(&mut self, workflow: &mut Workflow, context: &mut WorkflowExecutionContext) -> WorkflowSchedulingResult;
}
```

#### B. Move Scheduling Logic Out of `HEFTSyncWorkflowScheduler`

Move the hardware-interaction code (reserve calls, component queries) into `WorkflowExecutionContext` methods. The `HEFTSyncWorkflowScheduler` only handles:

1. Computing upward/downward ranks
2. Iterating through ranked nodes
3. Computing Earliest Start Times based on data dependencies
4. Calling `WorkflowExecutionContext` to perform actual resource allocation

#### C. Simplify `ADC::reserve()`

The ADC's `reserve()` method for workflows delegates to `WorkflowExecutionContext` instead of the scheduler directly.

```rust
// In ADC::reserve()
if self.reservation_store.is_workflow(reservation_id) {
    let mut context = WorkflowExecutionContext::new(
        &self.manager,
        self.reservation_store.clone(),
        self.simulator.clone(),
    );
    
    if let Some(mut workflow_scheduler) = self.workflow_scheduler.take() {
        // Get the workflow
        if let Some(workflow_handle) = self.reservation_store.get(reservation_id) {
            let mut reservation = workflow_handle.write();
            if let Reservation::Workflow(ref mut workflow) = *reservation {
                let result = workflow_scheduler.schedule(workflow, &mut context);
                // Apply results
                if result.success {
                    context.apply_allocations(reservation_id);
                    workflow.set_state(ReservationState::ReserveAnswer);
                } else {
                    workflow.set_state(ReservationState::Rejected);
                }
            }
        }
        self.workflow_scheduler = Some(workflow_scheduler);
    }
}
```

## Implementation Plan

### Phase 1: Create `WorkflowExecutionContext` Module

1. Create `src/domain/vrm_system_model/workflow/workflow_execution_context.rs`
2. Define `WorkflowExecutionContext` struct
3. Implement methods:
   - `new()` - constructor from manager, reservation store, simulator
   - `get_average_link_speed()` - query from VrmComponentManager
   - `reserve_node()` - reserve a single node reservation at the best VrmComponent
   - `reserve_link()` - reserve a link reservation between two VrmComponents
   - `cancel_all()` - rollback all reservations in this context
   - `apply_allocations()` - register all successful allocations with the manager
   - `get_booking_interval_end()` - get the workflow deadline

### Phase 2: Refactor `WorkflowScheduler` Trait

1. Add `schedule()` method to `WorkflowScheduler` trait
2. Update `HEFTSyncWorkflowScheduler` to implement `schedule()` instead of `reserve()`
3. Keep `reserve()` as deprecated compatibility wrapper

### Phase 3: Update `ADC::reserve()` for Workflows

1. Create `WorkflowExecutionContext` from ADC state
2. Call `workflow_scheduler.schedule(workflow, &mut context)`
3. Apply results using `context.apply_allocations()`

### Phase 4: Move Scheduling Heuristics into Pure Domain Logic

1. Move `calculate_upward_rank()` and `calculate_downward_rank()` out of `Workflow` struct into a standalone module
2. Create a `WorkflowRankCalculator` that only depends on `Workflow` and `ReservationStore`

## File Changes

### New Files

- `src/domain/vrm_system_model/workflow/workflow_execution_context.rs`

### Modified Files

- `src/domain/vrm_system_model/workflow/mod.rs` - Add new module
- `src/domain/vrm_system_model/scheduler/workflow_scheduler.rs` - Update trait
- `src/domain/vrm_system_model/scheduler/heft_sync_workflow_scheduler.rs` - Refactor to use context
- `src/domain/vrm_system_model/grid_resource_management_system/adc/vrm_component.rs` - Update reserve for workflows

## Testing Strategy

1. **Unit Tests for `WorkflowExecutionContext`**: Mock the VrmComponentManager to test reservation logic
2. **Unit Tests for Scheduler Logic**: Test rank calculation and node ordering without hardware dependencies
3. **Integration Tests**: Full VRM system test to ensure backwards compatibility

## Migration Path

1. Implement new `WorkflowExecutionContext` alongside existing code
2. Refactor ADC to use new approach
3. Remove legacy `reserve()` method from `WorkflowScheduler` trait once all usages are updated
