# Reservation Component — Module Structure

## Overview

The reservation module (`src/domain/vrm_system_model/reservation/`) contains 9 source files representing 10 logical modules. Below is the complete module tree and interaction map.

## Module Tree

```
src/domain/vrm_system_model/reservation/
├── mod.rs                          # Re-exports all sub-modules
├── reservation.rs                  # Core types: Reservation (enum), ReservationBase,
│                                   #   ReservationState, ReservationProceeding,
│                                   #   ReservationTrait (trait), ReservationTyp
├── link_reservation.rs             # LinkReservation struct
├── node_reservation.rs             # NodeReservation struct
├── reservation_store.rs            # ReservationStore, ReservationId (slotmap key)
├── reservations.rs                 # Reservations (lightweight set wrapper)
├── probe_reservations.rs           # ProbeReservations, ProbeReservationComparator
├── reservation_notification_listener.rs  # ReservationNotificationListener trait
├── vrm_state_listener.rs           # VrmStateListener (observer impl)
└── reservation_sync_gate.rs        # ReservationSyncGate, SyncRegistry
```

## Module Descriptions and Purposes

| # | Module | File | Purpose | Key Types |
|---|--------|------|---------|-----------|
| 1 | `reservation` | `reservation.rs` | Core domain model for all reservation types | `Reservation` (enum), `ReservationBase`, `ReservationState`, `ReservationProceeding`, `ReservationTrait`, `ReservationTyp` |
| 2 | `link_reservation` | `link_reservation.rs` | Network link-specific reservation data | `LinkReservation` |
| 3 | `node_reservation` | `node_reservation.rs` | Compute node-specific reservation data, including Slurm import | `NodeReservation` |
| 4 | `reservation_store` | `reservation_store.rs` | Thread-safe central repository with slotmap storage and multiple indices | `ReservationStore`, `ReservationId` (slotmap key) |
| 5 | `reservations` | `reservations.rs` | Lightweight tracked subset of reservation IDs for scheduling | `Reservations` |
| 6 | `probe_reservations` | `probe_reservations.rs` | Hypothetical reservations for scheduling exploration; promote/demote lifecycle | `ProbeReservations`, `ProbeReservationComparator` |
| 7 | `reservation_notification_listener` | `reservation_notification_listener.rs` | Observer trait for lifecycle state changes | `ReservationNotificationListener` |
| 8 | `vrm_state_listener` | `vrm_state_listener.rs` | Concrete observer maintaining active reservation set | `VrmStateListener` |
| 9 | `reservation_sync_gate` | `reservation_sync_gate.rs` | Condvar-based cross-thread synchronization primitive | `ReservationSyncGate`, `SyncRegistry` |

## Dependency Graph (Module Interactions)

```
reservation.rs
├── depends on: link_reservation.rs → LinkReservation
├── depends on: node_reservation.rs → NodeReservation
├── depends on: reservation_store.rs → ReservationId
├── depends on: utils::id → ClientId, ComponentId, ReservationName, RouterId
└── depends on: workflow::workflow → Workflow

reservation_store.rs
├── depends on: reservation.rs → Reservation, ReservationTrait, ReservationState,
│                ReservationProceeding, ReservationTyp
├── depends on: link_reservation.rs → LinkReservation
├── depends on: reservation_notification_listener.rs → ReservationNotificationListener
├── depends on: utils::id → ClientId, ComponentId, ReservationName, RouterId
├── depends on: workflow::workflow → Workflow
├── depends on: workflow::workflow_node → WorkflowNode
├── uses: slotmap crate
└── uses: parking_lot::RwLock

reservations.rs
├── depends on: reservation.rs → ReservationState
├── depends on: reservation_store.rs → ReservationId, ReservationStore
└── uses: rand crate

probe_reservations.rs
├── depends on: reservation.rs → Reservation, ReservationTrait
├── depends on: reservation_store.rs → ReservationId, ReservationStore
└── depends on: utils::id → ComponentId, ProbeReservationId, ShadowScheduleId

link_reservation.rs
├── depends on: reservation.rs → ReservationBase, ReservationTrait, ReservationTyp
└── depends on: utils::id → RouterId

node_reservation.rs
├── depends on: reservation.rs → ReservationBase, ReservationTrait, ReservationTyp,
│                ReservationProceeding, ReservationState
├── depends on: reservation_store.rs → ReservationId
├── depends on: resource::resource_store → NodeResourceId
├── depends on: rms::slurm_rms::api_client::response::tasks → SlurmOptionExt, SlurmTask
└── depends on: utils::id → ClientId, ComponentId, ReservationName, ResourceName

reservation_notification_listener.rs
├── depends on: reservation.rs → ReservationState
├── depends on: reservation_store.rs → ReservationId
└── depends on: utils::id → ReservationName

vrm_state_listener.rs
├── depends on: reservation.rs → ReservationState
├── depends on: reservation_store.rs → ReservationId
├── depends on: reservation_notification_listener.rs → ReservationNotificationListener
└── depends on: utils::id → ReservationName

reservation_sync_gate.rs
├── depends on: reservation.rs → ReservationState
└── depends on: reservation_store.rs → ReservationId
└── depends on: utils::id → ComponentId
```

## External Dependencies (Cargo.toml)

| Crate | Used By | Purpose |
|-------|---------|---------|
| `slotmap` | `reservation_store.rs` | O(1) key-based storage with generation counters |
| `parking_lot` | `reservation_store.rs`, `reservation_sync_gate.rs` | Deadlock-detecting RwLock, Mutex, Condvar |
| `serde` | `reservation.rs`, `link_reservation.rs`, `node_reservation.rs` | JSON serialization for network/distributed state transfer |
| `rand` | `reservations.rs` | Random reservation selection |
| `log` | All files | Structured logging |
| ~~`std::sync` (stdlib)~~ | ~~`reservation_sync_gate.rs`~~ | ⚠️ **Resolved**: Migrated to `parking_lot` |

## Hierarchy Diagram

```
External Consumers
    │
    ▼
┌────────────────────────────────────────────────────┐
│                 reservation::mod                    │
│  (re-exports all public types and traits)          │
└────────────────────────────────────────────────────┘
    │
    ├── reservation::reservation (core enum, base struct, state enums, trait)
    ├── reservation::reservation_store (central repository)
    ├── reservation::reservations (tracking subset)
    ├── reservation::probe_reservations (scheduling exploration)
    ├── reservation::link_reservation (link-specific data)
    ├── reservation::node_reservation (node-specific data)
    ├── reservation::reservation_notification_listener (observer trait)
    ├── reservation::vrm_state_listener (concrete observer)
    └── reservation::reservation_sync_gate (sync primitive)
```
