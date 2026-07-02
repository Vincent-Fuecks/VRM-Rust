# US_data_letterbox_and_deps — Letterbox for Intermediate Data Storage & Dependency Resolution

## Status
**Proposed**

## Problem Statement

When a workflow has **data dependencies** between tasks (e.g., `T0 → T1` with a `DataDependency` of size > 0), task `T1` needs the output files produced by task `T0` before it can start. In the current architecture:

- Data dependencies are modeled as `DataDependency` edges in the DAG and scheduled as `LinkReservation`s (network bandwidth reservations).
- But the **actual file data** is never transported — the dependency is purely a scheduling constraint (T1 starts after T0 finishes), not a data transfer.

The epic requires that:
1. When a task completes, its **output data** is stored in a **letterbox** (intermediate storage on the main/VRM system).
2. When a dependent task is about to be committed, the VRM retrieves the required data from the letterbox and includes it in the commit payload.
3. For **data dependencies** (as opposed to sync dependencies), this transfer can be **asynchronous** — data can be temporarily stored before being forwarded to the target.
4. For **sync dependencies**, data transfer is immediate/bidirectional (see [US_data_sync_streaming](US_data_sync_streaming.md)).

This US covers the **letterbox mechanism** and **data dependency resolution**. The letterbox acts as a temporary store on the VRM side, holding completed-task outputs until dependent tasks need them.

## Goal

Implement a **letterbox** (in-process data store) that:

1. **Stores output data** when a task completes — keyed by `(ReservationId, port_name)`.
2. **Provides data on request** — when a dependent task is being committed, the commit flow queries the letterbox for all required inputs and includes them in the payload.
3. **Resolves `dataIn` → `dataOut` mappings** — for each `dataIn` on a task, find the corresponding `dataOut` from the source task (via the `DataDependency` edges) and retrieve the data from the letterbox.
4. **Handles the entry-task edge case** — when a task is the workflow entry point and has `dataIn` with `sourceReservation: "EXTERNAL"`, the data comes from `input_data_files` (already handled in [US_data_commit_with_payload](US_data_commit_with_payload.md) AD-5), not from the letterbox.
5. **Cleans up** — data for a completed workflow can be evicted from the letterbox (optional for this US, explicit cleanup can be deferred).
6. **Only for SlurmRms** — simulator RMS variants are unaffected.

## Resolved Architectural Decisions

### AD-1: Letterbox as an In-Memory Store with Optional Disk Backing

**Decision:** For the first iteration, the letterbox is an `Arc<RwLock<HashMap<LetterboxKey, LetterboxEntry>>>` stored on the `SlurmRms` struct. Each entry contains the port name, producing reservation ID, file name, and raw bytes. Disk backing (writing to `data/letterbox/`) is optional and can be added later.

```rust
#[derive(Hash, Eq, PartialEq)]
struct LetterboxKey {
    reservation_id: ReservationId,
    port_name: String,       // matches dataOut.name
}

struct LetterboxEntry {
    file_name: String,
    content: Vec<u8>,
    produced_at: Instant,
}
```

**Rationale:** In-memory is simple, fast, and sufficient for single-VRM deployments. The `Arc<RwLock<HashMap<...>>>` pattern is already used in `SlurmRms` (e.g., `task_mapping`, `node_schedule`).

**Scope:**
- `src/vrm/rms/slurm_rms/letterbox.rs` — new module
- `src/vrm/rms/slurm_rms/slurm_base.rs` — add `letterbox` field

### AD-2: Letterbox Population — Polling for READY Files via the Gateway

**Decision:** This US introduces a **polling mechanism** on `SlurmRms` that periodically checks the Gateway for completed jobs (those with a `READY` file). When a job is ready, the Gateway returns the `output.json` content, and the letterbox extracts data files from it. 

Since the Gateway service is not yet built (it's part of a future US), **this US includes a simplified polling path** that can be tested with Mocks: a `LetterboxPopulator` trait with a method `check_ready_jobs() -> Vec<(ReservationId, Vec<DataFilePayload>)>`.

**Scope:**
- `src/vrm/rms/slurm_rms/letterbox.rs` — `LetterboxPopulator` trait and polling logic

### AD-3: Data Dependency Resolution at Commit Time

**Decision:** When `SlurmRms::commit()` is called for a task, after reading `input_data_files`, it queries the letterbox for each `dataIn` dependency:

1. Look up the `DataDependency` edges where `target_node == this_task`.
2. For each such edge, construct a `LetterboxKey { reservation_id: source_node, port_name: dep.port_name }`.
3. Query the letterbox — if the entry exists, add its content to the `TaskSubmission.data` payload.
4. If the entry does **not** exist yet (source task hasn't finished), **block or defer** — for this US, return an error/log warning. Future iterations can implement waiting/retry.

**Scope:**
- `src/vrm/rms/slurm_rms/base_rms.rs` — `commit()` method

### AD-4: Integration with Workflow DAG

**Decision:** The `SlurmRms` needs access to the `Workflow` DAG to resolve data dependencies. This is done by storing a reference to the `ReservationStore` (already present) and looking up `DataDependency` edges via `WorkflowNode` adjacency lists (`incoming_data`/`outgoing_data`).

**Scope:**
- `src/vrm/rms/slurm_rms/base_rms.rs` — DAG traversal for dependency resolution

### AD-5: Simulator RMS — No Letterbox

**Decision:** `RmsSimulator` et al. do not have a letterbox. Data dependency resolution for simulators remains purely schedule-based (no actual data transfer).

---

## Implementation Checklist

### Phase 1: Letterbox Data Structure
- [ ] Create `src/vrm/rms/slurm_rms/letterbox.rs` module
- [ ] Define `LetterboxKey`, `LetterboxEntry`, and `Letterbox` (wrapping `Arc<RwLock<HashMap<...>>>`)
- [ ] Implement `Letterbox::insert(key, entry)`, `Letterbox::get(key) -> Option<LetterboxEntry>`, `Letterbox::remove(key)`
- [ ] Add `letterbox: Letterbox` field to `SlurmRms`

### Phase 2: Letterbox Population (Simplified)
- [ ] Define `LetterboxPopulator` trait with `async fn check_ready_jobs(&self) -> Vec<(ReservationId, Vec<DataFilePayload>)>`
- [ ] Implement a `MockLetterboxPopulator` for testing
- [ ] Implement a polling loop (spawned as a background tokio task on `SlurmRms`) that calls `check_ready_jobs()` periodically
- [ ] On receiving ready jobs, insert their output data into the letterbox

### Phase 3: Data Dependency Resolution in `commit()`
- [ ] In `SlurmRms::commit()`, after handling `input_data_files`, resolve `dataIn` dependencies
- [ ] For each `dataIn` with a non-`EXTERNAL` `sourceReservation`:
  - Look up the `DataDependency` by `(source_reservation_id, port_name)`
  - Query letterbox for `LetterboxKey { reservation_id: source_id, port_name }`
  - If found: add to `TaskSubmission.data`
  - If not found: log `warn!`, set state to `Rejected` (or retry N times)
- [ ] Ensure `EXTERNAL` data-in is NOT routed through the letterbox (already handled)

### Phase 4: Cleanup
- [ ] Optionally: when a workflow reaches terminal state (`Finished`), evict its entries from the letterbox
- [ ] Add a `Letterbox::evict_workflow(workflow_id)` method

### Phase 5: Tests
- [ ] Unit test: Letterbox insert/get/remove
- [ ] Unit test: Letterbox key collision — same port name, different reservation → different entries
- [ ] Unit test: Data dependency resolution — letterbox has data → included in payload
- [ ] Unit test: Data dependency resolution — letterbox missing data → Rejected + warning
- [ ] Integration test: Mock populator feeds data → commit includes letterbox data
- [ ] Integration test: Simulator workflow with data deps — no letterbox, still works

---

## Test Cases

### TC-4.1: Letterbox Insert and Retrieve

**Objective:** Verify basic letterbox operations.

**Given:**
- An empty `Letterbox`

**When:**
- `insert(LetterboxKey { reservation_id: res_a, port_name: "out_data" }, LetterboxEntry { file_name: "result.dat", content: b"hello" })` is called
- `get(LetterboxKey { reservation_id: res_a, port_name: "out_data" })` is called

**Then:**
- The second call returns `Some(LetterboxEntry)` with `file_name: "result.dat"` and `content: b"hello"`
- `get` with a different key returns `None`

---

### TC-4.2: Letterbox Key Isolation

**Objective:** Verify that different reservations with the same port name are stored independently.

**Given:**
- An empty `Letterbox`

**When:**
- Entry 1: key `(res_a, "output")` → content `b"aaa"`
- Entry 2: key `(res_b, "output")` → content `b"bbb"`

**Then:**
- `get((res_a, "output"))` returns `b"aaa"`
- `get((res_b, "output"))` returns `b"bbb"`
- Both entries coexist

---

### TC-4.3: Data Dependency Resolution — Letterbox Has Data

**Objective:** Verify that a task with a `dataIn` dependency gets data from the letterbox at commit time.

**Given:**
- Workflow: `T0 → T1` (data dependency, port `"out_T0"`, size 100)
- Letterbox contains: key `(T0_res_id, "out_T0")` → file `"t0_out.dat"`, content `b"some_data"`
- `SlurmRms` with a mock HTTP server

**When:**
- `SlurmRms::commit(T1_res_id)` is called

**Then:**
- The `TaskSubmission.data` payload includes a `DataFilePayload` with `file_name: "t0_out.dat"` and `content_base64` encoding `b"some_data"`
- The `sourceReservation` in T1's `dataIn` is resolved through the letterbox (not treated as `EXTERNAL`)
- Reservation `T1` reaches `Committed`

---

### TC-4.4: Data Dependency Resolution — Letterbox Missing Data

**Objective:** Verify graceful handling when a dependent task is committed before its source task has finished.

**Given:**
- Workflow: `T0 → T1` (data dependency)
- Letterbox does NOT contain `(T0_res_id, "out_T0")` (T0 hasn't finished)
- `logtest::Logger` capturing logs

**When:**
- `SlurmRms::commit(T1_res_id)` is called

**Then:**
- Reservation `T1` transitions to `Rejected` (or the commit returns `false`)
- A `warn!` log message indicates missing letterbox data, naming the source reservation and port
- No panic

---

### TC-4.5: Entry Task with `EXTERNAL` Data-In Bypasses Letterbox

**Objective:** Verify that `dataIn` with `sourceReservation: "EXTERNAL"` does NOT query the letterbox.

**Given:**
- Workflow entry task `T0` with `dataIn: [{ sourceReservation: "EXTERNAL", sourcePort: "config", file: "cfg.json" }]`
- `input_data_files: [{ name: "config", file: "cfg.json" }]` on T0 (file exists)
- Letterbox does NOT contain any entry for `"config"`

**When:**
- `SlurmRms::commit(T0_res_id)` is called

**Then:**
- The commit succeeds (data comes from `input_data_files`, not letterbox)
- The letterbox is never queried for this data-in
- The payload includes `cfg.json` content from the filesystem

---

### TC-4.6: Mock Populator Populates Letterbox

**Objective:** Verify that the `MockLetterboxPopulator` correctly feeds data into the letterbox.

**Given:**
- A `SlurmRms` with a `MockLetterboxPopulator` configured to return:
  - `(res_a, [DataFilePayload { file_name: "out.dat", content_base64: base64("result") }])`
- Background polling loop running

**When:**
- One polling cycle completes

**Then:**
- `letterbox.get((res_a, "out.dat"))` returns `Some(...)` with content `b"result"`

---

### TC-4.7: Simulator RMS Workflow with Data Dependencies (Regression)

**Objective:** Verify that data dependencies on `RmsSimulator` still work purely as scheduling constraints.

**Given:**
- Two `RmsSimulator` AcIs
- A workflow with 2 tasks and 1 data dependency (`T0 → T1`, size = 500)

**When:**
- `VrmManager::run_vrm()` processes the workflow

**Then:**
- Both tasks reach `ReserveAnswer` or `Committed`
- The `LinkReservation` for the data dependency is scheduled
- No letterbox interaction (simulator path is unchanged)
- No file I/O in the commit path

---

## Dependencies

- **Depends on:** [US_data_commit_with_payload](US_data_commit_with_payload.md) — the commit flow must support `data` payload to include letterbox data. [US_data_gateway_job](US_data_gateway_job.md) — the READY mechanism is how the populator knows jobs are done.
- **Blocks:** [US_data_sync_streaming](US_data_sync_streaming.md) — sync dependency data transfer can leverage the letterbox for the non-streaming portion.

## Effort Estimate

- **Phase 1 (data structure):** ~1.5h — Letterbox struct, insert/get/remove, RwLock
- **Phase 2 (populator):** ~2.5h — trait, mock, polling loop, tokio spawn
- **Phase 3 (dependency resolution):** ~2h — DAG traversal, letterbox query, payload assembly
- **Phase 4 (cleanup):** ~0.5h — eviction logic
- **Phase 5 (tests):** ~1.5h — 7 test cases

**Total:** ~8h
