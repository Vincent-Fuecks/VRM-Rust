use std::sync::Arc;

use vrm_rust_workflow::domain::simulator::simulator::GlobalClock;
use vrm_rust_workflow::domain::vrm_system_model::grid_resource_management_system::vrm_component_trait::VrmComponent;
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation::ReservationState;
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use vrm_rust_workflow::domain::vrm_system_model::utils::id::ReservationName;

use crate::common::{create_dummy_aci, create_node_reservation};

/// Delete prior reserved reservation.
#[tokio::test]
async fn test_delete() {
    let clock = Arc::new(GlobalClock::new(true));
    let store = ReservationStore::new();
    let res_name = ReservationName::new("test_job_to_reserve".to_string());
    let mut aci = create_dummy_aci(clock.clone(), store.clone()).await;
    let node_reservation = create_node_reservation(res_name, 2, 0, 5, ReservationState::Open, clock);
    let res_id = store.add(node_reservation);

    // Reserve Reservation
    let _ = aci.reserve(res_id, None);
    assert_eq!(store.get_state(res_id), ReservationState::ReserveAnswer, "Reservation process was not successful.");

    let _ = aci.delete(res_id, None);
    assert_eq!(store.get_state(res_id), ReservationState::Deleted);
}