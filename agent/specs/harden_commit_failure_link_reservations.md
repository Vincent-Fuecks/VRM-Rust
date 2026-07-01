# Harden Commit Failure Handling for Link Reservations

## Problem Statement

`VrmComponentManager::handle_commit_failure()` (in `src/vrm/vrm_component/vrm_component_manager/scheduling.rs`) **panics** with `"Deletion of Committed task failed."` when a committed task cannot be cleanly deleted from its underlying VRM component during rollback. This panic occurs specifically when workflows containing **link reservations with non-zero data dependencies** are scheduled on an RMS variant that does not support link operations (e.g., `RmsNodeSimulator`, a node-only simulator).

The panic crashes the entire ADC actor thread, causing any integration test exercising multi-task workflows with data or sync dependencies to fail with a `RecvError` on the remote-component channel.

### Root Cause Chain

1. A workflow with `ReservationProceeding::Commit` is submitted to `VrmManager`.
2. `VrmManager::try_to_commit_reservation()` calls `adc_master.reserve()` (for workflows), which invokes `HEFTSyncWorkflowScheduler::reserve()`.
3. The HEFT scheduler calls `schedule_data_dependencies()` / `schedule_sync_dependencies()`, which schedule link reservations on child components.
4. After scheduling, `adc_master.commit()` is called on the ADC.
5. The ADC's `commit()` iterates entry-node sub-tasks and calls `manager.commit_at_component()` on each.
6. For link reservations, `commit_at_component()` **always returns `false`** when the underlying RMS does not support link operations, because:
   - The component's `commit()` (e.g., `RmsNodeSimulator`) updates state to `Committed` unconditionally, but
   - The calling code in `adc/vrm_component.rs` checks `is_reservation_state_at_least(w_entry_job, ReservationState::Committed) || !component_answer` — if the state is NOT `Committed` AND `component_answer` is `false`, it calls `handle_commit_failure()`.
7. `handle_commit_failure()` iterates over the failed reservation IDs, sets them to `Rejected`, and calls `delete_task_at_component()`.
8. `delete_task_at_component()` fails because the reservation was never fully tracked in `res_to_vrm_component` for link reservations on node-only simulators, or because the underlying RMS's `delete()` does not transition the state to `Deleted`.
9. The `panic!("Deletion of Committed task failed.")` is triggered, killing the ADC actor thread.

### Affected Code

| File | Location | Role |
|------|----------|------|
| `src/vrm/vrm_component/vrm_component_manager/scheduling.rs:248` | `handle_commit_failure()` | **The panic site** |
| `src/vrm/vrm_component/adc/vrm_component.rs` | `commit()` | Calls `handle_commit_failure` on sub-task failure |
| `src/vrm/vrm_component/vrm_component_manager/scheduling.rs` | `delete_task_at_component()` | Returns `false` triggering the panic |
| `src/vrm/rms/rms_node_simulator.rs` | `commit()` / `delete()` | Node-only; may not handle link reservation lifecycle correctly |
| `src/vrm/vrm_component/scheduler/heft_sync_workflow_scheduler.rs:85` | `reserve()` | Divide-by-zero when `average_link_speed == 0` (already guarded in current code) |

### Impact

- **Integration tests** for the cross-RMS gateway feature (`agent/specs/decoupling_workflow_from_vrm_hardware_topology.md`) cannot exercise multi-task workflows with data/sync dependencies on the `RmsNodeSimulator` RMS variant.
- Any production workflow with non-trivial link dependencies could crash the ADC if scheduled on a node-only RMS backend.
- The panic-by-design pattern in `handle_commit_failure()` violates Rust's error handling conventions (use `Result` instead of `panic!`).

---

## Proposed Solution

### Core Principle

`handle_commit_failure()` must **never panic**. Instead, it should be refactored to return a `Result` (or at minimum log the error and continue with best-effort cleanup). The caller (`adc/vrm_component.rs`) should handle partial cleanup gracefully rather than propagating a panic through the component channel.

### AD-1: Remove `panic!` from `handle_commit_failure`

**Decision:** Replace `panic!("Deletion of Committed task failed.")` with a `log::error!` and continue iterating over remaining reservations. The function signature changes from `-> ()` to `-> bool` returning `false` if any individual deletion failed, but the loop continues regardless.

**Scope:**
- `src/vrm/vrm_component/vrm_component_manager/scheduling.rs` — `handle_commit_failure()`
- `src/vrm/vrm_component/adc/vrm_component.rs` — both call sites

### AD-2: Ensure `delete_task_at_component` Handles Untracked Reservations

**Decision:** When `res_to_vrm_component` does not contain a mapping for the reservation ID (i.e., the reservation was never fully tracked), `delete_task_at_component` should fall back to updating the reservation state to `Deleted` directly in the store, rather than returning `false`.

**Scope:**
- `src/vrm/vrm_component/vrm_component_manager/scheduling.rs` — `delete_task_at_component()`

### AD-3: Ensure RMS Variants Correctly Handle Link Reservation Lifecycle

**Decision:** Each RMS variant (`RmsNodeSimulator`, `RmsNetworkSimulator`, `RmsSimulator`, `SlurmRms`) must be audited and hardened to ensure that:
- `commit()` correctly transitions link reservations (or rejects them with a clear error log)
- `delete()` correctly transitions any reservation to `Deleted`, even if the reservation was not previously committed

**Scope:**
- `src/vrm/rms/rms_node_simulator.rs`
- `src/vrm/rms/rms_network_simulator.rs`
- `src/vrm/rms/rms_simulator/rms_simulator.rs`
- `src/vrm/rms/slurm_rms/`

---

## Implementation Checklist

### Phase 1: Defuse the Panic
- [ ] Refactor `handle_commit_failure()` to use `log::error!` instead of `panic!`
- [ ] Change return type to `bool` (or keep `()` with continuation on error)
- [ ] Ensure all callers handle the non-panicking behavior

### Phase 2: Harden Deletion Path
- [ ] Update `delete_task_at_component()` to handle missing `res_to_vrm_component` entries gracefully
- [ ] Add fallback: if no component mapping found, set state to `Deleted` directly in the store
- [ ] Add debug logging for each fallback path taken

### Phase 3: Audit RMS Variants
- [ ] Audit `RmsNodeSimulator::commit()` and `::delete()` for link reservation handling
- [ ] Audit `RmsNetworkSimulator::commit()` and `::delete()` for node reservation handling
- [ ] Audit `RmsSimulator::commit()` and `::delete()` for mixed node+link handling
- [ ] Audit `SlurmRms` commit/delete paths for link reservations

### Phase 4: Re-enable Ignored Tests
- [ ] Remove `#[ignore]` from `test_single_rms_workflow_data_dependency`
- [ ] Remove `#[ignore]` from `test_single_rms_co_allocation`
- [ ] Remove `#[ignore]` from `test_full_cross_rms_workflow_10_nodes`
- [ ] Verify all three tests pass consistently

### Phase 5: Add Regression Tests
- [ ] Unit test: `handle_commit_failure` with reservations not in `res_to_vrm_component`
- [ ] Unit test: `delete_task_at_component` with missing component mapping
- [ ] Integration test: Workflow with link dependencies on `RmsNodeSimulator` (commit + rollback)
- [ ] Integration test: Workflow with link dependencies on `RmsSimulator` (full cross-RMS path)

---

## Test Cases

### TC-1: `handle_commit_failure` Does Not Panic on Partial Failure

**Objective:** Verify that `handle_commit_failure` completes cleanup even when some reservations cannot be deleted.

**Given:**
- A `VrmComponentManager` with one registered component
- Two reservation IDs: `res_a` (properly tracked in `res_to_vrm_component`) and `res_b` (NOT in `res_to_vrm_component`)

**When:**
- `handle_commit_failure(vec![res_a, res_b])` is called

**Then:**
- The method does not panic
- `res_a` is deleted from the component and its state is `Rejected`
- `res_b`'s state is `Rejected` (best-effort)
- A warning is logged for `res_b`'s deletion failure
- The method returns `false` (indicating partial failure) but completes iteration

---

### TC-2: `delete_task_at_component` Falls Back for Untracked Reservations

**Objective:** Verify that `delete_task_at_component` handles reservations with no registered component mapping.

**Given:**
- A `VrmComponentManager`
- A reservation ID `res_x` that does NOT exist in `res_to_vrm_component`

**When:**
- `delete_task_at_component(res_x, None)` is called

**Then:**
- The method does not return `false` (i.e., treats the fallback as success)
- The reservation's state in the store is `Deleted`
- A debug log message indicates the fallback path was used

---

### TC-3: Workflow with Data Dependencies Commits Successfully on RmsSimulator

**Objective:** End-to-end verification that a workflow with data dependencies (link reservations) successfully commits on an RMS that supports both node and link operations.

**Given:**
- A VRM system with one `RmsSimulator` AcI (supports node + link)
- A workflow with 2 tasks and 1 data dependency (`A → B`, size = 500)

**When:**
- `VrmManager::run_vrm()` processes the workflow with `ReservationProceeding::Commit`

**Then:**
- Workflow state is `ReservationState::ReserveAnswer`
- All node sub-tasks reach `ReservationState::ReserveAnswer`
- The link reservation is scheduled and committed
- No panic occurs
- No orphaned reservations remain

---

### TC-4: Workflow with Sync Dependencies Commits Successfully

**Objective:** End-to-end verification that co-allocated tasks with sync dependencies successfully commit.

**Given:**
- A VRM system with one `RmsSimulator` AcI
- A workflow with 3 tasks forming `CoAllocation(A, B, C)` via sync dependencies

**When:**
- `VrmManager::run_vrm()` processes the workflow

**Then:**
- All 3 node tasks reach `ReservationState::ReserveAnswer`
- Sync dependency link reservations are scheduled
- Co-allocation members share consistent start times
- No panic occurs

---

### TC-5: Full Cross-RMS Workflow End-to-End (10 Nodes)

**Objective:** The flagship end-to-end test from the gateway feature spec — 10 nodes spanning two RMS systems with data + sync dependencies.

**Given:**
- Two `RmsSimulator` AcIs (full node+link support)
- A workflow with 10 tasks, cross-RMS data dependencies, and cross-RMS sync dependencies

**When:**
- `VrmManager::run_vrm()` processes the workflow

**Then:**
- Workflow reaches `ReservationState::ReserveAnswer`
- Virtual reservation chains are created for cross-RMS dependencies
- All 4 segments per cross-RMS link are tracked
- Cascade-delete of virtual reservations works on rollback
- No panic occurs

---

## Dependencies

- **Blocks:** Completion of `agent/specs/decoupling_workflow_from_vrm_hardware_topology.md` Phase 5 (Tests) — the ignored tests TC-1.1, TC-1.2, and TC-7.1 depend on this fix.
- **Depends on:** Phase 1–4 of `decoupling_workflow_from_vrm_hardware_topology.md` (already completed).

## Effort Estimate

- **Phase 1 (defuse panic):** Small (~2–4 lines changed)
- **Phase 2 (harden deletion):** Small (~10–15 lines changed)
- **Phase 3 (audit RMS variants):** Medium (requires careful review of each RMS variant's lifecycle methods)
- **Phase 4 (re-enable tests):** Small (remove `#[ignore]` annotations)
- **Phase 5 (regression tests):** Medium (4 new test functions)

**Total:** ~1–2 days
