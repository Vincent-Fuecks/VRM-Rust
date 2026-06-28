//! Slot Unit Tests (TC-001 — TC-006)

use std::collections::HashSet;

use vrm_rust_workflow::vrm::reservation::{node_reservation::NodeReservation, reservation::{Reservation, ReservationProceeding, ReservationState}, reservation_store::ReservationStore};
use vrm_rust_workflow::domain::vrm_system_model::schedule::slotted_schedule::slot::Slot;
use vrm_rust_workflow::domain::vrm_system_model::utils::id::{ClientId, ReservationName};

fn make_id(store: &ReservationStore, name: &str) -> vrm_rust_workflow::vrm::reservation::reservation_store::ReservationId {
    let r = Reservation::Node(NodeReservation::new(
        ReservationName::new(name.to_string()),
        ClientId::new("test"),
        None, ReservationState::Open, ReservationProceeding::Reserve,
        0, 0, 100000, 3600, 4, false, 0.0, HashSet::new(),
        None, None, "/t".into(), None, None,
    ));
    store.add(r)
}

/// TC-001: Slot — Newly Created Slot Has Zero Load
#[test]
fn test_tc001_slot_new_has_zero_load() {
    let slot = Slot::new(64);
    assert_eq!(slot.capacity, 64);
    assert_eq!(slot.load, 0);
    assert!(slot.reservation_ids.is_empty());
}

/// TC-002: Slot — Insert Reservation Sufficient Capacity
#[test]
fn test_tc002_slot_insert_sufficient_capacity() {
    let store = ReservationStore::new();
    let id = make_id(&store, "tc002");
    let mut slot = Slot::new(16);

    let result = slot.insert_reservation(5, id);

    assert!(result);
    assert_eq!(slot.load, 5);
    assert!(slot.reservation_ids.contains(&id));
}

/// TC-003: Slot — Insert Duplicate Reservation Returns False
#[test]
fn test_tc003_slot_insert_duplicate_returns_false() {
    let store = ReservationStore::new();
    let id = make_id(&store, "tc003");
    let mut slot = Slot::new(16);

    assert!(slot.insert_reservation(5, id));
    let result = slot.insert_reservation(5, id);

    assert!(!result);
    assert_eq!(slot.load, 5);
}

/// TC-004: Slot — Insert Reservation Over Capacity Returns False
#[test]
fn test_tc004_slot_insert_over_capacity_returns_false() {
    let store = ReservationStore::new();
    let id1 = make_id(&store, "tc004a");
    let id2 = make_id(&store, "tc004b");
    let mut slot = Slot::new(10);

    assert!(slot.insert_reservation(8, id1));
    let result = slot.insert_reservation(5, id2);

    assert!(!result);
    assert_eq!(slot.load, 8);
    assert!(!slot.reservation_ids.contains(&id2));
}

/// TC-005: Slot — Delete Reservation Reduces Load
#[test]
fn test_tc005_slot_delete_reservation_reduces_load() {
    let store = ReservationStore::new();
    let id = make_id(&store, "tc005");
    let mut slot = Slot::new(20);

    assert!(slot.insert_reservation(8, id));
    let result = slot.delete_reservation(id, 8);

    assert!(result);
    assert_eq!(slot.load, 0);
    assert!(!slot.reservation_ids.contains(&id));
}

/// TC-006: Slot — Delete Non-Existent Reservation Returns False
#[test]
fn test_tc006_slot_delete_non_existent_returns_false() {
    let store = ReservationStore::new();
    let ghost_id = make_id(&store, "tc006ghost");
    let mut slot = Slot::new(16);

    let result = slot.delete_reservation(ghost_id, 5);

    assert!(!result);
    assert_eq!(slot.load, 0);
}