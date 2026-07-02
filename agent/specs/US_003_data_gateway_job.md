# US_data_gateway_job — Cluster Gateway Job & Standardized Job Directory

## Status
**Proposed**

## Problem Statement

Currently, `SlurmRms` submits jobs directly to the Slurm REST API via `SlurmRestApiClient`. The `TaskSubmission` payload contains a `script` field (the executable path), and Slurm runs that script on a compute node. There is:

- **No standardized job directory** — Slurm jobs write outputs to arbitrary paths specified in `output_path`/`error_path`.
- **No completion signaling mechanism** — the VRM has no reliable way to know when a job's output files are fully written and ready for collection.
- **No isolation between compute and infrastructure concerns** — the compute job runs directly in Slurm without any wrapper, so the VRM cannot enforce a standard lifecycle (input staging, execution, output signaling, error handling).

The epic defines a **Gateway Job pattern** that wraps every compute task in a standardized lifecycle:

1. A **wrapper script** (bash) creates a standardized directory `/data/jobs/<job-id>/`.
2. The wrapper writes `input.json`, runs the user's script, captures exit codes, writes `output.json`, and creates a `READY` marker file.
3. The **Cluster Gateway** (a separate service inside the cluster, future US) monitors the `READY` file to know when outputs are available.
4. The compute job itself is "dumb" — it only reads from and writes to the standardized directory, with no network access.

## Goal

For this US, implement the **wrapper script and standardized directory structure** on the Slurm side:

1. Every job submitted by `SlurmRms` is wrapped in a **gateway job script** (bash) that:
   - Creates `/data/jobs/<slurm-job-id>/` (or a configurable base path).
   - Writes `input.json` from the data payload received at commit time.
   - Writes a `state` file (`"running"` on start).
   - Executes the user's actual task script (`user_job.sh`).
   - Captures the exit code.
   - Writes `output.json` (at minimum, wraps the exit code and stdout/stderr paths).
   - Creates the `READY` marker file on successful completion.
   - Writes `"completed"` or `"failed"` to the `state` file.
2. The `TaskSubmission` sent to Slurm now uses this wrapper script as the `script`, with the user's original `task_path` passed as data (written into the job directory as `user_job.sh`).
3. The `READY` mechanism is implemented and testable — even if the Gateway service isn't built yet, the filesystem state after job completion can be verified.
4. **Only for SlurmRms** — simulator RMS variants are unaffected.

## Resolved Architectural Decisions

### AD-1: Gateway Wrapper Script as an Embedded Constant

**Decision:** The gateway wrapper script is embedded as a `const` string in the Rust source (e.g., `GATEWAY_WRAPPER_SCRIPT` in `slurm_rms/`). At commit time, the wrapper script is customized with the job's specific parameters (job ID, user script content, input data) and submitted as the `script` field of `TaskSubmission`.

**Rationale:** Embedding avoids external file dependencies. The script is small (~50 lines of bash). Future iterations can make the script path configurable.

**Scope:**
- `src/vrm/rms/slurm_rms/gateway_wrapper.rs` — new module with the wrapper script template

### AD-2: Standardized Directory Structure

**Decision:** Every job uses the following directory layout:

```
/data/jobs/<slurm-job-id>/
├── input.json          # Input data (from TaskSubmission.data payload)
├── user_job.sh         # The user's original task script
├── output.json         # Job results (at minimum: exit_code, stdout, stderr)
├── state               # Current state: "running" | "completed" | "failed"
├── stdout.log          # Captured standard output
├── stderr.log          # Captured standard error
└── READY               # Empty marker file — signals all outputs are written
```

The base path `/data/jobs/` is configurable via a constant or environment variable.

**Scope:**
- Wrapper script logic
- Documentation in code comments

### AD-3: `TaskSubmission.script` Becomes the Wrapper, User Script Becomes Data

**Decision:** The `script` field in `TaskSubmission` is changed from the user's `task_path` to the generated gateway wrapper script. The user's original `task_path` content is read and embedded as a data file (`user_job.sh`) in the `data` payload. This is a **breaking change** for SlurmRms, but the epic explicitly requires this architectural shift.

**Scope:**
- `src/vrm/rms/slurm_rms/base_rms.rs` — commit logic restructured

### AD-4: `TaskSubmission.script` Is Generated Per-Commit

**Decision:** The wrapper script is not a static template — it is generated per-commit with the specific `$SLURM_JOB_ID` placeholder replaced by the actual Slurm job ID returned after submission. Wait — there's a chicken-and-egg problem: we need the job ID to fill the template, but we get the job ID after submission. 

**Resolution:** The wrapper script uses `$SLURM_JOB_ID` (the environment variable Slurm sets inside the job) rather than a hardcoded ID. The script references `$SLURM_JOB_ID` at runtime. This is the standard Slurm convention.

**Scope:**
- Wrapper script uses `$SLURM_JOB_ID` environment variable

### AD-5: Simulator RMS Variants — No Changes

**Decision:** Same as in [US_data_commit_with_payload](US_data_commit_with_payload.md) AD-4 — simulators use the default `Rms::commit()` and are unaffected.

**Scope:**
- No changes in simulator code

---

## Implementation Checklist

### Phase 1: Gateway Wrapper Script
- [ ] Create `src/vrm/rms/slurm_rms/gateway_wrapper.rs` module
- [ ] Define `GATEWAY_WRAPPER_SCRIPT: &str` — the bash template using `$SLURM_JOB_ID`
- [ ] Script must: create directory, write `input.json`, write `state=running`, run `user_job.sh`, capture exit code, write `output.json`, write `state=completed|failed`, create `READY`
- [ ] Define `generate_wrapper_script() -> String` that returns the script as-is (uses `$SLURM_JOB_ID` at runtime)

### Phase 2: Restructure `SlurmRms::commit()`
- [ ] Read the user's `task_path` file content (the executable script)
- [ ] Add it to the `data` payload as `user_job.sh` (base64-encoded)
- [ ] Set `TaskSubmission.script` to the gateway wrapper script
- [ ] Ensure `input.json` is generated from `NodeReservation` fields and `input_data_files`
- [ ] The `input.json` payload is also included in the `data` field as base64

### Phase 3: `input.json` and `output.json` Schema
- [ ] Define `JobInput` struct: `{ task_path, environment, args, data_files: { name: path } }`
- [ ] Define `JobOutput` struct: `{ exit_code: i32, stdout: String, stderr: String }`
- [ ] These are serialized/deserialized by both the Rust side (for verification) and the bash script (via `jq` or heredocs)

### Phase 4: Integration with Existing Slurm Tests
- [ ] Update existing Slurm tests (`test_slurm_rms`, `test_slurm_rest_api`) to account for the new `script` content
- [ ] Since real Slurm tests require Docker, add a note that script behavior must be verified manually or via a mock

### Phase 5: Tests
- [ ] Unit test: `generate_wrapper_script()` produces valid bash referencing `$SLURM_JOB_ID`
- [ ] Unit test: `TaskSubmission` with gateway wrapper — `script` field contains the wrapper, not the user path
- [ ] Unit test: `JobInput` / `JobOutput` round-trip serialization
- [ ] Integration test: wrapper script executes correctly and creates directory structure (run as local bash subprocess in test)
- [ ] Integration test: `READY` file is created only on successful completion

---

## Test Cases

### TC-3.1: Gateway Wrapper Script Generates Valid Bash

**Objective:** Verify that the generated wrapper script is syntactically valid bash.

**Given:**
- The `GATEWAY_WRAPPER_SCRIPT` constant in Rust

**When:**
- The script is written to a temp file and checked with `bash -n`

**Then:**
- `bash -n` exits with code 0 (no syntax errors)
- The script references `$SLURM_JOB_ID` (grep confirms)
- The script contains the expected lifecycle steps: `mkdir`, `echo running > state`, `bash user_job.sh`, `EXIT_CODE=$?`, `echo $EXIT_CODE > ...`, `echo completed/failed > state`, `touch READY`

---

### TC-3.2: Wrapper Script Creates Standardized Directory (Local Execution)

**Objective:** Verify that the wrapper script, when executed locally with a fake `$SLURM_JOB_ID`, creates the correct directory structure.

**Given:**
- A temporary directory `/tmp/test-gateway/` as the base
- `SLURM_JOB_ID=test-job-42` exported
- `JOB_DIR` overridden to `/tmp/test-gateway/jobs/$SLURM_JOB_ID`
- A simple `user_job.sh`: `#!/bin/bash\necho "hello world"\nexit 0`

**When:**
- The wrapper script is executed with `bash`

**Then:**
- Directory `/tmp/test-gateway/jobs/test-job-42/` exists
- `state` file contains `"completed"`
- `exit_code` file contains `"0"`
- `READY` file exists (empty)
- `stdout.log` contains `"hello world"`
- `output.json` exists with valid JSON

---

### TC-3.3: Wrapper Script Handles User Job Failure

**Objective:** Verify that when the user's task fails, the state is `"failed"` and `READY` is still created (so the gateway can collect error info).

**Given:**
- A `user_job.sh` that does `exit 1`

**When:**
- The wrapper script executes

**Then:**
- `state` file contains `"failed"`
- `exit_code` file contains `"1"`
- `READY` file **still exists** (gateway should still collect the failure output)
- `stderr.log` captures any error output

---

### TC-3.4: `SlurmRms::commit()` Uses Gateway Wrapper as Script

**Objective:** Verify that the commit flow submits the wrapper script, not the user's raw `task_path`.

**Given:**
- A `NodeReservation` with `task_path: "/bin/my_task.sh"` and `input_data_files: [...]`
- A `SlurmRms` connected to a mock HTTP server that records requests

**When:**
- `SlurmRms::commit(reservation_id)` is called

**Then:**
- The `TaskSubmission.script` field contains the gateway wrapper script (not `"/bin/my_task.sh"`)
- The user's script content appears in the `data` payload as `user_job.sh` (base64-encoded)
- The `input.json` data file is in the `data` payload
- The mock server receives a valid JSON body

---

### TC-3.5: Existing Slurm Tests Still Pass With Modified `script`

**Objective:** Ensure that the change to `TaskSubmission.script` does not break existing Slurm integration tests.

**Given:**
- Existing Slurm test fixtures (they use `/bin/sleep` as task_path)
- Docker Slurm cluster running on `localhost:6820`

**When:**
- `test_slurm_rms` tests are executed

**Then:**
- Tests pass (the wrapper script executes `/bin/sleep` as the user job)
- The job completes on Slurm with exit code 0
- Reservation state reaches `Committed`

---

### TC-3.6: Simulator RMS Unaffected by Gateway Wrapper (Regression)

**Objective:** Same pattern as TC-2.5 — simulators must not be affected.

**Given:**
- `RmsSimulator` AcI with a workflow

**When:**
- `VrmManager::run_vrm()` processes the workflow

**Then:**
- All reservations reach `ReserveAnswer` or `Committed`
- No gateway wrapper script logic is triggered
- No file I/O in the simulator commit path

---

## Dependencies

- **Depends on:** [US_data_commit_with_payload](US_data_commit_with_payload.md) — the `data` payload field must exist to carry `user_job.sh` and `input.json`.
- **Blocks:** [US_data_letterbox_and_deps](US_data_letterbox_and_deps.md) — the letterbox reads from the standardized job directory.

## Effort Estimate

- **Phase 1 (wrapper script):** ~2h — bash template, testing with `bash -n`, local execution
- **Phase 2 (restructure commit):** ~3h — change `script` field, add user script to data, generate `input.json`
- **Phase 3 (input/output schemas):** ~1.5h — Rust structs, serde, JSON generation
- **Phase 4 (integration with existing tests):** ~1.5h — adapt Slurm tests, mock server adjustments
- **Phase 5 (new tests):** ~2h — 6 test cases including local bash execution

**Total:** ~10h
