use std::sync::Arc;

use vrm_rust_workflow::schema::reservation_dto::{ReservationProceedingDto, ReservationStateDto};
use vrm_rust_workflow::schema::vrm_dto::VrmDto;
use vrm_rust_workflow::vrm::global_clock::global_clock::{GlobalClock, GlobalClockDto};
use vrm_rust_workflow::vrm::reservation::reservation::ReservationState;
use vrm_rust_workflow::vrm::reservation::reservation_store::ReservationStore;
use vrm_rust_workflow::vrm::vrm_component::vrm_component_registry::registry_client::RegistryClient;
use vrm_rust_workflow::vrm::vrm_manager::VrmManager;

use crate::vrm::common::{get_aci_dto, get_adc_dto, get_clients, get_workflow_dto_with_one_task};

/// Test Probe
#[tokio::test]
async fn test_probe() {
    let store = ReservationStore::new();
    let clock_dto = GlobalClockDto { is_simulation: true };
    let adc_master_id = "ADC-Master".to_string();
    let aci_id = "AcI-001".to_string();
    let client_id = "Test-Client-001".to_string();
    let workflow_id = "Test-Direct-Mapping-Workflow".to_string();

    let aci_dtos = vec![get_aci_dto(adc_master_id.clone())];
    let adc_dtos = vec![get_adc_dto(adc_master_id.clone(), vec![aci_id])];

    let vrm_dto = VrmDto { aci: aci_dtos, adc: adc_dtos, adc_master_id: adc_master_id, simulator: clock_dto };
    let is_simulation = vrm_dto.simulator.is_simulation;
    let workflow_dto = get_workflow_dto_with_one_task(workflow_id, ReservationStateDto::Open, ReservationProceedingDto::Probe);

    let unprocessed_reservations = get_clients(client_id, vec![workflow_dto], store.clone()).unprocessed_reservations;
    let res_id = unprocessed_reservations.get(0).expect("Workflow should not be empty.").clone();

    let registry = RegistryClient::new();
    let simulator = Arc::new(GlobalClock::new(is_simulation));

    let mut vrm_manager = VrmManager::init_vrm_system(vrm_dto, unprocessed_reservations, simulator, registry, store.clone())
        .await
        .expect("Failed to initialize VRM system");

    vrm_manager.run_vrm().await;

    assert_eq!(store.get_state(res_id), ReservationState::Deleted);
}

/// Test reserve
#[tokio::test]
async fn test_reserve() {
    let store = ReservationStore::new();
    let clock_dto = GlobalClockDto { is_simulation: true };
    let adc_master_id = "ADC-Master".to_string();
    let aci_id = "AcI-001".to_string();
    let client_id = "Test-Client-001".to_string();
    let workflow_id = "Test-Direct-Mapping-Workflow".to_string();

    let aci_dtos = vec![get_aci_dto(adc_master_id.clone())];
    let adc_dtos = vec![get_adc_dto(adc_master_id.clone(), vec![aci_id])];

    let vrm_dto = VrmDto { aci: aci_dtos, adc: adc_dtos, adc_master_id: adc_master_id, simulator: clock_dto };
    let is_simulation = vrm_dto.simulator.is_simulation;
    let workflow_dto = get_workflow_dto_with_one_task(workflow_id, ReservationStateDto::Open, ReservationProceedingDto::Reserve);

    let unprocessed_reservations = get_clients(client_id, vec![workflow_dto], store.clone()).unprocessed_reservations;
    let res_id = unprocessed_reservations.get(0).expect("Workflow should not be empty.").clone();

    let registry = RegistryClient::new();
    let simulator = Arc::new(GlobalClock::new(is_simulation));

    let mut vrm_manager = VrmManager::init_vrm_system(vrm_dto, unprocessed_reservations, simulator, registry, store.clone())
        .await
        .expect("Failed to initialize VRM system");

    vrm_manager.run_vrm().await;

    assert_eq!(store.get_state(res_id), ReservationState::ReserveAnswer);
}

/// Test Commit
#[tokio::test]
async fn test_commit() {
    let store = ReservationStore::new();
    let clock_dto = GlobalClockDto { is_simulation: true };
    let adc_master_id = "ADC-Master".to_string();
    let aci_id = "AcI-001".to_string();
    let client_id = "Test-Client-001".to_string();
    let workflow_id = "Test-Direct-Mapping-Workflow".to_string();

    let aci_dtos = vec![get_aci_dto(adc_master_id.clone())];
    let adc_dtos = vec![get_adc_dto(adc_master_id.clone(), vec![aci_id])];

    let vrm_dto = VrmDto { aci: aci_dtos, adc: adc_dtos, adc_master_id: adc_master_id, simulator: clock_dto };
    let is_simulation = vrm_dto.simulator.is_simulation;
    let workflow_dto = get_workflow_dto_with_one_task(workflow_id, ReservationStateDto::Open, ReservationProceedingDto::Commit);

    let unprocessed_reservations = get_clients(client_id, vec![workflow_dto], store.clone()).unprocessed_reservations;
    let res_id = unprocessed_reservations.get(0).expect("Workflow should not be empty.").clone();

    let registry = RegistryClient::new();
    let simulator = Arc::new(GlobalClock::new(is_simulation));

    let mut vrm_manager = VrmManager::init_vrm_system(vrm_dto, unprocessed_reservations, simulator, registry, store.clone())
        .await
        .expect("Failed to initialize VRM system");

    vrm_manager.run_vrm().await;

    assert_eq!(store.get_state(res_id), ReservationState::Committed);
}
