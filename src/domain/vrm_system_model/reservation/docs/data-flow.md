# Reservation Component — Data Flow

## Overview

This document describes the lifecycle of reservations and how data flows between the modules within the Reservation component.

## 1. Reservation Lifecycle (State Machine)

The following describes the typical state transitions for a reservation from submission to completion:

```
  [Client submits]
       │
       ▼
     Open
       │
       ├───► Probe ──────────────────────────────────► ProbeAnswer
       │         │ (via ProbeReservations)                   │
       │         ▼                                           │
       │    ProbeReservation                                 │
       │         │                                           │
       │         └─── (promote best) ────────────────────────┘
       │                                                      │
       │                                                      ▼
       │                                               ReserveProbeReservation
       │                                                      │
       │                                                      ▼
       │                                               ReserveAnswer
       │                                                      │
       └───► Reserve ────────────────────────────────────────┘
                                                               │
                                                               ▼
                                                          Committed
                                                               │
                                                               ▼
                                                     ┌─────────┼─────────┐
                                                     ▼         ▼         ▼
                                                  Finished  Deleted  Rejected
```

**Key flows:**
- **Probe flow**: `Open` → (probe scheduling) → `ProbeAnswer` → `ProbeReservation` → `ReserveProbeReservation` → `ReserveAnswer`
- **Direct flow**: `Open` → `ReserveProbeReservation` → `ReserveAnswer`
- **Commit flow**: `ReserveAnswer` → `Committed` → `Finished`
- **Terminal states**: `Deleted`, `Rejected` (from any non-terminal), `Finished` (from `Committed`)

## 2. Reservation Addition Flow

```
Client / ADC / AcI
    │
    ├── ReservationStore::add(reservation)
    │       │
    │       ├── write lock StoreInner
    │       ├── insert into SlotMap → get ReservationId
    │       ├── insert into name_index
    │       ├── insert into client_index
    │       ├── insert into handler_index (if handler exists)
    │       └── release lock
    │
    └── returns ReservationId
```

## 3. State Update Flow (Observer Pattern)

```
ReservationStore::update_state(id, new_state)
    │
    ├── 1. Call get_state(id) → read lock → read state → release lock
    │
    ├── 2. Acquire write lock on StoreInner
    │
    ├── 3. Mutate reservation:
    │       └─ res_lock.write()
    │           └─ res.set_state(new_state)
    │
    ├── 4. Release store lock
    │
    ├── 5. Clone listeners list
    │       └─ store_inner.listeners.clone()
    │
    └── 6. For each listener:
            └─ listener.write().on_reservation_change(id, name, old, new)
```

## 4. Probe Reservation Flow

```
Scheduler receives Probe Request
    │
    ├── ReservationStore::get_reservation_snapshot(original_id)
    │       → Clone of original Reservation
    │
    ├── ProbeReservations::new(original_id, store)
    │       → Stores original reservation snapshot
    │
    ├── For each scheduling candidate slot:
    │       └─ ProbeReservations::add_reservation(candidate)
    │           → Generates unique ProbeReservationId
    │
    ├── Optionally: ProbeReservations::add_probe_meta_data(component_id, shadow_schedule_id)
    │
    ├── ProbeReservations::prompt_best(original_id, comparator)
    │       │
    │       ├── Find best candidate (EFT or EST comparison)
    │       │   └─ ProbeReservations::get_best_probe_reservation_id()
    │       │
    │       ├── Remove from local store
    │       ├── Update original in ReservationStore:
    │       │   ├── set_booking_interval_start()
    │       │   ├── set_booking_interval_end()
    │       │   ├── set_assigned_start()
    │       │   ├── set_assigned_end()
    │       │   └── update_state(ReserveProbeReservation)
    │       │
    │       └── Return (ComponentId, ShadowScheduleId) for routing
    │
    └── ProbeReservations::demote()
        → Revert original reservation to pre-probe state
```

## 5. Reservation Snapshot Flow (Scheduler Isolation)

```
Scheduler starts scheduling cycle
    │
    ├── ReservationStore::snapshot()
    │       │
    │       ├── Read lock StoreInner
    │       ├── Clone all SlotMap entries
    │       ├── Deep clone each reservation (Arc<RwLock<>> replacement)
    │       ├── Clone all indices
    │       └── Return new ReservationStore (no active listeners)
    │
    ├── Scheduler works on snapshot: add/remove/modify probe reservations
    │       └── Never affects original store
    │
    └── Scheduler commits results → calls update_state() on original store
```

## 6. Virtual Reservation Flow (Link Path Exploration)

```
ReservationStore::add_virtual_reservation_diff_start(original_id, start_router)
    │
    ├── Clone original LinkReservation
    ├── Modify start_point
    ├── Rename (prepend "Original-Res: ... | Start: ...")
    ├── ReservationStore::add(clone) → returns virtual ReservationId
    ├── Track: original_to_virtual[original_id].push(virtual_id)
    │
    └── Same for add_virtual_reservation_diff_end()

ReservationStore::remove_virtual_reservation(original_id, virtual_id)
    ├── Remove from name_index
    ├── Remove from SlotMap
    └── Clean up original_to_virtual tracking
```

## 7. Cross-Thread Synchronization Flow (SyncGate)

```
ADC thread (waits)                AcI thread (signals)
    │                                  │
    │   SyncRegistry::create_gate()    │
    │       → ReservationSyncGate      │
    │       → State: ReserveProbeReservation
    │                                  │
    │   ReservationSyncGate::          │
    │   wait_with_timeout(timeout)     │
    │       │                          │
    │       │ (blocks on Condvar)      │
    │       │                          │
    │       │                    ReservationSyncGate::notify(new_state, aci_id)
    │       │                          │   └─ lock Mutex
    │       │                          │   └─ update state + aci_id
    │       │                          │   └─ cvar.notify_all()
    │       │                          │
    │       │ (wakes up)               │
    │       ├── Check state change     │
    │       ├── If timeout → Rejected  │
    │       └── Return ReservationResult
```

## 8. Tracking Set Flow (Reservations)

```
VrmStateListener / Scheduler
    │
    ├── Reservations::new_empty(store)
    │
    ├── For each reservation to track:
    │       └─ Reservations::insert(id)
    │           └─ panics if duplicate
    │
    ├── Scheduling query:
    │       ├─ Reservations::get_random_id()
    │       ├─ Reservations::get_id_with_first_start_slot()
    │       └─ Reservations::len() / is_empty()
    │
    └── On deletion:
            └─ Reservations::delete_reservation(id)
                └─ store.update_state(id, Deleted)
```

## 9. Slurm Task Import Flow

```
Slurm RMS detects external job
    │
    └─ NodeReservation::from_slurm(task, aci_id)
        │
        ├── Extract: job_id → ReservationName
        ├── Extract: user_name → ClientId
        ├── Extract: allocated_cpus → reserved_capacity
        ├── Extract: time fields → arrival_time, booking_interval, assigned_start/end
        ├── Set: state → External
        ├── Set: request_proceeding → Ignore
        │
        └── Returns NodeReservation (managed separately by local RMS)
```

## Summary Data Flow Diagram

```
                    ┌───────────────────────┐
                    │   External Inputs      │
                    │ (Client, ADC, Slurm)   │
                    └──────────┬────────────┘
                               │
                               ▼
                    ┌───────────────────────┐
                    │   ReservationStore     │
                    │  (Single Source of     │
                    │   Truth)               │
                    └──┬────────────────┬───┘
                       │                │
                       ▼                ▼
              ┌─────────────┐   ┌───────────────┐
              │ Schedulers   │   │  Observers     │
              │ (snapshot)   │   │ (VrmStateListener)
              └─────────────┘   └───────────────┘
                       │
                       ▼
              ┌───────────────────────┐
              │   ProbeReservations   │
              │ (scheduling explore)  │
              └───────────────────────┘
                       │
                       ▼
              ┌───────────────────────┐
              │ ReservationSyncGate   │
              │ (cross-thread sync)   │
              └───────────────────────┘
```
