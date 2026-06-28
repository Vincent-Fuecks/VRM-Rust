# RMS Component — Data Flow

## 1. RMS Initialization Flow

### 1.1 Factory Dispatch

```
RmsSystemWrapper::get_instance(dto, simulator, component_id, reservation_store)
│
├── dto = Slurm(slurm_dto)
│   └──► SlurmRms::new(slurm_dto, ...)
│        │  1. Create SlurmRestApiClient (JWT auth, HTTP client)
│        │  2. GET /nodes  (fetch live node inventory)
│        │  3. Build topology: get_nodes_and_links(dto, nodes_response)
│        │     ├── nodes: Vec<Node>  (from live Slurm node data + topology config)
│        │     └── links: Vec<Link>  (bidirectional links from topology)
│        │  4. Populate ResourceStore with node resources
│        │  5. Create node schedule via SchedulerType
│        │  6. Create NetworkTopology → network schedule via SchedulerType::get_network_scheduler_variant()
│        │  7. Create RmsBase with identity + stores
│        │  8. Spawn background sync task (start_sync)
│        │  9. Return SlurmRms instance
│        └──► Box<dyn AdvanceReservationRms>
│
├── dto = RmsSimulator(sim_dto)
│   └──► RmsSimulator::new(sim_dto, ...)
│        │  1. Parse topology + compute nodes via get_nodes_and_links()
│        │  2. Create RmsSetupContext
│        │  3. get_node_schedule() ──► Arc<RwLock<Box<dyn Schedule>>>
│        │  4. get_network_schedule() ──► Arc<RwLock<Box<dyn Schedule>>>
│        │  5. get_base() ──► RmsBase
│        │  6. Return RmsSimulator instance
│        └──► Box<dyn AdvanceReservationRms>
│
└── dto = DummyRms(dummy_dto)
    ├── typ = "RmsNodeSimulator"
    │   └──► RmsNodeSimulator::try_from((dto, simulator, component_id, store))
    │        │  1. Parse GridNodeDto → Vec<Node>
    │        │  2. Populate ResourceStore
    │        │  3. Create node schedule via SchedulerType
    │        │  4. Create RmsBase
    │        └──► Box<dyn AdvanceReservationRms>
    │
    └── typ = "RmsNetworkSimulator"
        └──► RmsNetworkSimulator::try_from((dto, simulator, component_id, store))
             │  1. Parse DummyRmsDto → (nodes, links) via RmsBase::get_nodes_and_links()
             │  2. Create NetworkTopology → network schedule
             │  3. Create RmsBase
             └──► Box<dyn AdvanceReservationRms>
```

## 2. Reservation Life Cycle (Probe → Reserve → Commit / Delete)

This is the primary runtime data flow. The consumer (typically an `AcI` or `VrmComponentManager`) drives this through the `AdvanceReservationRms` trait.

### 2.1 Probe Flow

```
Consumer
  │
  └──► advance_rms.probe(reservation_id, optional shadow_id)
       │
       ├──► self.get_active_schedule(shadow_id, reservation_id)
       │    │  1. Check reservation type (link vs. node) via ReservationStore
       │    │  2. Select schedule (master or shadow, node or link)
       │    └──► Arc<RwLock<Box<dyn Schedule>>>
       │
       └──► schedule.write().probe(reservation_id)
            │  Reads reservation constraints (duration, booking interval, capacity)
            │  Scans slot windows for feasible placements
            │  Returns ProbeReservations with candidates (state = ProbeAnswer)
            │  Does NOT modify the schedule
            └──► ProbeReservations
```

### 2.2 Reserve Flow

```
Consumer
  │
  └──► advance_rms.reserve(reservation_id, optional shadow_id)
       │
       ├──► self.get_active_schedule(shadow_id, reservation_id)
       │
       └──► schedule.write().reserve(reservation_id)
            │  Finds best-fitting slot for the reservation
            │  Marks slot as occupied (state → ReserveAnswer)
            │  On failure: state → Rejected
            └──► Option<ReservationId>  (Some if success, None if rejected)
```

### 2.3 Commit Flow (Simulators — Default Rms::commit)

```
Consumer
  │
  └──► rms.commit(reservation_id)
       │  (Default impl on Rms trait)
       │
       └──► self.get_base().reservation_store.update_state(reservation_id, Committed)
            └──► ReservationStore notifies listeners (e.g., VrmManager)
```

### 2.4 Commit Flow (SlurmRms — Overridden)

```
Consumer
  │
  └──► slurm_rms.commit(reservation_id)      [async fire-and-forget via rt_handle.spawn]
       │
       ├── 1. Extract NodeReservation from ReservationStore
       │     If link reservation → Rejected (Slurm only handles nodes)
       │     If not found → Rejected
       │
       ├── 2. Build TaskSubmission payload:
       │     JobProperties {
       │       name:        "rms_id, VRM-Res-ID, job-name"
       │       cpus_per_task: reserved_capacity
       │       memory_per_node: MEMORY_PER_NODE (config constant)
       │       begin_time: assigned_start (Unix timestamp)
       │       time_limit:  assigned_end (Unix timestamp)
       │       current_working_directory, standard_output, standard_error, environment
       │     }
       │     script: task_path
       │
       ├── 3. POST /job/submit with timeout (SLURM_RMS_COMMIT_TIMEOUT_S)
       │     │
       │     ├── Success → task_mapping.insert(reservation_id, slurm_job_id)
       │     │              reservation_store.update_state(reservation_id, Committed)
       │     │
       │     ├── REST error → reservation_store.update_state(reservation_id, Rejected)
       │     │
       │     └── Timeout → reservation_store.update_state(reservation_id, Rejected)
       │
       └── 4. Returns immediately (spawned async task runs to completion)
```

### 2.5 Delete Flow (Simulators — Default Rms::delete_task)

```
Consumer
  │
  └──► rms.delete_task(reservation_id, optional shadow_id)
       │  (Default impl on Rms trait)
       │
       └──► active_schedule.write().delete_reservation(reservation_id)
            │  Removes reservation from schedule slots
            │  ReservationStore state → Deleted
            └──► ()
```

### 2.6 Delete Flow (SlurmRms — Overridden)

```
Consumer
  │
  └──► slurm_rms.delete_task(reservation_id, optional shadow_id)
       │
       ├── State != Committed? → schedule.delete_reservation() only (local)
       │
       └── State == Committed? → async fire-and-forget:
            │
            ├── 1. Lookup slurm_job_id in task_mapping
            │
            ├── 2. DELETE /job/{slurm_job_id} with timeout (SLURM_RMS_DELETE_TIMEOUT_S)
            │     │
            │     ├── Success → task_mapping.remove_by_right(slurm_job_id)
            │     │              schedule.delete_reservation(reservation_id)
            │     │              If schedule deletion fails → state → Rejected (cleanup error)
            │     │
            │     ├── REST error → state → Rejected
            │     │
            │     └── Timeout → state → Rejected
            │
            └── 3. Returns immediately
```

## 3. Shadow Schedule Life Cycle

```
Consumer (VrmManager / Co-allocation logic)
  │
  ├── create_shadow_schedule(shadow_id)
  │   └──► Clone node schedule → insert in node_shadow_schedule map
  │        Clone network schedule → insert in network_shadow_schedule map
  │        (For single-schedule simulators: clone only the relevant schedule)
  │
  ├── [Operations on shadow schedule]
  │   │  probe(reservation_id, Some(shadow_id))
  │   │  reserve(reservation_id, Some(shadow_id))
  │   │  delete_task(reservation_id, Some(shadow_id))
  │   │  (All operations route to shadow schedule via get_active_schedule)
  │   └──► Shadow schedule is modified independently of master
  │
  ├── commit_shadow_schedule(shadow_id)
  │   └──► Remove shadow from maps → set as new master
  │        (Master schedule is replaced atomically)
  │
  └── delete_shadow_schedule(shadow_id)
      └──► Remove shadow from maps (discarded)
```

## 4. SlurmRms Background Synchronization

```
[Tokio background task — spawned at SlurmRms::new()]
  │
  └──► loop (every SCHEDULE_SYNC_TIMEINTERVAL_S seconds)
       │
       ├── 1. GET /nodes
       │     └──► Parse SlurmNodesResponse
       │          Extract: node name, cpus
       │          Build Vec<NodeResource>
       │
       ├── 2. Update ResourceStore
       │     resource_store.update_nodes(node_resources)
       │     (New nodes added, down nodes removed)
       │
       ├── 3. If total node capacity changed:
       │     node_schedule.write().update_capacity(new_capacity)
       │
       ├── 4. GET /jobs
       │     └──► Parse SlurmTaskResponse → Vec<SlurmTask>
       │
       └── 5. update_reservations()
            │
            ├── For tasks DELETED from Slurm:
            │   └──► task_mapping.remove_by_right(slurm_id)
            │        reservation_store.update_state(res_id, Finished)
            │
            ├── For tasks with state CHANGED on Slurm:
            │   │  Map Slurm state → ReservationState
            │   └──► reservation_store.update_state(res_id, new_state)
            │
            └── For tasks NEW on Slurm (not in task_mapping):
                └──► Build NodeReservation from SlurmTask
                     reservation_store.add(external_reservation)  [state = External]
                     task_mapping.insert(res_id, slurm_job_id)
                     node_schedule.write().reserve(res_id)
```

## 5. Load Metric & Fragmentation Query Flow

```
Consumer
  │
  └──► rms.get_load_metric(start, end, optional shadow_id)       [read-only]
  │    │
  │    └──► if shadow_id: read shadow schedule → get_load_metric()
  │         else:           read master schedule → get_load_metric()
  │         Returns RmsLoadMetric { node_load_metric, link_load_metric }
  │
  ├──► rms.get_load_metric_up_to_date(start, end, optional shadow_id)  [write lock]
  │    └──► Same as above but acquires write lock (refreshes slot utilization first)
  │
  ├──► rms.get_fragmentation(start, end, optional shadow_id)
  │    └──► sum of node schedule fragmentation + network schedule fragmentation
  │
  └──► rms.get_system_fragmentation(optional shadow_id)
       └──► sum of node + network system fragmentation
```

## 6. Capability Check Flow

```
Consumer (VrmComponentManager)
  │
  ├──► rms.can_handle_adc_request(reservation)
  │    │  Checks: is reservation link or node type?
  │    │  Delegates to resource_store.can_handle_adc_request(res)
  │    └──► bool
  │
  └──► rms.can_handle_aci_request(reservation_store, reservation_id)
       │  Checks: is reservation link or node type?
       │  Delegates to resource_store.can_handle_aci_request(store, id)
       └──► bool
```
