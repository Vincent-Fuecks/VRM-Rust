# US_data_sync_streaming — Bidirectional Streaming for Sync Dependency Data Transfer

## Status
**Proposed**

## Problem Statement

**Sync dependencies** in a workflow (`SyncDependency`) enforce **co-allocation** (gang scheduling) — a set of tasks linked by sync dependencies must run simultaneously on the same or different clusters. When tasks in a co-allocation group are on **different HPC clusters**, they need to exchange data in real time during execution.

The epic defines the following requirements for sync dependency data transfer:

1. **Bidirectional streaming** — both Cluster A → Cluster B and Cluster B → Cluster A must be able to send data simultaneously.
2. **Independent data paths** — each direction (A→B, B→A) is an independent async task; a slow/failed transfer in one direction must not block the other.
3. **Server-mediated** — a central server (the VRM system, or a dedicated streaming proxy on the VRM side) relays data between clusters since clusters may not have direct network connectivity to each other.
4. **Backup** — data passing through the server is backed up (persisted to disk) via a separate async task (using `tokio::sync::mpsc` channels) so that forwarding latency is not impacted by disk I/O.

This is distinct from **data dependencies** (handled in [US_data_letterbox_and_deps](US_data_letterbox_and_deps.md)) — sync dependency data transfer is **real-time** and **bidirectional**, whereas data dependency transfer is **store-and-forward** (producer finishes → data stored → consumer fetches).

## Goal

Implement a **bidirectional streaming proxy** within the VRM system that:

1. **Accepts connections** from two cluster endpoints (Cluster A's Gateway, Cluster B's Gateway) — or, for the first iteration, from two in-process channels.
2. **Relays data** in both directions: data from A is immediately forwarded to B, data from B is immediately forwarded to A.
3. **Backs up data** — each relayed chunk is also sent to a backup task via `tokio::sync::mpsc` that writes it to disk asynchronously.
4. **Integrates with sync dependency resolution** — when the WorkflowScheduler identifies a co-allocation group spanning multiple RMS systems, the sync streaming channels are set up before the tasks are committed.
5. **Handles errors gracefully** — if one direction fails, the other direction continues; errors are logged; partial transfers are recorded.
6. **Only for SlurmRms** — simulator RMS variants have no sync streaming (co-allocation is purely schedule-based for simulators).

## Resolved Architectural Decisions

### AD-1: Bidirectional Streaming Proxy as a Tokio Service

**Decision:** The streaming proxy is implemented as an async tokio service that manages two independent `tokio::sync::mpsc` channels per direction:

```
                    ┌──────────────────────────────┐
                    │     StreamingProxy (VRM)       │
                    │                                │
    Cluster A ─────→│  rx_a → (forward) → tx_b  ───→│──→ Cluster B
                    │            ↘                    │
                    │         (backup)                │
                    │            ↓                    │
                    │      backup_tx → disk           │
                    │                                │
    Cluster B ─────→│  rx_b → (forward) → tx_a  ───→│──→ Cluster A
                    │            ↘                    │
                    │         (backup)                │
                    │            ↓                    │
                    │      backup_tx → disk           │
                    └──────────────────────────────┘
```

**Scope:**
- `src/vrm/rms/slurm_rms/sync_streaming.rs` — new module

### AD-2: In-Process Channels for First Iteration

**Decision:** For the first iteration, the "network" between the streaming proxy and the cluster gateways is represented by in-process `tokio::sync::mpsc` channels, not real TCP/HTTP connections. This allows the streaming logic to be tested without network infrastructure. A `StreamingProxy` trait abstracts the transport so that a future US can swap in real TCP/TLS.

```rust
struct StreamingProxy {
    /// Channel: data from cluster A arrives here
    rx_from_a: mpsc::Receiver<DataChunk>,
    /// Channel: data destined for cluster A is sent here
    tx_to_a: mpsc::Sender<DataChunk>,
    /// Channel: data from cluster B arrives here
    rx_from_b: mpsc::Receiver<DataChunk>,
    /// Channel: data destined for cluster B is sent here
    tx_to_b: mpsc::Sender<DataChunk>,
    /// Channel: backup task receives data chunks
    backup_tx: mpsc::Sender<DataChunk>,
}
```

**Scope:**
- `src/vrm/rms/slurm_rms/sync_streaming.rs`

### AD-3: Backup as an Independent Tokio Task

**Decision:** A `backup_task` is spawned that reads from `backup_tx` (an `mpsc::Receiver<DataChunk>`) and writes each chunk to disk. Chunks are stored under `data/sync_backup/{co_allocation_id}/{direction}/{sequence_number}.bin`. The backup task runs independently; if it falls behind, the `mpsc` channel buffers (with a configurable bound, default 256). If the channel is full, `send` returns an error and the chunk is skipped for backup (but still forwarded).

**Rationale:** This follows the epic's recommendation to use `tokio::sync::mpsc` for decoupling forwarding from backup.

**Scope:**
- `src/vrm/rms/slurm_rms/sync_streaming.rs` — `backup_task` function

### AD-4: Integration with Co-Allocation Scheduling

**Decision:** When the `WorkflowScheduler` (e.g., `HEFTSyncWorkflowScheduler`) schedules a co-allocation group, and the group's tasks are assigned to **different RMS systems**, the scheduler (or the `VrmComponentManager`) creates a `StreamingProxy` for that co-allocation group and registers the sender/receiver endpoints with each `SlurmRms` instance.

The `SlurmRms` instances inject the channel endpoints into their commit flow: when a sync-dependent task is committed, its `TaskSubmission` includes a reference to the channel endpoint so the Gateway job knows where to send/receive streaming data.

**For this US**, we implement the proxy and channel setup. The Gateway-side consumption of channels is deferred to a future US (the Gateway service). This US focuses on the VRM-side proxy logic.

**Scope:**
- `src/vrm/vrm_component/scheduler/` — co-allocation cross-RMS detection
- `src/vrm/rms/slurm_rms/sync_streaming.rs` — proxy lifecycle

### AD-5: Graceful Shutdown and Error Propagation

**Decision:** 
- Each direction's relay task runs in a `tokio::select!` that monitors both the incoming channel and a shutdown `oneshot` receiver.
- If the incoming channel closes (sender dropped), the relay task for that direction terminates gracefully.
- If the outgoing channel closes (receiver dropped), the relay task logs an error and terminates.
- The backup task terminates when `backup_tx` is dropped (all relay tasks have ended).
- A `StreamingProxyHandle` provides `shutdown()` and `await_shutdown()` methods.

**Scope:**
- `src/vrm/rms/slurm_rms/sync_streaming.rs` — shutdown logic

### AD-6: Simulator RMS — No Streaming

**Decision:** Sync dependencies on simulator RMS variants remain purely schedule-based (co-allocation enforces simultaneous start times, no data transfer). The streaming proxy is only created for `SlurmRms` instances.

---

## Implementation Checklist

### Phase 1: Core Streaming Proxy
- [ ] Create `src/vrm/rms/slurm_rms/sync_streaming.rs` module
- [ ] Define `DataChunk` struct: `{ sequence: u64, co_allocation_id: String, direction: Direction, payload: Vec<u8> }`
- [ ] Define `Direction` enum: `AtoB`, `BtoA`
- [ ] Implement `StreamingProxy::new()` — takes 4 mpsc channels + backup channel
- [ ] Implement `StreamingProxy::run()` — spawns 2 relay tasks + 1 backup task, returns `StreamingProxyHandle`
- [ ] Implement relay task: `async fn relay(mut rx, tx, backup_tx, shutdown_rx)`

### Phase 2: Backup Task
- [ ] Implement `backup_task(mut backup_rx, base_path: PathBuf)` — writes chunks to `data/sync_backup/{co_allocation_id}/{direction}/{seq}.bin`
- [ ] Handle `mpsc::error::SendError` when backup channel is full (log warn, skip chunk)
- [ ] Handle disk I/O errors (log error, continue)

### Phase 3: Shutdown & Lifecycle
- [ ] Implement `StreamingProxyHandle` with `shutdown()` (sends on all shutdown senders)
- [ ] Implement `await_shutdown()` (joins all 3 tasks)
- [ ] Ensure dropping `StreamingProxyHandle` without calling `shutdown()` triggers graceful shutdown (all channels dropped → tasks terminate)

### Phase 4: Unit Tests (In-Process)
- [ ] Test: Send data A→B, verify B receives it
- [ ] Test: Send data B→A, verify A receives it
- [ ] Test: Simultaneous bidirectional send, both directions deliver correctly
- [ ] Test: Backup task writes chunks to disk, verify file contents
- [ ] Test: One direction fails (channel closed) — other direction continues
- [ ] Test: Shutdown — all tasks terminate within timeout
- [ ] Test: Backup channel full — chunk skipped for backup but still forwarded

### Phase 5: Integration with Scheduler (Stub)
- [ ] Add a method on `VrmComponentManager` (or scheduler) to detect cross-RMS co-allocations
- [ ] Create `StreamingProxy` when cross-RMS co-allocation is detected
- [ ] Store proxy handle and channel endpoints on the relevant `SlurmRms` instances
- [ ] Test: Cross-RMS co-allocation triggers proxy creation (verify via log output for now)

---

## Test Cases

### TC-5.1: Unidirectional Data Relay A → B

**Objective:** Verify that data sent from A arrives at B through the proxy.

**Given:**
- A `StreamingProxy` with in-process mpsc channels
- Proxy is running (relay tasks spawned)

**When:**
- Send `DataChunk { sequence: 0, payload: b"hello from A" }` via `tx_to_a` (meaning: A is sending to the proxy, which forwards to B)

**Then:**
- A `DataChunk` with `payload: b"hello from A"` is received on `rx_from_b` (the B-side receiver)
- The sequence number is preserved

---

### TC-5.2: Unidirectional Data Relay B → A

**Objective:** Verify the reverse direction.

**Given:**
- Running `StreamingProxy`

**When:**
- Send `DataChunk { sequence: 0, payload: b"hello from B" }` via B's send channel

**Then:**
- The chunk is received on A's receive channel with content `b"hello from B"`

---

### TC-5.3: Simultaneous Bidirectional Relay

**Objective:** Verify that both directions operate independently and simultaneously.

**Given:**
- Running `StreamingProxy`
- Two concurrent tokio tasks: one sending A→B, one sending B→A

**When:**
- Task 1 sends 100 chunks A→B (each 1KB, with sequence numbers 0..99)
- Task 2 sends 50 chunks B→A (each 1KB, with sequence numbers 0..49)
- Both run concurrently

**Then:**
- All 100 chunks arrive at B's receiver in order
- All 50 chunks arrive at A's receiver in order
- Both tasks complete without blocking each other
- Total time is approximately `max(time_for_100, time_for_50)`, not `time_for_100 + time_for_50`

---

### TC-5.4: Backup Task Writes Chunks to Disk

**Objective:** Verify that relayed data is backed up to disk asynchronously.

**Given:**
- Running `StreamingProxy` with a temp directory as backup base path
- 5 chunks sent A→B

**When:**
- All chunks are relayed and the proxy is shut down (after a short delay to let backup complete)

**Then:**
- Directory `{temp}/<co_allocation_id>/AtoB/` exists
- It contains 5 files: `0.bin` through `4.bin`
- File `0.bin` contains `payload` of the first chunk
- The forwarding to B completed before or concurrently with backup writes (not blocked by disk I/O)

---

### TC-5.5: One Direction Fails — Other Continues

**Objective:** Verify fault isolation between directions.

**Given:**
- Running `StreamingProxy`
- B's receiver (`rx_from_b`) is **dropped** (simulating B disconnecting)

**When:**
- Send 10 chunks A→B (these will fail at forwarding because B's receiver is gone)
- Simultaneously send 5 chunks B→A

**Then:**
- The A→B relay task logs an error (forwarding failed) and terminates
- The B→A relay task continues normally — all 5 chunks arrive at A
- The proxy does not panic
- After shutdown, the backup contains the A→B chunks that were received (they were backed up even if forwarding failed)

---

### TC-5.6: Graceful Shutdown

**Objective:** Verify that the proxy can be shut down cleanly.

**Given:**
- Running `StreamingProxy`

**When:**
- `handle.shutdown()` is called
- `handle.await_shutdown()` is awaited with a 5-second timeout

**Then:**
- All relay tasks terminate within the timeout
- The backup task terminates within the timeout
- No channels are left dangling
- `await_shutdown()` returns `Ok(())`

---

### TC-5.7: Backup Channel Full — Skips Backup, Forwards Anyway

**Objective:** Verify that backup backpressure does not block forwarding.

**Given:**
- A `StreamingProxy` with a backup `mpsc` channel of capacity **1**
- The backup task is **not running** (simulating a slow/failed backup)

**When:**
- Send 3 chunks A→B in quick succession

**Then:**
- All 3 chunks arrive at B (forwarding is not blocked)
- Only 1 or 2 chunks are buffered in the backup channel (the rest are dropped with `SendError`)
- A `warn!` log is emitted indicating backup channel overflow
- The proxy does not deadlock

---

### TC-5.8: Simulator RMS Co-Allocation Without Streaming (Regression)

**Objective:** Verify that co-allocated tasks on `RmsSimulator` still work without any streaming proxy.

**Given:**
- A VRM system with one `RmsSimulator` AcI
- A workflow with 3 tasks forming a `CoAllocation` group via sync dependencies

**When:**
- `VrmManager::run_vrm()` processes the workflow

**Then:**
- All 3 tasks reach `ReserveAnswer` or `Committed`
- All 3 tasks share the same `assigned_start` time
- No `StreamingProxy` is created
- No sync streaming log messages appear
- Existing co-allocation tests pass unchanged

---

## Dependencies

- **Depends on:** [US_data_letterbox_and_deps](US_data_letterbox_and_deps.md) — the letterbox stores output data that sync streaming may also need to access. [US_data_gateway_job](US_data_gateway_job.md) — the Gateway handles the cluster-side channel endpoints.
- **Blocks:** Future Gateway service US — the Gateway consumes the channel endpoints set up here.

## Effort Estimate

- **Phase 1 (core proxy):** ~3h — DataChunk, StreamingProxy struct, relay task logic, tokio::select!
- **Phase 2 (backup task):** ~1.5h — file I/O, directory structure, error handling
- **Phase 3 (shutdown):** ~1.5h — oneshot channels, graceful termination, handle API
- **Phase 4 (unit tests):** ~1.5h — 7 in-process test cases
- **Phase 5 (scheduler stub):** ~0.5h — cross-RMS detection, proxy creation hook

**Total:** ~8h
