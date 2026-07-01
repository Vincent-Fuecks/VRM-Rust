# Test Specification: Workflow Scheduling & Dependency Resolution
## TC-001: Multiple Data Dependencies Between Co-Allocation Groups Across One and Two HPC Systems
* **Description:** Validates that a 10-node workflow with three co-allocation groups (formed by sync dependencies) and multiple inter-group data dependencies schedules successfully. Data dependencies are split: some connect nodes co-located within the same HPC system (`AcI-0`), while others connect nodes residing on different HPC systems (`AcI-0` → `AcI-1`), triggering the 4-segment virtual reservation chain for cross-RMS links.
* **Preconditions:**
  * Two `RmsSimulator` instances are configured (`AcI-0` on `rms_0`, `AcI-1` on `rms_1`), each with sufficient CPU slots, network bandwidth, and a defined gateway topology (`TopologyDto` with switches, gateway ingress/egress).
  * One `ADC` (`ADC-Master`) acts as parent of both AcIs.
  * A `WorkflowDto` with 10 `TaskDto` entries is constructed:
    - **Co-Allocation Group A**: Nodes `N1`, `N2`, `N3` connected via sync dependencies `N1↔N2`, `N2↔N3` → all three on `AcI-0`.
    - **Co-Allocation Group B**: Nodes `N4`, `N5`, `N6` connected via sync dependencies `N4↔N5`, `N5↔N6` → all three on `AcI-0`.
    - **Co-Allocation Group C**: Nodes `N7`, `N8`, `N9`, `N10` connected via sync dependencies `N7↔N8`, `N8↔N9`, `N9↔N10` → `N7`,`N8` on `AcI-0` and `N9`,`N10` on `AcI-1`.
    - **Inter-group Data Dependencies**:
      - `N3 → N4` (size=100 MB) — same HPC (`AcI-0` internal).
      - `N6 → N7` (size=200 MB) — same HPC (`AcI-0` internal).
      - `N8 → N9` (size=150 MB) — cross-HPC (`AcI-0` → `AcI-1`).
  * All tasks have `duration=10`, `cpus=2`, `is_moldable=false`.
  * Workflow `booking_interval_start=0`, `booking_interval_end=10000`, `request_proceeding=Commit`.
  * `GlobalClock` is set to simulation mode.
* **Test Steps:**
  1. Parse the WorkflowDto into the `ReservationStore` via `Workflow::create_form_dto()`.
  2. Verify that exactly 3 `CoAllocation` groups are created and that group membership matches the sync-dependency topology.
  3. Verify that inter-group `CoAllocationDependency` edges exist for A→B, B→C and that the cross-HPC edge B→C is correctly identified.
  4. Initialize the VRM system with `VrmManager::init_vrm_system()` using two `AcIDto` (each with an `RmsSimulatorDto`) and one `ADCDto`.
  5. Execute `vrm_manager.run_vrm().await`.
  6. Inspect the `ReservationStore` for all sub-reservations of the workflow.
* **Expected Result:**
  * The workflow reservation is in state `ReserveAnswer` (or `Committed` after finalization).
  * All 10 `NodeReservation`s are in state `ReserveAnswer` or `Committed`.
  * All `DataDependency` link reservations are in state `ReserveAnswer` or `Committed`.
  * All `SyncDependency` link reservations are in state `ReserveAnswer` or `Committed`.
  * For each `CoAllocation` group, all member nodes share the same `assigned_start` time.
  * The cross-HPC dependency `N8 → N9` has its 4-segment virtual reservation chain fully allocated and tracked in `original_to_virtual`.
  * No child reservation is in state `Rejected`.

---
## TC-002: Multiple Data Dependencies Within a Single Co-Allocation Group
* **Description:** Verifies that multiple data dependencies between members of the *same* co-allocation group are scheduled correctly. Since co-allocated nodes share identical start times, the scheduler must place data transfers within that window without violating the co-allocation constraint.
* **Preconditions:**
  * A single `RmsSimulator` is configured (one HPC system, `AcI-0`).
  * A `WorkflowDto` with 10 `TaskDto` entries is constructed:
    - **Co-Allocation Group A**: All 10 nodes `N1 … N10` are connected by a chain of sync dependencies: `N1↔N2`, `N2↔N3`, …, `N9↔N10`.
    - **Data Dependencies within the group**: `N1 → N3` (size=50), `N2 → N5` (size=75), `N4 → N7` (size=30), `N6 → N9` (size=60), `N8 → N10` (size=40).
  * All tasks have `duration=20`, `cpus=4`, `is_moldable=false`.
  * Workflow `booking_interval_start=0`, `booking_interval_end=50000`, `request_proceeding=Commit`.
* **Test Steps:**
  1. Parse the WorkflowDto and verify a single `CoAllocation` group containing all 10 nodes is formed.
  2. Verify that the 5 data dependencies are registered as `incoming_data_dependencies` / `outgoing_data_dependencies` on the `CoAllocation`, but that no `CoAllocationDependency` edges are created (since source and target belong to the same group).
  3. Run the VRM system.
  4. Check the states of all reservations.
* **Expected Result:**
  * The single `CoAllocation` contains all 10 nodes as members.
  * Zero `CoAllocationDependency` edges exist (all data deps are intra-group).
  * All 10 `NodeReservation`s share the same `assigned_start` and are in state `ReserveAnswer` or `Committed`.
  * All 5 data dependency link reservations are in state `ReserveAnswer` or `Committed`.
  * No child reservation is `Rejected`.

---
## TC-003: Two Co-Allocation Groups Each With Multiple Sync Dependencies
* **Description:** Validates a workflow containing two distinct co-allocation groups, each with multiple internal sync dependencies. Ensures that sync dependencies are scheduled as link reservations and that the two groups are correctly isolated from each other.
* **Preconditions:**
  * A single `RmsSimulator` (one HPC system, `AcI-0`).
  * A `WorkflowDto` with 10 `TaskDto` entries:
    - **Co-Allocation Group A**: Nodes `N1 … N5` connected via sync: `N1↔N2`, `N2↔N3`, `N3↔N4`, `N4↔N5`, plus an additional cross-link `N1↔N3`.
    - **Co-Allocation Group B**: Nodes `N6 … N10` connected via sync: `N6↔N7`, `N7↔N8`, `N8↔N9`, `N9↔N10`, plus an additional cross-link `N6↔N8`.
    - **One data dependency** connecting the two groups: `N5 → N6` (size=100).
  * All tasks have `duration=15`, `cpus=2`.
  * Workflow `booking_interval_start=0`, `booking_interval_end=100000`, `request_proceeding=Commit`.
* **Test Steps:**
  1. Parse the WorkflowDto and verify exactly 2 `CoAllocation` groups are formed.
  2. Verify Group A has 5 members, Group B has 5 members.
  3. Verify that all 6 sync dependency link reservations (4+2 for A, 4+2 for B) are registered under their respective `CoAllocation.sync_dependencies`.
  4. Verify exactly 1 `CoAllocationDependency` edge exists (Group A → Group B).
  5. Run the VRM system.
  6. Assert all reservations reach `ReserveAnswer` / `Committed`.
* **Expected Result:**
  * Group A members `N1 … N5` all share `assigned_start_A`.
  * Group B members `N6 … N10` all share `assigned_start_B`.
  * `assigned_start_A` ≤ `assigned_start_B` (Group A is upstream via the data dependency).
  * All 6 sync dependency links are in state `ReserveAnswer` or `Committed`.
  * The single data dependency link is in state `ReserveAnswer` or `Committed`.
  * No reservation is `Rejected`.

---
## TC-004: All Dummy Dependencies Transition to Committed State
* **Description:** Verifies that dependencies classified as *dummy* (zero reserved capacity or source- and target-components identical after scheduling) are immediately set to `Committed` state by the `schedule_dummy_dependency()` path, bypassing the normal reserve/commit cycle on an AcI.
* **Preconditions:**
  * A single `RmsSimulator` (`AcI-0`).
  * A `WorkflowDto` with 10 nodes. Dependencies are a mix:
    - 3 data dependencies with `size=0` (implicit "data" dependencies via the `dependencies.data` field).
    - 2 sync dependencies with `bandwidth=0` (implicit "sync" dependencies via the `dependencies.sync` field).
    - 2 explicit data dependencies with `size>0` between nodes that will be scheduled on the same AcI (these become dummy after scheduling if both endpoints land on the same component — but for this test, ensure they are on different components so they are NOT dummy).
  * All tasks have `duration=10`, `cpus=2`.
  * Workflow `booking_interval_end=50000`, `request_proceeding=Commit`.
* **Test Steps:**
  1. Parse the WorkflowDto and note the reservation IDs of all dependencies.
  2. Run the VRM system.
  3. For each dependency reservation, inspect its state in the `ReservationStore`.
* **Expected Result:**
  * All dependencies with `size=0` or `bandwidth=0` are in state `Committed`.
  * Dependencies with `size>0` between different components are in state `ReserveAnswer` (they went through the real scheduling path).
  * The dummy dependencies have `assigned_start == assigned_end` (zero duration) and `reserved_capacity == 0`.
  * The dummy dependency link endpoints are both set to `RouterId("localhost")`.
  * No reservation is `Rejected`.

---
## TC-005: Workflow Where All Data and Sync Dependencies Are Dummy Reservations
* **Description:** End-to-end test of the *all-dummy* scenario: every dependency in the workflow has either zero capacity or connects nodes that land on the same component. All dependencies must be fast-tracked to `Committed` while node reservations follow the normal scheduling path.
* **Preconditions:**
  * A single `RmsSimulator` (`AcI-0`).
  * A `WorkflowDto` with 10 nodes, all dependencies declared via the implicit `dependencies.data` and `dependencies.sync` fields (which produce `size=0` / `bandwidth=0`):
    - Data chain: `N1→N2`, `N2→N3`, …, `N9→N10` (9 data deps of size=0).
    - Sync pairs: `N1↔N3`, `N4↔N6`, `N7↔N9` (3 sync deps of bandwidth=0).
  * All tasks have `duration=10`, `cpus=2`.
  * Workflow `booking_interval_end=50000`, `request_proceeding=Commit`.
* **Test Steps:**
  1. Parse the WorkflowDto; verify 9 data deps and 3 sync deps are created, all with `size=0` / `bandwidth=0`.
  2. Run the VRM system.
  3. Inspect the state of every data and sync dependency.
  4. Inspect the state of every node reservation.
* **Expected Result:**
  * All 9 data dependency link reservations are in state `Committed`.
  * All 3 sync dependency link reservations are in state `Committed`.
  * All 10 node reservations are in state `ReserveAnswer` or `Committed`.
  * The workflow itself is in state `ReserveAnswer` (or `Committed`).
  * No dummy dependency went through the real scheduling path (no cross-RMS virtual chain created).
  * No reservation is `Rejected`.

---
## TC-006: Workflow Deadline Not Reachable — Rejected
* **Description:** A workflow whose total critical-path duration exceeds the `booking_interval_end` cannot be accommodated. The `HEFTSyncWorkflowScheduler` must detect the deadline violation during scheduling and reject the entire workflow, setting all associated reservations to `Rejected`.
* **Preconditions:**
  * A single `RmsSimulator` (`AcI-0`).
  * A `WorkflowDto` with 10 nodes, each having `duration=5000`. Nodes are arranged in a sequential data-dependency chain: `N1→N2→…→N10`, forcing serial execution. Total minimum makespan ≈ 10 × 5000 = 50,000.
  * Workflow `booking_interval_start=0`, `booking_interval_end=10000` (only 10,000 slots available — far too few).
  * `request_proceeding=Commit`.
* **Test Steps:**
  1. Parse the WorkflowDto and confirm the sequential topology.
  2. Run the VRM system.
  3. Check the workflow reservation state.
  4. Check the states of all child reservations (nodes and dependencies).
* **Expected Result:**
  * The workflow reservation is in state `Rejected`.
  * All 10 node reservations are in state `Rejected`.
  * All data dependency link reservations are in state `Rejected`.
  * No child reservation is left in `Open`, `ReserveAnswer`, or `Committed`.
  * A debug log message contains "Deadline exceeded" identifying the offending node.

---
## TC-007: Assigned Start Later Than Assigned End — Rejected
* **Description:** A task with an invalid time window where `assigned_start > assigned_end` (i.e., the computed start time falls after the allowed end) must cause the workflow to be rejected. This condition can arise when a predecessor's `assigned_end` plus the file transfer time pushes the start past `booking_interval_end`.
* **Preconditions:**
  * A single `RmsSimulator` (`AcI-0`).
  * A `WorkflowDto` with 2 nodes: `N1` (duration=100, cpus=2) and `N2` (duration=100, cpus=2). A data dependency `N1 → N2` with `size=1000000` (huge file).
  * Workflow `booking_interval_start=0`, `booking_interval_end=150`. The huge file transfer time makes it impossible to start `N2` before the deadline.
  * Node `N1` has `booking_interval_start=0`, `booking_interval_end=150` → `N1` fits (`start=0`, `end=100`), but `N2` cannot start before `100 + transfer_time`, which exceeds `150`.
* **Test Steps:**
  1. Parse the WorkflowDto.
  2. Run the VRM system.
  3. Assert the workflow is `Rejected`.
  4. Assert `N2` is `Rejected` because its earliest start (`N1.end + transfer_time`) exceeds the booking interval.
* **Expected Result:**
  * The workflow is in state `Rejected`.
  * `N1` may be scheduled (`ReserveAnswer`) but will be rolled back via `cancel_all_reservations()`, resulting in `Rejected`.
  * `N2` is `Rejected`.
  * The data dependency is `Rejected`.
  * All reservations associated with the workflow are in state `Rejected`.

---
## TC-008: Assigned Start Equals Assigned End — Rejected
* **Description:** A task with zero effective duration (`assigned_start == assigned_end`) represents an invalid scheduling outcome and must cause the workflow to be rejected. This occurs when a node's `task_duration` is 0 or when the scheduler compresses the booking window to a single point.
* **Preconditions:**
  * A single `RmsSimulator` (`AcI-0`).
  * A `WorkflowDto` with 10 nodes, where at least one node (`N5`) has `duration=0`.
  * All other nodes have `duration=10`, `cpus=2`.
  * Workflow `booking_interval_end=50000`, `request_proceeding=Commit`.
* **Test Steps:**
  1. Parse the WorkflowDto and confirm `N5` has `task_duration=0` in the `ReservationStore`.
  2. Run the VRM system.
  3. Verify that the zero-duration reservation causes the scheduling to fail.
  4. Assert all workflow-associated reservations are `Rejected`.
* **Expected Result:**
  * The workflow is in state `Rejected`.
  * All 10 node reservations are in state `Rejected`.
  * All dependency reservations are in state `Rejected`.
  * The zero-duration task is identified in the logs.

---
## TC-009: Negative Payload on Data Dependency — Reject Entire Workflow With Error Message
* **Description:** A data dependency with a negative `size` value is semantically invalid. The system must detect this during scheduling, reject the entire workflow, and produce a diagnostic error message identifying both the offending node and the negative payload value.
* **Preconditions:**
  * A single `RmsSimulator` (`AcI-0`).
  * A `WorkflowDto` with 10 nodes. One data dependency (e.g., `N3 → N4`) has `size=-500` in its `DataOutDto`.
  * All other dependencies have valid non-negative sizes.
  * All tasks have `duration=10`, `cpus=2`.
  * Workflow `booking_interval_end=50000`, `request_proceeding=Commit`.
* **Test Steps:**
  1. Parse the WorkflowDto and confirm the negative size is preserved in the `DataDependency`.
  2. Run the VRM system.
  3. Check the workflow state.
  4. Check that an error-level log message contains the node ID (`N3` or `N4`) and the negative payload value (`-500`).
  5. Assert that no child reservation is in a success state.
* **Expected Result:**
  * The workflow is in state `Rejected`.
  * All 10 node reservations are `Rejected`.
  * All dependency link reservations are `Rejected`.
  * An error log entry identifies the specific data dependency, its source/target nodes, and the negative size value.
  * The error is surfaced to the client via the `ReservationState::Rejected` transition.

---
## TC-010: Gateway Node Traffic Capacity Exceeded at a Given Time — Rejected
* **Description:** Tests the schedule-level constraint that the ingress or egress bandwidth of a gateway node must not be exceeded at any point in time. When a link reservation would cause the gateway's scheduled traffic to surpass its configured capacity, the reservation must be rejected, cascading to the entire workflow if no alternative slot exists.
* **Preconditions:**
  * Two `RmsSimulator` instances (`AcI-0`, `AcI-1`), each with a gateway having limited ingress/egress bandwidth (e.g., `ingress_bandwidth_gbps=10`, `egress_bandwidth_gbps=10`).
  * A `WorkflowDto` with 10 nodes, many having cross-HPC data dependencies with large file sizes that collectively saturate the gateway link.
  * All cross-RMS data dependencies are scheduled within overlapping time windows so that the aggregate bandwidth demand exceeds the gateway capacity at some time slot.
  * Workflow `booking_interval_end=50000`, `request_proceeding=Commit`.
* **Test Steps:**
  1. Parse the WorkflowDto.
  2. Run the VRM system.
  3. Verify that at least one cross-RMS link reservation (or one of its 4 virtual segments) fails to reserve due to insufficient gateway bandwidth.
  4. Verify that `cancel_all_reservations()` rolls back all prior allocations.
  5. Assert the workflow is `Rejected`.
* **Expected Result:**
  * The workflow reservation is in state `Rejected`.
  * All child reservations are in state `Rejected`.
  * The `ReservationStore` contains no orphaned virtual reservations (all cascade-deleted).
  * A log entry indicates that a link reservation on the gateway was rejected due to capacity exhaustion.
  * The gateway schedule (`network_schedule` of the relevant `RmsSimulator`) shows the rejected reservation was attempted but could not be placed.

---
## TC-011: Cross-RMS 4-Segment Virtual Chain — Partial Failure Atomic Rollback
* **Description:** Verifies the atomicity guarantee of the cross-RMS virtual reservation chain. If any of the 4 segments (source internal, source-gateway→ADC, ADC→target-gateway, target internal) fails to reserve, all previously reserved segments are rolled back and the workflow is rejected.
* **Preconditions:**
  * Two `RmsSimulator` instances (`AcI-0`, `AcI-1`).
  * A `WorkflowDto` with 2 nodes: `N1` on `AcI-0`, `N2` on `AcI-1`, connected by a cross-RMS data dependency with `size=500`.
  * `AcI-1` is configured with extremely constrained network capacity so that segment 3 or 4 will fail.
  * Workflow `booking_interval_end=50000`, `request_proceeding=Commit`.
* **Test Steps:**
  1. Run the VRM system.
  2. After `run_vrm()` completes, check that no virtual reservations remain in the `ReservationStore` for the failed cross-RMS dependency.
  3. Verify that `original_to_virtual` tracking map has been cleaned up.
  4. Assert both `N1` and `N2` and the data dependency are `Rejected`.
* **Expected Result:**
  * The workflow is `Rejected`.
  * Zero virtual reservations remain in the store (all cascade-deleted during `cancel_all_reservations`).
  * The `original_to_virtual` map contains no entry for the failed dependency.
  * A debug log message identifies which segment of the 4-segment chain failed (e.g., "Cross-RMS Segment 3 failed").

---
## TC-012: Workflow Rejection — All Child Reservations Transition to Rejected
* **Description:** Comprehensive validation that when a workflow is rejected for any reason, *every* associated reservation (nodes, data deps, sync deps, virtual reservations) is consistently set to `Rejected` and no reservation remains in `Open`, `ProbeAnswer`, `ReserveAnswer`, or `Committed`.
* **Preconditions:**
  * Two `RmsSimulator` instances.
  * A `WorkflowDto` with 10 nodes having both intra- and inter-HPC dependencies.
  * A deadline constraint that makes the workflow impossible to schedule (e.g., `booking_interval_end=10` with long task durations).
* **Test Steps:**
  1. Run the VRM system.
  2. Collect all reservation IDs associated with the workflow via `store.get_workflow_res_ids()`.
  3. Assert every collected ID has state `Rejected`.
  4. Assert no child reservation has state `Open`, `ProbeAnswer`, `ReserveAnswer`, `Committed`, or `Finished`.
  5. Assert the workflow itself is `Rejected`.
* **Expected Result:**
  * The set of child reservation IDs is non-empty.
  * For every child ID: `store.get_state(id) == ReservationState::Rejected`.
  * For the workflow ID: `store.get_state(workflow_id) == ReservationState::Rejected`.
  * The `open_reservations` set in `VrmManager` does not contain the workflow or any of its children.
