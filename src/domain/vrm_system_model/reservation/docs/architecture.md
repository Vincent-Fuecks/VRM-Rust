# Reservation Component — Architecture

## High-Level Pattern

The **Reservation** sub-system follows a **layered, repository-centric, event-driven architecture**. It acts as a **single-writer, multiple-reader** data layer within the larger VRM-Rust distributed scheduling system. The core pattern combines:

- **Repository Pattern**: `ReservationStore` is the single source of truth for all reservation data.
- **Observer/Listener Pattern**: State changes propagate via `ReservationNotificationListener` to components like `VrmStateListener`.
- **Trait-based Polymorphism**: The `ReservationTrait` trait provides a uniform interface over three concrete types (`Workflow`, `NodeReservation`, `LinkReservation`).
- **Synchronization Primitive**: `ReservationSyncGate` implements a **condvar-based rendezvous** for cross-thread wait/notify.

## Core Components and Responsibilities

### 1. `Reservation` (Enum — `reservation.rs`)
- The top-level discriminated union over `Workflow`, `Node`, and `Link` reservation types.
- Provides delegate accessors (`as_workflow()`, `as_node()`, `as_link()`), state queries, and factory methods.
- Implements `ReservationTrait` to unify behavior.

### 2. `ReservationBase` (Struct — `reservation.rs`)
- Shared fields for all reservation variants (name, client, handler, state, proceeding, time windows, capacity, moldable flags).
- Contains `adjust_capacity()` and `adjust_task_duration()` for moldable job negotiation.

### 3. `ReservationStore` (Struct — `reservation_store.rs`)
- **Central thread-safe repository** using `Arc<RwLock<StoreInner>>`.
- Primary storage via `SlotMap<ReservationId, Arc<RwLock<Reservation>>>`.
- Secondary indexes: `name_index`, `client_index`, `handler_index`, `original_to_virtual`.
- Supports virtual (shadow) reservations for link path exploration.
- Provides snapshot isolation (`snapshot()`) for schedulers.
- Implements `ReservationNotificationListener` subscription.

### 4. `NodeReservation` (Struct — `node_reservation.rs`)
- Extends `ReservationBase` with compute-specific fields: `data_dependencies`, `current_working_directory`, `environment`, `task_path`, `output_path`, `error_path`.
- Includes `from_slurm()` factory for importing external Slurm tasks.

### 5. `LinkReservation` (Struct — `link_reservation.rs`)
- Extends `ReservationBase` with `start_point` and `end_point` (RouterId).
- Supports bandwidth reservation for data transfer and co-allocated communication.

### 6. `Reservations` (Struct — `reservations.rs`)
- A lightweight tracked subset of `ReservationId`s referencing the global `ReservationStore`.
- Provides helper methods like `get_random_id()`, `get_id_with_first_start_slot()`.

### 7. `ProbeReservations` (Struct — `probe_reservations.rs`)
- Manages hypothetical (probe) reservations generated during scheduling exploration.
- Supports promote/demote operations to transition probe results into the actual store.
- Uses `ProbeReservationComparator` (EFT, EST) for selection.

### 8. `ReservationSyncGate` + `SyncRegistry` (Structs — `reservation_sync_gate.rs`)
- Synchronization primitive using `Mutex + Condvar` to wait for state transitions.
- Used for cross-thread coordination between ADC and AcI.

### 9. `VrmStateListener` (Struct — `vrm_state_listener.rs`)
- Implements `ReservationNotificationListener`.
- Maintains an `Arc<RwLock<HashSet<ReservationId>>>` of open reservations.
- Automatically removes terminal-state reservations (Deleted, Rejected, Finished).

### 10. `ReservationNotificationListener` (Trait — `reservation_notification_listener.rs`)
- Observer contract with `on_reservation_change()` lifecycle callback.

### 11. `ReservationState` + `ReservationProceeding` (Enums — `reservation.rs`)
- `ReservationState`: Tracks lifecycle from `Open` → `ProbeAnswer` → `ReserveAnswer` → `Committed` → `Finished` / `Deleted` / `Rejected`.
- `ReservationProceeding`: Defines the desired action (`Probe`, `Reserve`, `Commit`, `Delete`, `Ignore`).

## Interfaces Between Layers

```
┌─────────────────────────────────────────────────────────┐
│                    External Consumers                    │
│  ADC, AcI, Schedulers, WorkflowScheduler, Clients       │
├─────────────────────────────────────────────────────────┤
│                    ReservationTrait                      │
│  (get_base, set_state, adjust_capacity, ...)             │
├─────────────────────────────────────────────────────────┤
│                  ReservationStore                        │
│  (add, get, update_state, snapshot,                     │
│   get_by_name, get_client_reservations, ...)             │
├─────────────────────────────────────────────────────────┤
│           ReservationNotificationListener                │
│           VrmStateListener (Observer)                    │
├─────────────────────────────────────────────────────────┤
│          StoreInner (RwLock-protected)                   │
│  SlotMap + name_index + client_index + handler_index     │
└─────────────────────────────────────────────────────────┘
```

## Error Handling

- **Panic-based** error handling is pervasive in the reservation module:
  - `Reservations::insert()` panics on duplicate submission.
  - Many `ReservationStore` accessor methods (`get_client_id`, `get_handler_id`, `get_state`, etc.) panic when the reservation ID is not found, rather than returning `Result`.
  - `Reservation::set_name()` panics if called on a non-ProbeAnswer state.
  - `ProbeReservations::new()` panics if the original reservation is not found.
- `update_state()` uses a boolean guard to conditionally notify listeners, but errors are logged rather than returned.
- The codebase uses `log::error!()` extensively for diagnostic output but rarely returns errors to callers.

## State Management

- **Centralized**: All state is stored in `ReservationStore.inner` under a single `RwLock`.
- **Mutable state** flows through: `get()` → `handle.write()` → modify → drop lock.
- **Immutability via Snapshot**: Schedulers work on a `snapshot()` (deep clone) to avoid locking contention.
- **Observer chain**: `update_state()` → lock → mutate → unlock → iterate listeners → notify each.

## Deadlock Potential and Thread Management

### Locking Analysis

1. **`ReservationStore.inner`** is an `Arc<RwLock<StoreInner>>`. The `RwLock` (from `parking_lot`) allows concurrent reads, exclusive writes.
2. **Per-Reservation Locks**: Each reservation is wrapped in `Arc<RwLock<Reservation>>`. A write on a reservation's inner lock while holding the store lock creates potential for **lock contention**, though not deadlock if locks are acquired consistently.
3. **`ReservationSyncGate`** uses `std::sync::Mutex + Condvar`, which is a **different lock implementation** from the `parking_lot` locks used elsewhere. This inconsistency introduces risk of priority inversion and complicates deadlock analysis.

### Identified Risks

- **Nested Lock Acquisition**: `get_state()`, `get_client_id()`, etc., acquire the store read lock, then acquire the reservation read lock. This is a lock hierarchy (store → reservation) which is safe if always followed. However, mutation methods like `update_state()` follow store → reservation write, which is consistent.
- **Listener Notification in `update_state()`**: Listeners are called **outside** the store lock, but they acquire `self.open_reservations.write()` (in `VrmStateListener`). If a listener attempts to re-enter the store (e.g., call `get_state()`), this would create a **deadlock** because the store lock is held at the time of notification.
- **`dump_store_contents()` / `print_store_contents()`**: These use `try_read_for()` with a 50ms timeout to detect lock contention, which is a reasonable diagnostic practice.
- **Mixing `std::sync::Mutex` and `parking_lot::RwLock`**: `ReservationSyncGate` uses `std::sync::Mutex` while the rest of the module uses `parking_lot::RwLock`. This inconsistency violates the project's deadlock prevention guidelines.

### Thread Safety

- All public types implement `Send + Sync` (via `Arc`, `RwLock`).
- The `ReservationTrait` trait requires `Send + Sync`.
- `ReservationNotificationListener` requires `Send + Sync` on the trait object.
