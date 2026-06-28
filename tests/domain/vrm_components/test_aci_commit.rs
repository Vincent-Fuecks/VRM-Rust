use std::sync::Arc;

use vrm_rust_workflow::domain::simulator::simulator::GlobalClock;
use vrm_rust_workflow::domain::vrm_system_model::grid_resource_management_system::vrm_component_trait::VrmComponent;
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation::ReservationState;
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use vrm_rust_workflow::domain::vrm_system_model::utils::id::ReservationName;

use crate::common::{create_dummy_aci, create_node_reservation};

/// Submit reservation without prior reserve
#[tokio::test]
async fn test_commit_without_prior_reserve() {
    let clock = Arc::new(GlobalClock::new(true));
    let store = ReservationStore::new();
    let res_name = ReservationName::new("test_job_to_reserve".to_string());
    let mut aci = create_dummy_aci(clock.clone(), store.clone()).await;
    let node_reservation = create_node_reservation(res_name, 2, 0, 5, ReservationState::Open, clock);
    let res_id = store.add(node_reservation);
    let is_committed = aci.commit(res_id);

    assert!(is_committed, "Reservation was not successful commit.");
    assert_eq!(store.get_state(res_id), ReservationState::Committed, "Commit process was not successful.");
}

/// Submit reservation with prior reserve
#[tokio::test]
async fn test_commit_with_prior_reserve() {
    let clock = Arc::new(GlobalClock::new(true));
    let store = ReservationStore::new();
    let res_name = ReservationName::new("test_job_to_reserve".to_string());
    let mut aci = create_dummy_aci(clock.clone(), store.clone()).await;
    let node_reservation = create_node_reservation(res_name, 2, 0, 5, ReservationState::Open, clock);
    let res_id = store.add(node_reservation);

    // Reserve Reservation
    let _ = aci.reserve(res_id, None);
    assert_eq!(store.get_state(res_id), ReservationState::ReserveAnswer, "Reservation process was not successful.");

    // Commit Reservation
    let is_committed = aci.commit(res_id);
    assert!(is_committed, "Reservation was not successful commit.");
    assert_eq!(store.get_state(res_id), ReservationState::Committed, "Commit process was not successful.");
}

/// Submit reservation with prior reserve in false reservation state.
#[tokio::test]
async fn test_commit_with_prior_reserve_invalid_state() {
    let clock = Arc::new(GlobalClock::new(true));
    let store = ReservationStore::new();
    let res_name = ReservationName::new("test_job_to_reserve".to_string());
    let mut aci = create_dummy_aci(clock.clone(), store.clone()).await;
    let node_reservation = create_node_reservation(res_name, 2, 0, 5, ReservationState::Open, clock);
    let res_id = store.add(node_reservation);

    // Reserve Reservation
    let _ = aci.reserve(res_id, None);
    assert_eq!(store.get_state(res_id), ReservationState::ReserveAnswer, "Reservation process was not successful.");

    // Change Reservation into invalid state
    store.update_state(res_id, ReservationState::Deleted);

    // Commit Reservation
    let is_committed = aci.commit(res_id);
    assert!(is_committed == false, "Reservation was successful commit.");
    assert_eq!(store.get_state(res_id), ReservationState::Rejected, "Reservation to Commit was not Rejected as expected.");
}

/// Submit reservation with invalid start and end time (start before end).
#[tokio::test]
async fn test_commit_invalid_end_time() {
    let clock = Arc::new(GlobalClock::new(true));
    let store = ReservationStore::new();
    let res_name = ReservationName::new("test_job_to_reserve".to_string());
    let mut aci = create_dummy_aci(clock.clone(), store.clone()).await;
    let node_reservation = create_node_reservation(res_name, 2, 100, 50, ReservationState::Open, clock);
    let res_id = store.add(node_reservation);
    let is_committed = aci.commit(res_id);

    assert!(is_committed == false, "Reservation was successful commit.");
    assert_eq!(store.get_state(res_id), ReservationState::Rejected, "Commit process was successful.");
}

/// Submit reservation in invalid state
#[tokio::test]
async fn test_commit_reservation_with_invalid_reservation_state() {
    let clock = Arc::new(GlobalClock::new(true));
    let store = ReservationStore::new();
    let res_name = ReservationName::new("test_job_to_reserve".to_string());
    let mut aci = create_dummy_aci(clock.clone(), store.clone()).await;
    let node_reservation = create_node_reservation(res_name, 2, 100, 50, ReservationState::Deleted, clock);
    let res_id = store.add(node_reservation);
    let is_committed = aci.commit(res_id);

    assert!(is_committed == false, "Reservation was successful commit.");
    assert_eq!(store.get_state(res_id), ReservationState::Rejected, "Commit process was successful.");
}
