# US_data_workflow_data_input — Workflow Data File Input & Staging
## Status
**Proposed**

## Problem Statement

Currently, workflows submitted to the VRM system contain only task metadata (executable path, CPU count, duration) and abstract data dependency declarations (`dataOut`/`dataIn` with port names and sizes). There is **no mechanism** for users to attach actual data files to a workflow, and no convention for where such files should reside. The `NodeReservation` stores only a `task_path` (the executable), but the actual input data that the executable consumes is never transported to the HPC cluster.

For data transport to function (see [US_data_commit_with_payload](US_data_commit_with_payload.md)), the system first needs a way for users to declare which data files belong to a workflow and a convention for where those files are staged before commit.

## Goal

Extend the workflow JSON schema and the internal data model so that:

1. Users can declare **data file references** in the workflow JSON — each task may reference one or more input files by name.
2. A **well-known staging directory** convention is established: `data/workflow_data/{WorkflowID}/` — all data files for a workflow reside here.
3. At **workflow load time** (parsing), the system validates that declared data files actually exist on disk.
4. The `NodeReservation` (or an associated structure) stores the **list of data file paths** that must accompany the task to the cluster.
5. The feature is **limited to SlurmRms** — simulator RMS variants are unaffected and continue to work with their existing in-memory behavior.

## Resolved Architectural Decisions

### AD-1: Extend `NodeReservationDto` with an Optional `input_data` Field

**Decision:** Add an `input_data: Option<Vec<InputDataFileDto>>` field to `NodeReservationDto` (and the corresponding `TaskDto` in workflow JSON). Each entry contains a logical `name` and a `file` (filename relative to the workflow data directory). This is separate from `dataIn`/`dataOut` which describe inter-task dependencies — `input_data` describes **user-provided** data files staged alongside the workflow.

```rust
pub struct InputDataFileDto {
    /// Logical name for the data file (e.g., "config", "model_weights")
    pub name: String,
    /// Filename relative to data/workflow_data/{WorkflowID}/ (e.g., "config.json")
    pub file: String,
}
```

**Scope:**
- `src/loader/dto.rs` — add `InputDataFileDto`
- `src/vrm/` — propagate into `NodeReservation`
- JSON workflow files

### AD-2: Staging Directory Convention — `data/workflow_data/{WorkflowID}/`

**Decision:** Data files must be placed in `data/workflow_data/{WorkflowID}/` where `WorkflowID` is the `id` field of the workflow in the JSON config. The `file` field in `InputDataFileDto` is relative to this directory. Validation at parse time checks that the resolved absolute path exists.

**Rationale:** This keeps data files co-located with the repository and discoverable by workflow ID. It also supports containerized deployments where this directory can be volume-mounted.

**Scope:**
- `data/workflow_data/` — new directory convention
- `src/loader/` — validation logic

### AD-3: Store Resolved Paths in `NodeReservation`

**Decision:** After validation, resolve each `InputDataFileDto.file` to an absolute path and store it in the `NodeReservation` as `input_data_files: Vec<InputDataFile>`. The absolute path is needed because the data transport layer (future US) must read these files at commit time regardless of the current working directory.

**Scope:**
- `src/vrm/reservation/node_reservation.rs` — add `input_data_files` field
- `src/vrm/reservation/workflow.rs` — propagate during `create_form_dto()`

### AD-4: Simulator RMS Variants Ignore `input_data_files`

**Decision:** `RmsSimulator`, `RmsNodeSimulator`, and `RmsNetworkSimulator` will **not** read or transport `input_data_files`. They continue to work with task metadata only. The `input_data_files` field is purely a `SlurmRms` concern. This is enforced by the `Rms` trait's default `commit()` implementation remaining unchanged, while `SlurmRms::commit()` is extended in [US_data_commit_with_payload](US_data_commit_with_payload.md).

**Scope:**
- No changes needed in simulator RMS implementations

---

## Implementation Checklist

### Phase 1: Data Model Changes
- [ ] Add `InputDataFileDto` struct to `src/loader/dto.rs` (or wherever `NodeReservationDto` is defined)
- [ ] Add `input_data: Option<Vec<InputDataFileDto>>` field to `NodeReservationDto`
- [ ] Add `InputDataFile` struct (with resolved absolute path) to `src/vrm/reservation/node_reservation.rs`
- [ ] Add `input_data_files: Vec<InputDataFile>` field to `NodeReservation`

### Phase 2: Validation at Parse Time
- [ ] In `Workflow::create_form_dto()`, after building each `NodeReservation`, resolve each `InputDataFileDto.file` against `data/workflow_data/{WorkflowID}/`
- [ ] If any resolved path does not exist on disk, return an `Err` with a descriptive error message listing the missing file(s)
- [ ] Store resolved absolute paths in `NodeReservation.input_data_files`

### Phase 3: Workflow JSON Schema Updates
- [ ] Update example workflow JSON files to demonstrate the `input_data` field
- [ ] Create a minimal test workflow JSON (`data/test/workflow_with_data.json`) with `input_data` declarations
- [ ] Create corresponding test data files under `data/workflow_data/{WorkflowID}/`

### Phase 4: Unit Tests
- [ ] Test: Parse workflow with `input_data` — all files exist → success
- [ ] Test: Parse workflow with `input_data` — one file missing → error with descriptive message
- [ ] Test: Parse workflow without `input_data` (backward compatibility) → success, empty `input_data_files`
- [ ] Test: Resolved paths are absolute and correct

---

## Test Cases

### TC-1.1: Workflow JSON with Valid `input_data` Parses Successfully

**Objective:** Verify that a workflow declaring `input_data` with existing files parses without error.

**Given:**
- A workflow JSON file with `id: "test-wf-1"` containing a task `T0` with:
  ```json
  "input_data": [
    { "name": "config", "file": "config.json" },
    { "name": "weights", "file": "model.bin" }
  ]
  ```
- Files `data/workflow_data/test-wf-1/config.json` and `data/workflow_data/test-wf-1/model.bin` exist on disk

**When:**
- `Workflow::create_form_dto()` processes the workflow

**Then:**
- The workflow is created successfully
- `NodeReservation` for `T0` has `input_data_files` with 2 entries
- Each entry has the correct absolute path
- No error is returned

---

### TC-1.2: Missing Data File Returns Descriptive Error

**Objective:** Verify that a missing declared data file produces a clear, actionable error.

**Given:**
- A workflow JSON with `id: "test-wf-2"` containing a task `T0` with:
  ```json
  "input_data": [
    { "name": "config", "file": "missing.json" }
  ]
  ```
- File `data/workflow_data/test-wf-2/missing.json` does **not** exist

**When:**
- `Workflow::create_form_dto()` processes the workflow

**Then:**
- An `Err` is returned
- The error message contains: the workflow ID (`test-wf-2`), the task ID (`T0`), and the missing filename (`missing.json`)
- No partial workflow is created

---

### TC-1.3: Backward Compatibility — Workflow Without `input_data`

**Objective:** Verify that existing workflow JSON files without the `input_data` field continue to parse correctly.

**Given:**
- A workflow JSON file with **no** `input_data` field on any task (e.g., `data/test/workflow_cross_rms_10_nodes.json`)

**When:**
- `Workflow::create_form_dto()` processes the workflow

**Then:**
- The workflow is created successfully
- All `NodeReservation`s have empty `input_data_files` vectors
- No errors or warnings related to `input_data`

---

### TC-1.4: Multiple Tasks with Separate Data Files

**Objective:** Verify that different tasks in the same workflow can declare different data files.

**Given:**
- A workflow with `id: "test-wf-3"` containing tasks `T0` and `T1`
- `T0` declares `input_data: [{ "name": "a", "file": "a.dat" }]`
- `T1` declares `input_data: [{ "name": "b", "file": "b.dat" }]`
- Both files exist under `data/workflow_data/test-wf-3/`

**When:**
- `Workflow::create_form_dto()` processes the workflow

**Then:**
- `T0`'s `NodeReservation.input_data_files` contains only `a.dat` (absolute path)
- `T1`'s `NodeReservation.input_data_files` contains only `b.dat` (absolute path)
- Both paths point to the correct workflow data directory

---

### TC-1.5: Simulator RMS Ignores `input_data_files` (Integration)

**Objective:** Verify that a workflow with `input_data` can still be scheduled and committed on `RmsSimulator` without any data transport side effects.

**Given:**
- A VRM system with one `RmsSimulator` AcI
- A workflow with `input_data` declarations (files exist)
- `GlobalClock` in simulation mode

**When:**
- `VrmManager::run_vrm()` processes the workflow with `ReservationProceeding::Commit`

**Then:**
- Workflow reaches `ReservationState::ReserveAnswer` or `Committed`
- All task reservations reach `ReserveAnswer` or `Committed`
- No errors or warnings about data transport (simulator ignores `input_data_files`)
- No reservation is `Rejected`

---

## Dependencies

- **Depends on:** Nothing — this is the foundational US for data transport.
- **Blocks:** [US_data_commit_with_payload](US_data_commit_with_payload.md) — the commit flow reads `input_data_files` to transport data.

## Effort Estimate

- **Phase 1 (data model):** ~1.5h — add structs, fields, serde derives
- **Phase 2 (validation):** ~2h — path resolution, error messages, integration into `create_form_dto()`
- **Phase 3 (JSON schema + fixtures):** ~1h — example files, test data
- **Phase 4 (unit tests):** ~1.5h — 5 test cases

**Total:** ~6h
