# VRM-Rust Workflow — Agent Guidelines

## Project

A distributed resource-management and workflow-scheduling system written in Rust.
Entry point: `src/main.rs`. Library root: `src/lib.rs`.
Package name: `vrm_rust_workflow` (edition 2024).

## Commands

All automation lives in `agent/script/`. Do **not** use inline `sed`, `awk`, or ad‑hoc shell for editing/search/automation — invoke the scripts below.

| Task | Script | Notes |
|------|--------|-------|
| Compile check (all targets) | `agent/script/coding_check.sh` | `cargo check --all-targets --all-features` |
| Full quality gate | `agent/script/check_all.sh` | `coding_check.sh` + `verify_removed.sh` |
| Verify legacy symbols removed | `agent/script/verify_removed.sh` | greps for `RMS_GATEWAY_NAME`, `get_component_router_list` |
| Run tests (filter + timeout) | `agent/script/run_test.sh [filter] [timeout]` | default: all tests, 20 s |
| Run workflow tests | `agent/script/check_workflow_tests.sh [filter] [timeout]` | default: filter `workflow`, 30 s |
| Search codebase | `agent/script/search.sh "pattern" [grep-flags]` | recursive, skips `target/` `.git/` |
| Multi-pattern search | `agent/script/find_refs.sh "pat1\|pat2"` | across `src/` + `tests/` |
| Run binary with debugging | `agent/script/debug.sh workflow.json config.json timeout lines pattern` | |
| Smoke-test `cargo run` | `agent/script/validate.sh -- args` | greps for error/panic/fail |
| Auto-fix (fmt + clippy) | `agent/script/auto_fix.sh` | `cargo fmt` then `cargo clippy --fix --allow-dirty --allow-staged` |
| List changed files | `agent/script/changed_files.sh [ref]` | default ref: `HEAD` |

Build system: `cargo build`, `cargo test`, `cargo fmt`, `cargo clippy`.

## Architecture
The VRM system models a hierarchy of **Administrative Domain Controllers (ADCs)** that broker reservations across **AcI** components wrapping local **RMS** (resource management systems).

Key modules under `src/`:
- **`vrm/vrm_manager.rs`** — Top-level orchestrator: submits unprocessed reservations, runs the commit/finish lifecycle loop, advances the simulation clock.
- **`vrm/vrm_component/adc/`** — ADC: the grid broker. Holds a `VrmComponentManager`, a `WorkflowScheduler`, and a `ReservationStore`.
- **`vrm/vrm_component/aci.rs`** — AcI: wraps one RMS, exposes probe/reserve/commit/delete to the ADC.
- **`vrm/vrm_component/scheduler/`** — HEFT-sync workflow scheduler. Decomposes workflows into node/link reservations, handles co-allocation and cross-RMS virtual reservation chains.
- **`vrm/reservation/`** — `ReservationStore` (slotmap-backed), `ReservationBase`, `NodeReservation`, `LinkReservation`, `ProbeReservations`.
- **`vrm/resource/`** — `ResourceStore`, `NodeResource`, `LinkResource`, `NetworkTopology`.
- **`vrm/rms/`** — RMS implementations: `RmsSimulator`, `RmsNodeSimulator`, `SlurmRms`.
- **`vrm/schedule/`** — `SlottedScheduleContext` and strategies (`NodeStrategy`, `LinkStrategy`).
- **`vrm/workflow/`** — `Workflow` graph, `WorkflowNode`, co-allocation groups, data/sync dependencies.
- **`vrm/global_clock/`** — `GlobalClock`: simulation time (starts at 0, advanced by `tick_forward`).
- **`vrm/common/`** — IDs, config constants, logging, legacy workflow adapter.
- **`schema/`** — DTOs for JSON parsing (`VrmDto`, `AcIDto`, `ADCDto`, `RmsSimulatorDto`, `GatewayConfigDto`, workflow/client DTOs).

Documentation: `agent/docs/vrm_rust_architecture.md` (master) + per-component folders under `agent/docs/{reservation,resource,rms,schedule,vrm_component,workflow}/`.

## Conventions

### Code style
- **DRY** — extract shared logic; no copy-paste.
- **Simplicity** — avoid unnecessary traits, generics, or abstraction layers.
- **Idiomatic Rust** — follow standard patterns; match, `Option`/`Result`, iterator adapters.
- **Minimize allocations** — prefer `&str` over `String`, `&[T]` over `Vec<T>` for read-only inputs. Justify every `clone()`.
- **Visibility** — default private; `pub` only when external access is required.
- **Error handling** — `Result<T, E>` with explicit error types; use `?`; avoid `unwrap()`/`expect()`/`panic!()` in production code.
- **No `unsafe`**.
- **Concurrency** — `parking_lot` only for `Mutex`/`RwLock`; keep lock duration minimal; avoid nested locks.

### Documentation
- Document every public function and struct.
- When changing a component, update its docs under `agent/docs/{component}/` (`architecture.md`, `data-flow.md`, `modules.md`).
- Architectural changes must be reflected in `agent/docs/vrm_rust_architecture.md`.
- Process-improvement ideas go in `agent/agent_suggestions.md`.

### Testing
- Place tests under `tests/` mirroring the `src/` structure.
- Test happy paths, error paths, and edge cases.
- Keep tests deterministic and independent.
- Use the `agent/script/coding_check.sh` gate before finalizing any change.

### Logging
- `info` — important system-state events.
- `debug` — lock activity, detailed diagnostics.

## Notes

- `RMS_GATEWAY_NAME` and `get_component_router_list()` are permanently removed per the information-hiding principle (AD‑5). `agent/script/verify_removed.sh` enforces this.
- `USE_FULL_INTER_GATEWAY_PATH_FINDING` defaults to `false` (single-hop virtual resource between gateways).
- Cross-RMS dependencies are split into 4-segment virtual reservation chains; virtual reservations are cascade-deleted with their parent.
- The simulation clock advances via `GlobalClock::tick_forward()` (called in `VrmManager::run_vrm` loop).
