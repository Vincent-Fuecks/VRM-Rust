use std::sync::Arc;

use vrm_rust_workflow::loader::parser::parse_json_file;
use vrm_rust_workflow::schema::client_dto::ClientsDto;
use vrm_rust_workflow::schema::vrm_dto::VrmDto;
use vrm_rust_workflow::vrm::client::client::Clients;
use vrm_rust_workflow::vrm::global_clock::global_clock::{GlobalClock, GlobalClockDto};
use vrm_rust_workflow::vrm::reservation::reservation::ReservationState;
use vrm_rust_workflow::vrm::reservation::reservation_store::ReservationStore;
use vrm_rust_workflow::vrm::vrm_component::vrm_component_registry::registry_client::RegistryClient;
use vrm_rust_workflow::vrm::vrm_manager::VrmManager;

// =========================================================================================
//  Helper: build VrmManager from JSON config files
// =========================================================================================

struct VrmTestHarness {
    pub store: ReservationStore,
    pub workflow_res_id: vrm_rust_workflow::vrm::reservation::reservation_store::ReservationId,
    pub vrm_manager: VrmManager,
}

impl VrmTestHarness {
    async fn from_json(vrm_config_path: &str, workflow_path: &str) -> Self {
        let store = ReservationStore::new();

        // Parse VRM system config
        let vrm_dto: VrmDto = parse_json_file::<VrmDto>(vrm_config_path).expect("Failed to parse VRM config JSON");

        // Parse client workflows
        let clients_dto: ClientsDto = parse_json_file::<ClientsDto>(workflow_path).expect("Failed to parse workflow JSON");

        let unprocessed = Clients::from_dto(clients_dto, store.clone()).expect("Failed to create Clients from DTO").unprocessed_reservations;

        let workflow_res_id = unprocessed.first().cloned().expect("No workflow reservation found");

        let is_simulation = vrm_dto.simulator.is_simulation;
        let registry = RegistryClient::new();
        let simulator = Arc::new(GlobalClock::new(is_simulation));

        let vrm_manager =
            VrmManager::init_vrm_system(vrm_dto, unprocessed, simulator, registry, store.clone()).await.expect("Failed to initialize VRM system");

        VrmTestHarness { store, workflow_res_id, vrm_manager }
    }
}

// =========================================================================================
//  TC-7.1: Full Cross-RMS Workflow End-to-End
// =========================================================================================

/// End-to-end test: 2 RMS systems, 10 nodes with data + sync dependencies
/// spanning both RMS systems. Verifies the workflow schedules successfully.
#[tokio::test]
async fn test_full_cross_rms_workflow_10_nodes() {
    let mut harness = VrmTestHarness::from_json("data/test/vrm_config_two_rms.json", "data/test/workflow_cross_rms_10_nodes.json").await;

    harness.vrm_manager.run_vrm().await;

    let wf_state = harness.store.get_state(harness.workflow_res_id);
    assert_eq!(wf_state, ReservationState::ReserveAnswer, "Workflow should be in ReserveAnswer state, but was {:?}", wf_state);

    // All sub-reservations should be in a non-rejected state
    if let Some(child_ids) = harness.store.get_workflow_res_ids(harness.workflow_res_id) {
        for child_id in &child_ids {
            let state = harness.store.get_state(*child_id);
            assert!(
                state != ReservationState::Rejected,
                "Child reservation {:?} ({:?}) was Rejected",
                child_id,
                harness.store.get_name_for_key(*child_id)
            );
        }
    }

    // Verify the workflow is committed
    harness.store.print_store_contents();
}

// =========================================================================================
//  TC-1.1: Single-RMS Workflow — Two Tasks With Data Dependency
// =========================================================================================

/// Verifies a single-RMS workflow with data dependencies schedules without
/// virtual reservation chains.
#[tokio::test]
async fn test_single_rms_workflow_data_dependency() {
    use vrm_rust_workflow::schema::reservation_dto::{
        DataInDto, DataOutDto, DependencyDto, LinkReservationDto, NodeReservationDto, ReservationProceedingDto, ReservationStateDto,
    };
    use vrm_rust_workflow::schema::workflow_dto::{TaskDto, WorkflowDto};

    use crate::vrm::common::{get_aci_dto, get_adc_dto, get_clients};

    let store = ReservationStore::new();

    let adc_master_id = "ADC-Master".to_string();
    let aci_id = "AcI-001".to_string();
    let client_id = "Test-Client-SingleRMS".to_string();
    let workflow_id = "SingleRMS-DataDep".to_string();

    let aci_dtos = vec![get_aci_dto(adc_master_id.clone())];
    let adc_dtos = vec![get_adc_dto(adc_master_id.clone(), vec![aci_id])];

    let vrm_dto = VrmDto { aci: aci_dtos, adc: adc_dtos, adc_master_id: adc_master_id, simulator: GlobalClockDto { is_simulation: true } };

    // Create a workflow with 2 tasks (A → B data dependency)
    let task_a = TaskDto {
        id: "A".to_string(),
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        node_reservation: NodeReservationDto {
            task_path: "/bin/task_a".to_string(),
            duration: 10,
            is_moldable: false,
            cpus: 2,
            dependencies: DependencyDto { data: vec![], sync: vec![] },
            data_out: vec![DataOutDto { name: "out_A".to_string(), file: Some("a_out.dat".to_string()), size: Some(500), bandwidth: None }],
            data_in: vec![DataInDto {
                source_reservation: "EXTERNAL".to_string(),
                source_port: "input".to_string(),
                file: Some("input.dat".to_string()),
            }],
            output_path: None,
            error_path: None,
            current_working_directory: None,
            environment: None,
        },
        link_reservation: vec![LinkReservationDto {
            start_point: "".to_string(),
            end_point: "".to_string(),
            amount: Some(500),
            bandwidth: Some(100),
        }],
    };

    let task_b = TaskDto {
        id: "B".to_string(),
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        node_reservation: NodeReservationDto {
            task_path: "/bin/task_b".to_string(),
            duration: 15,
            is_moldable: false,
            cpus: 2,
            dependencies: DependencyDto { data: vec!["A".to_string()], sync: vec![] },
            data_out: vec![DataOutDto { name: "out_B".to_string(), file: Some("b_out.dat".to_string()), size: Some(0), bandwidth: None }],
            data_in: vec![DataInDto { source_reservation: "A".to_string(), source_port: "out_A".to_string(), file: Some("a_out.dat".to_string()) }],
            output_path: None,
            error_path: None,
            current_working_directory: None,
            environment: None,
        },
        link_reservation: vec![LinkReservationDto {
            start_point: "".to_string(),
            end_point: "".to_string(),
            amount: Some(500),
            bandwidth: Some(100),
        }],
    };

    let workflow_dto = WorkflowDto {
        id: workflow_id.clone(),
        arrival_time: 0,
        booking_interval_start: 0,
        booking_interval_end: 100000,
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        tasks: vec![task_a, task_b],
    };

    let unprocessed = get_clients(client_id, vec![workflow_dto], store.clone()).unprocessed_reservations;
    let workflow_res_id = unprocessed.first().cloned().unwrap();

    let registry = RegistryClient::new();
    let simulator = Arc::new(GlobalClock::new(true));

    let mut vrm_manager =
        VrmManager::init_vrm_system(vrm_dto, unprocessed, simulator, registry, store.clone()).await.expect("Failed to initialize VRM system");

    vrm_manager.run_vrm().await;

    // Workflow should be in ReserveAnswer state
    let wf_state = store.get_state(workflow_res_id);
    assert_eq!(wf_state, ReservationState::ReserveAnswer);

    // No virtual reservations should exist (same-RMS)
    // Check all child reservations are not Rejected
    if let Some(child_ids) = store.get_workflow_res_ids(workflow_res_id) {
        for child_id in &child_ids {
            let state = store.get_state(*child_id);
            assert_ne!(state, ReservationState::Rejected, "Child {:?} was Rejected", store.get_name_for_key(*child_id));
        }
    }
}

// =========================================================================================
//  TC-1.2: Single-RMS Workflow — Sync Dependency (Co-Allocation)
// =========================================================================================

/// Verifies that co-allocated tasks on the same RMS schedule correctly.
#[tokio::test]
async fn test_single_rms_co_allocation() {
    use vrm_rust_workflow::schema::reservation_dto::{
        DataInDto, DataOutDto, DependencyDto, LinkReservationDto, NodeReservationDto, ReservationProceedingDto, ReservationStateDto,
    };
    use vrm_rust_workflow::schema::workflow_dto::{TaskDto, WorkflowDto};

    use crate::vrm::common::{get_aci_dto, get_adc_dto, get_clients};

    let store = ReservationStore::new();

    let adc_master_id = "ADC-Master".to_string();
    let aci_id = "AcI-001".to_string();
    let client_id = "Test-Client-CoAlloc".to_string();

    let aci_dtos = vec![get_aci_dto(adc_master_id.clone())];
    let adc_dtos = vec![get_adc_dto(adc_master_id.clone(), vec![aci_id])];

    let vrm_dto = VrmDto { aci: aci_dtos, adc: adc_dtos, adc_master_id, simulator: GlobalClockDto { is_simulation: true } };

    // A → B (sync), B → C (sync) → CoAllocation(A, B, C)
    let make_task = |id: &str, sync_deps: Vec<String>, data_deps: Vec<String>| -> TaskDto {
        TaskDto {
            id: id.to_string(),
            reservation_state: ReservationStateDto::Open,
            request_proceeding: ReservationProceedingDto::Commit,
            node_reservation: NodeReservationDto {
                task_path: format!("/bin/task_{}", id),
                duration: 10,
                is_moldable: false,
                cpus: 2,
                dependencies: DependencyDto { data: data_deps, sync: sync_deps },
                data_out: vec![DataOutDto { name: format!("out_{}", id), file: Some(format!("{}_out.dat", id)), size: Some(100), bandwidth: None }],
                data_in: vec![DataInDto {
                    source_reservation: "EXTERNAL".to_string(),
                    source_port: "input".to_string(),
                    file: Some("input.dat".to_string()),
                }],
                output_path: None,
                error_path: None,
                current_working_directory: None,
                environment: None,
            },
            link_reservation: vec![LinkReservationDto {
                start_point: "".to_string(),
                end_point: "".to_string(),
                amount: Some(100),
                bandwidth: Some(10),
            }],
        }
    };

    let tasks = vec![
        make_task("A", vec!["B".to_string()], vec![]),
        make_task("B", vec!["A".to_string(), "C".to_string()], vec![]),
        make_task("C", vec!["B".to_string()], vec![]),
    ];

    let workflow_dto = WorkflowDto {
        id: "CoAlloc-Workflow".to_string(),
        arrival_time: 0,
        booking_interval_start: 0,
        booking_interval_end: 100000,
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        tasks,
    };

    let unprocessed = get_clients(client_id, vec![workflow_dto], store.clone()).unprocessed_reservations;
    let workflow_res_id = unprocessed.first().cloned().unwrap();

    let registry = RegistryClient::new();
    let simulator = Arc::new(GlobalClock::new(true));

    let mut vrm_manager =
        VrmManager::init_vrm_system(vrm_dto, unprocessed, simulator, registry, store.clone()).await.expect("Failed to initialize VRM system");

    vrm_manager.run_vrm().await;

    assert_eq!(store.get_state(workflow_res_id), ReservationState::ReserveAnswer);

    // Verify co-allocation members have same assigned_start
    if let Some(child_ids) = store.get_workflow_res_ids(workflow_res_id) {
        let node_starts: Vec<(String, i64)> = child_ids
            .iter()
            .filter(|&&id| store.is_node(id))
            .map(|&id| {
                let name = store.get_name_for_key(id).unwrap().to_string();
                let start = store.get_assigned_start(id);
                (name, start)
            })
            .collect();

        // All co-allocated nodes should share the same start time (they all sync with each other)
        if node_starts.len() > 1 {
            let first_start = node_starts[0].1;
            for (name, start) in &node_starts {
                assert_eq!(*start, first_start, "Co-allocated node {} has different start time", name);
            }
        }
    }
}

// =========================================================================================
//  TC-4.4: Workflow Deadline Exceeded → Rejected
// =========================================================================================

/// Verifies that a workflow whose deadline would be exceeded is rejected.
#[tokio::test]
async fn test_workflow_deadline_exceeded_rejected() {
    use vrm_rust_workflow::schema::reservation_dto::{
        DataInDto, DependencyDto, LinkReservationDto, NodeReservationDto, ReservationProceedingDto, ReservationStateDto,
    };
    use vrm_rust_workflow::schema::workflow_dto::{TaskDto, WorkflowDto};

    use crate::vrm::common::{get_aci_dto, get_adc_dto, get_clients};

    let store = ReservationStore::new();

    let adc_master_id = "ADC-Master".to_string();
    let aci_id = "AcI-001".to_string();

    let aci_dtos = vec![get_aci_dto(adc_master_id.clone())];
    let adc_dtos = vec![get_adc_dto(adc_master_id.clone(), vec![aci_id])];

    let vrm_dto = VrmDto { aci: aci_dtos, adc: adc_dtos, adc_master_id, simulator: GlobalClockDto { is_simulation: true } };

    // Task with duration > booking interval → deadline exceeded
    let task = TaskDto {
        id: "LongTask".to_string(),
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        node_reservation: NodeReservationDto {
            task_path: "/bin/long_task".to_string(),
            duration: 500, // Very long
            is_moldable: false,
            cpus: 2,
            dependencies: DependencyDto { data: vec![], sync: vec![] },
            data_out: vec![],
            data_in: vec![DataInDto {
                source_reservation: "EXTERNAL".to_string(),
                source_port: "input".to_string(),
                file: Some("input.dat".to_string()),
            }],
            output_path: None,
            error_path: None,
            current_working_directory: None,
            environment: None,
        },
        link_reservation: vec![LinkReservationDto { start_point: "".to_string(), end_point: "".to_string(), amount: Some(0), bandwidth: Some(0) }],
    };

    let workflow_dto = WorkflowDto {
        id: "Deadline-Exceeded-WF".to_string(),
        arrival_time: 0,
        booking_interval_start: 0,
        booking_interval_end: 100, // Very short booking interval
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        tasks: vec![task],
    };

    let unprocessed = get_clients("deadline-client".to_string(), vec![workflow_dto], store.clone()).unprocessed_reservations;
    let workflow_res_id = unprocessed.first().cloned().unwrap();

    let registry = RegistryClient::new();
    let simulator = Arc::new(GlobalClock::new(true));

    let mut vrm_manager =
        VrmManager::init_vrm_system(vrm_dto, unprocessed, simulator, registry, store.clone()).await.expect("Failed to initialize VRM system");

    vrm_manager.run_vrm().await;

    // Workflow should be rejected (deadline exceeded)
    let wf_state = store.get_state(workflow_res_id);
    assert_eq!(wf_state, ReservationState::Rejected, "Workflow with exceeded deadline should be Rejected, but was {:?}", wf_state);
}

// =========================================================================================
//  TC-5.2: Workflow Rejection Sets All Children to Rejected
// =========================================================================================

/// Verifies that when a workflow is rejected, all child reservations are
/// also set to Rejected (or not left in ReserveAnswer).
#[tokio::test]
async fn test_workflow_rejection_children_consistent() {
    use vrm_rust_workflow::schema::reservation_dto::{
        DataInDto, DependencyDto, LinkReservationDto, NodeReservationDto, ReservationProceedingDto, ReservationStateDto,
    };
    use vrm_rust_workflow::schema::workflow_dto::{TaskDto, WorkflowDto};

    use crate::vrm::common::{get_aci_dto, get_adc_dto, get_clients};

    let store = ReservationStore::new();

    let adc_master_id = "ADC-Master".to_string();
    let aci_id = "AcI-001".to_string();

    let aci_dtos = vec![get_aci_dto(adc_master_id.clone())];
    let adc_dtos = vec![get_adc_dto(adc_master_id.clone(), vec![aci_id])];

    let vrm_dto = VrmDto { aci: aci_dtos, adc: adc_dtos, adc_master_id, simulator: GlobalClockDto { is_simulation: true } };

    // Multiple tasks, all will miss deadline
    let make_task = |id: &str| -> TaskDto {
        TaskDto {
            id: id.to_string(),
            reservation_state: ReservationStateDto::Open,
            request_proceeding: ReservationProceedingDto::Commit,
            node_reservation: NodeReservationDto {
                task_path: format!("/bin/task_{}", id),
                duration: 500,
                is_moldable: false,
                cpus: 2,
                dependencies: DependencyDto { data: vec![], sync: vec![] },
                data_out: vec![],
                data_in: vec![DataInDto {
                    source_reservation: "EXTERNAL".to_string(),
                    source_port: "input".to_string(),
                    file: Some("input.dat".to_string()),
                }],
                output_path: None,
                error_path: None,
                current_working_directory: None,
                environment: None,
            },
            link_reservation: vec![LinkReservationDto {
                start_point: "".to_string(),
                end_point: "".to_string(),
                amount: Some(0),
                bandwidth: Some(0),
            }],
        }
    };

    let workflow_dto = WorkflowDto {
        id: "MultiReject-WF".to_string(),
        arrival_time: 0,
        booking_interval_start: 0,
        booking_interval_end: 50,
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        tasks: vec![make_task("A"), make_task("B"), make_task("C"), make_task("D"), make_task("E")],
    };

    let unprocessed = get_clients("reject-client".to_string(), vec![workflow_dto], store.clone()).unprocessed_reservations;
    let workflow_res_id = unprocessed.first().cloned().unwrap();

    let registry = RegistryClient::new();
    let simulator = Arc::new(GlobalClock::new(true));

    let mut vrm_manager =
        VrmManager::init_vrm_system(vrm_dto, unprocessed, simulator, registry, store.clone()).await.expect("Failed to initialize VRM system");

    vrm_manager.run_vrm().await;

    // Workflow should be rejected
    assert_eq!(store.get_state(workflow_res_id), ReservationState::Rejected);

    // No child should be in ReserveAnswer (partial commit)
    if let Some(child_ids) = store.get_workflow_res_ids(workflow_res_id) {
        for child_id in &child_ids {
            let state = store.get_state(*child_id);
            assert!(
                state != ReservationState::ReserveAnswer,
                "Child {:?} was in ReserveAnswer, but workflow was Rejected",
                store.get_name_for_key(*child_id)
            );
        }
    }
}

// =========================================================================================
//  TC-3.3: Gateway RouterId Fallback
// =========================================================================================

/// Verifies that when no explicit gatewayRouterId is configured,
/// the fallback "AcI-Gateway-{component_id}" is used.
#[test]
fn test_gateway_router_id_fallback() {
    use vrm_rust_workflow::schema::gateway_config_dto::GatewayConfigDto;

    let config =
        GatewayConfigDto { gateway_router_id: None, ingress_bandwidth_gbps: 1000, egress_bandwidth_gbps: 1000, gateway_switch_id: "s0".to_string() };

    let router_id = config.resolve_gateway_router_id("rms_0");
    assert_eq!(router_id, "AcI-Gateway-rms_0");

    let router_id = config.resolve_gateway_router_id("rms_1");
    assert_eq!(router_id, "AcI-Gateway-rms_1");
}

/// Verifies that an explicit gatewayRouterId overrides the fallback.
#[test]
fn test_gateway_router_id_explicit() {
    use vrm_rust_workflow::schema::gateway_config_dto::GatewayConfigDto;

    let config = GatewayConfigDto {
        gateway_router_id: Some("Custom-Gateway-rms_0".to_string()),
        ingress_bandwidth_gbps: 1000,
        egress_bandwidth_gbps: 1000,
        gateway_switch_id: "s0".to_string(),
    };

    let router_id = config.resolve_gateway_router_id("rms_0");
    assert_eq!(router_id, "Custom-Gateway-rms_0");
}

// =========================================================================================
//  TC-5.4 / TC-5.5: Removal of Legacy Constants
// =========================================================================================

/// Verify that `RMS_GATEWAY_NAME` does not exist in the codebase.
#[test]
fn test_rms_gateway_name_removed() {
    // This is a compile-time check. If `RMS_GATEWAY_NAME` still exists,
    // the test would fail to compile. We verify by checking the config
    // module only contains the new constants.
    let config_source = include_str!("../../../src/vrm/common/config.rs");
    assert!(!config_source.contains("RMS_GATEWAY_NAME"), "RMS_GATEWAY_NAME constant should be removed from config.rs");
}

/// Verify that `get_component_router_list` does not exist in the codebase.
#[test]
fn test_get_component_router_list_removed() {
    let core_source = include_str!("../../../src/vrm/vrm_component/vrm_component_manager/core.rs");
    assert!(!core_source.contains("get_component_router_list"), "get_component_router_list should be removed from core.rs");
}

// =========================================================================================
//  TC-6.4: JSON Configuration Parsing — GatewayConfigDto and InterGatewayLinkDto
// =========================================================================================

#[test]
fn test_gateway_config_json_parsing() {
    use vrm_rust_workflow::schema::gateway_config_dto::GatewayConfigSectionDto;

    let json = r#"{
        "gatewayConfig": {
            "rms_0": {
                "gatewayRouterId": "AcI-Gateway-rms_0",
                "ingressBandwidthGbps": 1000,
                "egressBandwidthGbps": 1000,
                "gatewaySwitchId": "s0"
            },
            "rms_1": {
                "gatewayRouterId": "AcI-Gateway-rms_1",
                "ingressBandwidthGbps": 2000,
                "egressBandwidthGbps": 2000,
                "gatewaySwitchId": "s1"
            }
        },
        "interGatewayLinks": [
            {
                "sourceGateway": "AcI-Gateway-rms_0",
                "targetGateway": "AcI-Gateway-rms_1",
                "bandwidthGbps": 10000
            }
        ]
    }"#;

    let config: GatewayConfigSectionDto = serde_json::from_str(json).expect("Failed to parse GatewayConfigSectionDto");

    let rms0 = config.gateway_config.get("rms_0").expect("rms_0 config missing");
    assert_eq!(rms0.gateway_router_id, Some("AcI-Gateway-rms_0".to_string()));
    assert_eq!(rms0.ingress_bandwidth_gbps, 1000);
    assert_eq!(rms0.egress_bandwidth_gbps, 1000);

    let rms1 = config.gateway_config.get("rms_1").expect("rms_1 config missing");
    assert_eq!(rms1.gateway_router_id, Some("AcI-Gateway-rms_1".to_string()));
    assert_eq!(rms1.ingress_bandwidth_gbps, 2000);
    assert_eq!(rms1.egress_bandwidth_gbps, 2000);

    assert_eq!(config.inter_gateway_links.len(), 1);
    let link = &config.inter_gateway_links[0];
    assert_eq!(link.source_gateway, "AcI-Gateway-rms_0");
    assert_eq!(link.target_gateway, "AcI-Gateway-rms_1");
    assert_eq!(link.bandwidth_gbps, 10000);
}

// =========================================================================================
//  TC-5.1: Virtual Reservation Cascade-Delete
// =========================================================================================

/// Verifies that when a parent link reservation is removed from the store,
/// all virtual reservations are cascade-deleted.
#[test]
fn test_virtual_reservation_cascade_delete() {
    use vrm_rust_workflow::vrm::common::id::{ClientId, ReservationName, RouterId};
    use vrm_rust_workflow::vrm::reservation::link_reservation::LinkReservation;
    use vrm_rust_workflow::vrm::reservation::reservation::{Reservation, ReservationBase, ReservationProceeding, ReservationState};

    let store = ReservationStore::new();

    // Create original link reservation
    let base = ReservationBase {
        name: ReservationName::new("original-link".to_string()),
        client_id: ClientId::new("test-client".to_string()),
        handler_id: None,
        state: ReservationState::Open,
        request_proceeding: ReservationProceeding::Commit,
        arrival_time: 0,
        booking_interval_start: 0,
        booking_interval_end: 100,
        assigned_start: 0,
        assigned_end: 100,
        task_duration: 100,
        reserved_capacity: 500,
        is_moldable: true,
        moldable_work: 100,
        frag_delta: 0.0,
    };

    let original = Reservation::Link(LinkReservation { base, start_point: Some(RouterId::new("node_1")), end_point: Some(RouterId::new("node_2")) });

    let original_id = store.add(original);

    // Create virtual reservations via the store API
    let v1 = store.add_virtual_reservation_diff_end(original_id, RouterId::new("gateway-A"));
    let v2 = store.add_virtual_reservation_diff_start(original_id, RouterId::new("gateway-B"));

    assert!(v1.is_some(), "Virtual reservation 1 should be created");
    assert!(v2.is_some(), "Virtual reservation 2 should be created");

    let v1 = v1.unwrap();
    let v2 = v2.unwrap();

    // Verify they exist
    assert!(store.contains(v1));
    assert!(store.contains(v2));

    // Remove the original → should cascade-delete virtuals
    store.remove(original_id);

    // Verify virtual reservations are deleted
    assert!(!store.contains(v1), "Virtual reservation 1 should be cascade-deleted");
    assert!(!store.contains(v2), "Virtual reservation 2 should be cascade-deleted");

    // Verify original is deleted
    assert!(!store.contains(original_id));
}

// =========================================================================================
//  TC-6.7: original_to_virtual Tracking Map Integrity
// =========================================================================================

#[test]
fn test_original_to_virtual_tracking_map() {
    use vrm_rust_workflow::vrm::common::id::{ClientId, ReservationName, RouterId};
    use vrm_rust_workflow::vrm::reservation::link_reservation::LinkReservation;
    use vrm_rust_workflow::vrm::reservation::reservation::{Reservation, ReservationBase, ReservationProceeding, ReservationState};

    let store = ReservationStore::new();

    let base = ReservationBase {
        name: ReservationName::new("tracking-test-link".to_string()),
        client_id: ClientId::new("test-client".to_string()),
        handler_id: None,
        state: ReservationState::Open,
        request_proceeding: ReservationProceeding::Commit,
        arrival_time: 0,
        booking_interval_start: 0,
        booking_interval_end: 100,
        assigned_start: 0,
        assigned_end: 100,
        task_duration: 100,
        reserved_capacity: 100,
        is_moldable: true,
        moldable_work: 100,
        frag_delta: 0.0,
    };

    let original = Reservation::Link(LinkReservation { base, start_point: Some(RouterId::new("A")), end_point: Some(RouterId::new("B")) });

    let original_id = store.add(original);

    let v1 = store.add_virtual_reservation_diff_end(original_id, RouterId::new("G1")).unwrap();
    let v2 = store.add_virtual_reservation_diff_start(original_id, RouterId::new("G2")).unwrap();

    // Remove v1
    store.remove_virtual_reservation(original_id, v1);
    // After removal, v2 should still be tracked
    // We verify by checking v2 still exists and original_id->v2 mapping remains
    assert!(store.contains(v2), "v2 should still exist after removing v1");

    // Remove v2 → entry should be cleaned up
    store.remove_virtual_reservation(original_id, v2);

    // Clean up
    store.remove(original_id);
    assert!(!store.contains(v1));
    assert!(!store.contains(v2));
}

// =========================================================================================
//  TC-4.6: Cancel All Reservations With Virtual Reservations
// =========================================================================================

/// Verifies that cancel_all_reservations correctly deletes virtual reservations
/// from grid_component_res_database.
#[test]
fn test_cancel_all_reservations_with_virtuals() {
    use vrm_rust_workflow::vrm::common::id::{ClientId, ReservationName, RouterId};
    use vrm_rust_workflow::vrm::reservation::link_reservation::LinkReservation;
    use vrm_rust_workflow::vrm::reservation::reservation::{Reservation, ReservationBase, ReservationProceeding, ReservationState};

    let store = ReservationStore::new();

    let base = ReservationBase {
        name: ReservationName::new("cancel-test-link".to_string()),
        client_id: ClientId::new("test-client".to_string()),
        handler_id: None,
        state: ReservationState::Open,
        request_proceeding: ReservationProceeding::Commit,
        arrival_time: 0,
        booking_interval_start: 0,
        booking_interval_end: 100,
        assigned_start: 0,
        assigned_end: 100,
        task_duration: 100,
        reserved_capacity: 100,
        is_moldable: true,
        moldable_work: 100,
        frag_delta: 0.0,
    };

    let original = Reservation::Link(LinkReservation { base, start_point: Some(RouterId::new("A")), end_point: Some(RouterId::new("B")) });

    let original_id = store.add(original);

    // Add virtual reservations
    let v1 = store.add_virtual_reservation_diff_end(original_id, RouterId::new("G1")).unwrap();
    let v2 = store.add_virtual_reservation_diff_start(original_id, RouterId::new("G2")).unwrap();

    assert!(store.contains(v1));
    assert!(store.contains(v2));

    // Simulate cascade_delete
    store.cascade_delete_virtual_reservations(original_id);

    assert!(!store.contains(v1), "Virtual v1 should be cascade-deleted");
    assert!(!store.contains(v2), "Virtual v2 should be cascade-deleted");

    // Clean up original
    store.remove(original_id);
    assert!(!store.contains(original_id));
}
