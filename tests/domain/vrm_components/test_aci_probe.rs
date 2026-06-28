use std::sync::Arc;

use vrm_rust_workflow::domain::simulator::simulator::GlobalClock;
use vrm_rust_workflow::domain::vrm_system_model::grid_resource_management_system::vrm_component_trait::VrmComponent;
use vrm_rust_workflow::domain::vrm_system_model::reservation::probe_reservations::ProbeReservationComparator;
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation::ReservationState;
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use vrm_rust_workflow::domain::vrm_system_model::utils::id::ReservationName;

use crate::common::{create_dummy_aci, create_node_reservation};

/// Normal probe request with subsequent promotion of the request.  
#[tokio::test]
async fn test_probe() {
    let clock = Arc::new(GlobalClock::new(true));
    let store = ReservationStore::new();
    let res_name = ReservationName::new("test_job_to_reserve".to_string());
    let mut aci = create_dummy_aci(clock.clone(), store.clone()).await;
    let node_reservation = create_node_reservation(res_name, 2, 0, 5, ReservationState::Open, clock);
    let res_id = store.add(node_reservation);
    let mut probe_reservations = aci.probe(res_id, None);

    assert_eq!(1, probe_reservations.get_ids().len());
    assert_eq!(store.get_state(res_id), ReservationState::ProbeAnswer, "Probe process was not successful.");

    if let Some((component_id, shadow_scheduling_id)) = probe_reservations.prompt_best(res_id, ProbeReservationComparator::EFTReservationCompare) {
        assert!(component_id.compare(&aci.id.clone().cast()), "Unexpected AcI id.");
        assert!(shadow_scheduling_id.is_none(), "Probe was on shadow schedule performed.");
        assert_eq!(store.get_state(res_id), ReservationState::ProbeReservation, "Probe process was not successful.");

        // Transfer the reservation in a valid reserve state.
        store.update_state(res_id, ReservationState::ReserveProbeReservation);

        // Is only one possible ProbeReservation --> try to reserve it.
        let _ = aci.reserve(res_id, None);
        assert_eq!(store.get_state(res_id), ReservationState::ReserveAnswer, "Reservation process was not successful.");
    } else {
        assert!(false, "Error in the probe or reserve process happen.");
    }
}

/// Normal best_probe request with subsequent promotion of the request.  
#[tokio::test]
async fn test_best_probe() {
    let clock = Arc::new(GlobalClock::new(true));
    let store = ReservationStore::new();
    let res_name = ReservationName::new("test_job_to_reserve".to_string());
    let mut aci = create_dummy_aci(clock.clone(), store.clone()).await;
    let node_reservation = create_node_reservation(res_name, 2, 0, 5, ReservationState::Open, clock);
    let res_id = store.add(node_reservation);
    let mut probe_reservations = aci.probe_best(res_id, None, ProbeReservationComparator::EFTReservationCompare);

    assert_eq!(1, probe_reservations.get_ids().len());
    assert_eq!(store.get_state(res_id), ReservationState::ProbeAnswer, "Probe best process was not successful.");

    if let Some((component_id, shadow_scheduling_id)) = probe_reservations.prompt_best(res_id, ProbeReservationComparator::EFTReservationCompare) {
        assert!(component_id.compare(&aci.id.clone().cast()), "Unexpected AcI id.");
        assert!(shadow_scheduling_id.is_none(), "Probe was on shadow schedule performed.");
        assert_eq!(store.get_state(res_id), ReservationState::ProbeReservation, "Probe process was not successful.");

        // Transfer the reservation in a valid reserve state.
        store.update_state(res_id, ReservationState::ReserveProbeReservation);

        // Is only one possible ProbeReservation --> try to reserve it.
        let _ = aci.reserve(res_id, None);
        assert_eq!(store.get_state(res_id), ReservationState::ReserveAnswer, "Reservation process was not successful.");
    } else {
        assert!(false, "Error in the promotion process happened.");
    }
    
}
