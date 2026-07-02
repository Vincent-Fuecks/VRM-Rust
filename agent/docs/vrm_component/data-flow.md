# VrmComponent — Data Flow

## Lifecycle of a Reservation Request

### 1. Atomic Task: Probe → Reserve → Commit

```
Client / VrmManager
  │
  ├─(1)─► adc_master.probe(res_id, None)
  │         │
  │         └─► VrmComponentManager.probe_all_components(res_id)
  │               │
  │               ├─ For each child VrmComponent:
  │               │    ├─ container.can_handel(snapshot) ?
  │               │    └─ container.vrm_component.probe(res_id, None)
  │               │         │
  │               │         └─► [VrmComponentProxy.call()]
  │               │               └─► [Actor thread: AcI.probe()]
  │               │                     └─► rms_system.probe(res_id, None)
  │               │                           └─► Schedule.probe()
  │               │
  │               └─► ProbeReservations (aggregated)
  │
  ├─(2)─► probe_reservations.prompt_best(res_id, comparator)
  │         │
  │         └─ Selects best (ComponentId, ShadowScheduleId) pair
  │
  ├─(3)─► adc_master.reserve(res_id, None)
  │         │
  │         └─► ADC.reserve() discriminates:
  │               ├─ Atomic: VrmComponentManager.reserve_task_at_first_grid_component()
  │               │    └─ Iterates ordered components → reserve at first accepting component
  │               │         └─► VrmComponentProxy.reserve()
  │               │               └─► [Actor: AcI.reserve()]
  │               │                     └─► rms_system.reserve(res_id, None)
  │               │                           └─► Schedule.reserve()
  │               │
  │               └─ Workflow: workflow_scheduler.reserve(res_id, &mut adc)
  │                    (See Section 2 below)
  │
  └─(4)─► adc_master.commit(res_id)
            │
            └─► ADC.commit()
                  ├─ Looks up component_id from VrmComponentManager
                  └─► VrmComponentManager.commit_at_component(res_id, component_id)
                        └─► VrmComponentProxy.commit()
                              └─► [Actor: AcI.commit()]
                                    └─► rms_system.commit(res_id)
```

### 2. Workflow Reservation: HEFT Scheduling

```
ADC.reserve(workflow_res_id)
  │
  └─► workflow_scheduler.reserve(workflow_res_id, &mut adc)
        │
        ├─ Phase 1: Upward Rank Calculation
        │   workflow.calculate_upward_rank(average_link_speed)
        │   → Sorted WorkflowNode list (critical path first)
        │
        ├─ Phase 2: For each WorkflowNode (in rank order):
        │   │
        │   ├─ Calculate Earliest Start Time from data dependencies
        │   │   start = max(booking_interval_start, max(data_dep.end + transfer_time))
        │   │
        │   ├─ schedule_co_allocation_node_reservations():
        │   │   │
        │   │   ├─ schedule_node_reservation_eft(main_node):
        │   │   │   └─► VrmComponentManager.reserve_reservation_at_best_vrm_component()
        │   │   │         ├─ Probes all components → collects ProbeReservations
        │   │   │         └─ TRY_N_PROMOTIONS attempts:
        │   │   │               prompt_best(EFTReservationCompare) → reserve at component
        │   │   │
        │   │   ├─ For each co-allocation member:
        │   │   │   └─► adc.submit_task_at_first_grid_component()
        │   │   │         └─ Iterates ordered components → reserve at first accepting
        │   │   │
        │   │   └─ schedule_sync_dependencies():
        │   │         └─ For each sync dependency:
        │   │               schedule_dependency() [see below]
        │   │
        │   └─ schedule_data_dependencies():
        │         └─ For each incoming data dependency:
        │               schedule_dependency()
        │                 │
        │                 ├─ Dummy (same component, zero capacity):
        │                 │   └─ State → Committed, localhost endpoints
        │                 │
        │                 └─ Real (different components):
        │                     ├─ Set router endpoints (source → target)
        │                     ├─ If file transfer: adjust_task_duration to 1 (moldable)
        │                     └─► adc.submit_task_at_first_grid_component()
        │
        ├─ On success:
        │   └─► VrmComponentManager.register_workflow_subtasks()
        │         ├─ Merges allocation map (res_to_vrm_component)
        │         ├─ Tracks workflow_subtasks (parent → children)
        │         └─ Tracks reverse_workflow_subtasks (child → parent)
        │
        └─ On failure:
              └─ cancel_all_reservations()
                    └─► VrmComponentManager.delete_task_at_component() for each allocation
```

### 3. Shadow Schedule Lifecycle

```
ADC.optimize_schedule()
  │
  ├─(1)─► VrmComponentManager.get_system_satisfaction(None)
  │        If satisfaction > 0.5 (fragmented):
  │
  ├─(2)─► VrmComponentManager.create_shadow_schedule(shadow_id)
  │         │
  │         ├─ Snapshots ReservationStore → shadow_store
  │         ├─ Clones res_to_vrm_component → shadow_map
  │         └─ Propagates to all children:
  │               └─► container.vrm_component.create_shadow_schedule(shadow_id)
  │                     └─► AcI: rms_system.create_shadow_schedule(shadow_id)
  │
  ├─(3)─► Reschedule operations in shadow:
  │         ├─ Delete all tasks from shadow schedule
  │         ├─ Sort by duration (longest first)
  │         └─ Re-reserve each at best component (EST comparator)
  │
  ├─(4)─► Compare satisfaction:
  │         new_satisfaction = VrmComponentManager.get_system_satisfaction(Some(shadow_id))
  │         │
  │         ├─ If better:  VrmComponentManager.commit_shadow_schedule(shadow_id)
  │         │                ├─ Propagates commit to all children
  │         │                ├─ Replaces res_to_vrm_component with shadow_map
  │         │                ├─ Replaces reservation_store with shadow_store
  │         │                └─ Rebuilds committed/not_committed from new state
  │         │
  │         └─ If worse:   VrmComponentManager.delete_shadow_schedule(shadow_id)
  │                          ├─ Propagates deletion to all children
  │                          └─ Removes local shadow context
```

### 4. Actor Communication Flow

```
Caller Thread                          Actor Thread (e.g., "Actor-MyAcI")
─────────────                          ──────────────────────────────────

VrmComponentProxy.reserve(id, sid)
  │
  ├─ let (reply_tx, reply_rx) = mpsc::channel()
  ├─ self.tx.send(VrmMessage::Reserve { id, sid, reply_to: reply_tx })
  │                                          │
  │                                          ▼
  │                              rx.recv() → VrmMessage::Reserve
  │                                            │
  │                                            ▼
  │                              component.reserve(id, sid)
  │                                            │
  │                              reply_tx.send(result)
  │                                            │
  ▼                                            │
  reply_rx.recv() ◄────────────────────────────┘
  │
  └─ Returns ReservationId
```

**Critical note**: The caller blocks on `reply_rx.recv()`. If Actor A calls Actor B while Actor B calls Actor A, deadlock occurs.

### 5. VrmManager Orchestration Flow

```
VrmManager.run_vrm()
  │
  ├─ For each unprocessed reservation (sorted by arrival time):
  │   │
  │   └─► process_reservation(res_id)
  │         │
  │         ├─ Step 1: Quick probe+reserve (non-workflow only)
  │         │   adc_master.probe() → prompt_best(EST) → implicit reserve
  │         │
  │         ├─ Step 2: Reserve (if Reserve proceeding)
  │         │   adc_master.reserve()
  │         │
  │         ├─ Step 3: Commit (if Commit proceeding)
  │         │   adc_master.commit()
  │         │   → Adds to open_reservations
  │         │
  │         └─ Step 4: Delete (if Delete proceeding)
  │               adc_master.delete()
  │
  └─ Polling loop: while open_reservations not empty:
        ├─ Check cycle-end → move to terminal state
        ├─ Check commit-ready → try_commit_reservation()
        └─ Sleep 5s → repeat
```

### 6. Key Data Transformations

| Stage | Input | Output | Key Transformation |
|:------|:------|:-------|:-------------------|
| **Probe** | `ReservationId` | `ProbeReservations` | Reservation requirements → feasible time slots across all components |
| **ProbeBest** | `ProbeReservations` + comparator | selected `(ComponentId, ShadowScheduleId)` | Aggregate → single best candidate via comparison function |
| **Reserve** | `ReservationId` + `ComponentId` | `ReservationId` (state updated) | Temporary hold on schedule slot; state Open/ProbeAnswer → ReserveAnswer |
| **Commit** | `ReservationId` | `bool` | ReserveAnswer → Committed; propagates to physical RMS |
| **Delete** | `ReservationId` | `ReservationId` | Any active state → Deleted; releases schedule resources |
| **HEFT Schedule** | `WorkflowReservation` | `HashMap<ReservationId, ComponentId>` | Workflow graph → ranked task list → per-task component assignment |
| **Shadow Commit** | `ShadowScheduleId` | `bool` | Shadow state atomically replaces master state |
| **Metrics** | `(start, end)` window | `RmsLoadMetric` / `f64` | Per-component load/satisfaction → capacity-weighted system average |
