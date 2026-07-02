# US_data_commit_with_payload — Data Handover at Task Commit Time

## Status
**Proposed**

## Problem Statement

Currently, `SlurmRms::commit()` (in `src/vrm/rms/slurm_rms/base_rms.rs`) constructs a `TaskSubmission` payload containing only job metadata and a `script` (the executable path). The actual **input data files** that a task needs are never transported to the cluster. The commit flow sends a JSON body to the Slurm REST API with fields like `cpus_per_task`, `time_limit`, and `script`, but no data payload.

The epic requires that data files be handed over at the moment a task is **committed**. For SlurmRms, this means the data must be included in or alongside the job submission so that the cluster node can access the input files when the job runs.

## Goal

Extend `SlurmRms::commit()` so that:

1. When a `NodeReservation` has `input_data_files` (from [US_data_workflow_data_input](US_data_workflow_data_input.md)), those files are **read and serialized** into the commit payload.
2. The `TaskSubmission` struct is extended to carry a **data payload** (file name + base64-encoded content, or similar inline representation).
3. The data arrives at the cluster node alongside the job script — the receiving side (Gateway job, see [US_data_gateway_job](US_data_gateway_job.md)) writes the files into the standardized job directory.
4. **Simulator RMS variants** (`RmsSimulator`, `RmsNodeSimulator`, `RmsNetworkSimulator`) are **completely unaffected** — the `Rms` trait's default `commit()` implementation and the `VrmComponentManager::commit_at_component()` logic for `DUMMY_COMPONENT_ID` must still work identically.
5. The feature is **only active for SlurmRms**.

## Resolved Architectural Decisions

### AD-1: Extend `TaskSubmission` with an Optional `data` Field

**Decision:** Add `data: Option<Vec<DataFilePayload>>` to `TaskSubmission` (or to `JobProperties`). Each `DataFilePayload` contains the filename and the file content as a base64-encoded string. This keeps the payload self-contained and compatible with JSON-over-HTTP transport.

```rust
pub struct DataFilePayload {
    /// Relative filename (e.g., "config.json")
    pub file_name: String,
    /// Base64-encoded file content
    pub content_base64: String,
}
```

**Rationale:** Base64 encoding ensures binary files survive JSON serialization. For large files this is not ideal long-term, but the epic says "für das erste nur dummy implementierung" — a simple inline approach is appropriate for the first iteration. Future US can optimize with chunked transfer or shared storage references.

**Scope:**
- `src/vrm/rms/slurm_rms/api_client/task_properties.rs` — add `DataFilePayload`, extend `TaskSubmission`

### AD-2: Read Files at Commit Time in `SlurmRms::commit()`

**Decision:** Inside `SlurmRms::commit()`, after constructing the `TaskSubmission` from `NodeReservation` fields, iterate over `node_reservation.input_data_files`. For each file, read its content, base64-encode it, and add to `payload.data`. The file reading is synchronous (not async) since `commit()` itself is called from a synchronous context and spawns the actual HTTP call as an async task.

**Scope:**
- `src/vrm/rms/slurm_rms/base_rms.rs` — `SlurmRms::commit()` method

### AD-3: Graceful Handling of Missing or Unreadable Files

**Decision:** If a file listed in `input_data_files` cannot be read at commit time (e.g., it was deleted between parse and commit), log a `warn!` and set the reservation state to `Rejected`. Do **not** panic. The error message must include the reservation ID, the filename, and the OS error.

**Scope:**
- `src/vrm/rms/slurm_rms/base_rms.rs` — error handling in commit

### AD-4: Simulator RMS Variants — No Changes Required

**Decision:** The default `Rms::commit()` implementation in `rms.rs` (which sets state `Committed → Finished`) is **not modified**. `RmsSimulator`, `RmsNodeSimulator`, and `RmsNetworkSimulator` defer to this default and remain unaffected. The `VrmComponentManager::commit_at_component()` path for `DUMMY_COMPONENT_ID` is also unchanged. This is verified by existing simulator-based integration tests continuing to pass.

**Scope:**
- No changes in simulator RMS code — verified by existing test suite

### AD-5: `EXTERNAL` Data — Workflow Entry Tasks

**Decision:** When a `NodeReservation` has `dataIn` entries with `source_reservation: "EXTERNAL"`, these indicate that the task (typically a workflow entry point) needs data from outside the workflow. The mapping between `EXTERNAL` data-in ports and actual `input_data_files` is done by matching on the `sourcePort` name. If a `dataIn` port references `"EXTERNAL"` with a port name, and an `input_data_file` exists with a matching `name`, that file is included in the commit payload.

**Scope:**
- `src/vrm/rms/slurm_rms/base_rms.rs` — `EXTERNAL` resolution logic

---

## Implementation Checklist

### Phase 1: Extend `TaskSubmission` Payload
- [ ] Add `DataFilePayload` struct to `task_properties.rs` with `Serialize`/`Deserialize`
- [ ] Add `data: Option<Vec<DataFilePayload>>` field to `TaskSubmission`
- [ ] Ensure the JSON serialization of existing fields is unchanged when `data` is `None`

### Phase 2: Implement File Reading in `SlurmRms::commit()`
- [ ] After constructing `TaskSubmission`, read `node_reservation.input_data_files`
- [ ] For each file: `std::fs::read()`, base64-encode (`base64` crate or manual), create `DataFilePayload`
- [ ] Append to `payload.data`
- [ ] Handle `EXTERNAL` data-in ports: match against `input_data_files` by `sourcePort` name
- [ ] On read failure: `log::warn!`, set state to `Rejected`, return early

### Phase 3: Error Handling & Logging
- [ ] Add descriptive warn/error log messages for:
  - File not found at commit time
  - Permission denied reading file
  - Base64 encoding failure (should not happen, but guard)
- [ ] Ensure the reservation transitions to `Rejected` (not stuck in intermediate state)

### Phase 4: Regression Tests — Simulators Unaffected
- [ ] Run existing simulator test suite (`test_aci_commit`, `test_aci_reserve`, `test_cross_rms_integration`) — all must pass
- [ ] Run existing Slurm tests (`test_slurm_rms`, `test_slurm_rest_api`) — must continue to work (they don't use `input_data_files`, so payload `data` remains `None`)

### Phase 5: New Tests — Data Payload in Commit
- [ ] Unit test: `TaskSubmission` with `data: None` serializes identically to current format
- [ ] Unit test: `TaskSubmission` with `data: Some(...)` includes base64-encoded file content
- [ ] Integration test: `SlurmRms::commit()` with `input_data_files` — verify payload includes data (via mock or recorded HTTP)
- [ ] Integration test: `SlurmRms::commit()` with missing file — state becomes `Rejected`, warning logged

---

## Test Cases

### TC-2.1: `TaskSubmission` Backward Compatibility (No Data)

**Objective:** Verify that adding the optional `data` field does not break the existing JSON serialization.

**Given:**
- A `NodeReservation` with **no** `input_data_files`
- Standard task metadata (task_path, cpus, duration, etc.)

**When:**
- `SlurmRms::commit()` constructs the `TaskSubmission`

**Then:**
- The serialized JSON does **not** contain a `"data"` key (or contains `"data": null`)
- All existing fields (`job.name`, `job.cpus_per_task`, `script`, etc.) are present and unchanged
- The JSON is accepted by the Slurm REST API (existing tests pass)

---

### TC-2.2: `SlurmRms::commit()` Includes Data Files in Payload

**Objective:** Verify that declared input data files are read and included in the commit payload.

**Given:**
- A `NodeReservation` with `input_data_files` containing one file `data/workflow_data/test-wf/config.json` (content: `{"key": "value"}`)
- A running SlurmRms connected to a test Slurm cluster (or a mock HTTP server)

**When:**
- `SlurmRms::commit(reservation_id)` is called

**Then:**
- The `TaskSubmission.data` field is `Some` with one entry
- The entry has `file_name: "config.json"` and `content_base64` equal to the base64 encoding of `{"key": "value"}`
- The HTTP POST to `/slurm/{version}/job/submit` includes the `data` field in the JSON body
- The reservation state transitions to `Committed`

---

### TC-2.3: Missing File at Commit Time → Rejected

**Objective:** Verify graceful handling when a declared data file disappears between parse and commit.

**Given:**
- A `NodeReservation` with `input_data_files` pointing to a file that was deleted after parsing
- A `logtest::Logger` capturing log output

**When:**
- `SlurmRms::commit(reservation_id)` is called

**Then:**
- The reservation state transitions to `Rejected`
- A `warn!` log message is emitted containing: the reservation ID, the filename, and an OS error indicator
- No panic occurs
- No HTTP request is sent to Slurm (early return before spawn)

---

### TC-2.4: `EXTERNAL` Data Matched to Input Files

**Objective:** Verify that a workflow entry task with `dataIn` referencing `"EXTERNAL"` gets the correct file from `input_data_files`.

**Given:**
- A `NodeReservation` for task `T0` (workflow entry point) with:
  - `dataIn: [{ sourceReservation: "EXTERNAL", sourcePort: "model_weights", file: "model.bin" }]`
  - `input_data_files: [{ name: "model_weights", file: "model.bin" }]` (absolute path resolves correctly)
- File exists at the resolved path

**When:**
- `SlurmRms::commit(reservation_id)` is called

**Then:**
- The `TaskSubmission.data` includes the file content of `model.bin`
- The `file_name` in the payload is `"model.bin"` (the file basename, not the port name)

---

### TC-2.5: Simulator RMS Ignores Data Payload Entirely (Regression)

**Objective:** Verify that `RmsSimulator` (and other simulators) are completely unaffected by the `input_data_files` field.

**Given:**
- A VRM system with one `RmsSimulator` AcI
- A workflow with `input_data` declarations on some tasks
- `GlobalClock` in simulation mode

**When:**
- `VrmManager::run_vrm()` processes the workflow with `ReservationProceeding::Commit`

**Then:**
- All tasks reach `ReservationState::ReserveAnswer` or `Committed`
- The default `Rms::commit()` implementation is used (state `Committed → Finished`)
- No file I/O occurs during commit (simulator path is unchanged)
- No `Rejected` reservations
- Existing cross-RMS integration tests pass unchanged

---

## Dependencies

- **Depends on:** [US_data_workflow_data_input](US_data_workflow_data_input.md) — the `input_data_files` field on `NodeReservation` must exist.
- **Blocks:** [US_data_gateway_job](US_data_gateway_job.md) — the gateway receives the data payload and writes it to the job directory.

## Effort Estimate

- **Phase 1 (payload extension):** ~1h — small struct + serde
- **Phase 2 (file reading in commit):** ~3h — main logic, base64 handling, EXTERNAL matching
- **Phase 3 (error handling):** ~1h — logging, state transitions
- **Phase 4 (regression tests):** ~1.5h — running existing suites, confirming no regressions
- **Phase 5 (new tests):** ~1.5h — 4-5 test cases with mock HTTP

**Total:** ~8h
