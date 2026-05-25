use std::collections::HashSet;
use std::sync::Arc;

use vrm_rust_workflow::api::rms_config_dto::rms_dto::{DummyRmsDto, GridNodeDto, NetworkLinkDto, RmsSystemWrapper};
use vrm_rust_workflow::api::vrm_system_model_dto::aci_dto::AcIDto;
use vrm_rust_workflow::api::vrm_system_model_dto::adc_dto::ADCDto;
use vrm_rust_workflow::api::vrm_system_model_dto::vrm_dto::VrmDto;
use vrm_rust_workflow::api::workflow_dto::client_dto::{ClientDto, ClientsDto};
use vrm_rust_workflow::api::workflow_dto::dependency_dto::DependencyDto;
use vrm_rust_workflow::api::workflow_dto::reservation_dto::{
    DataInDto, DataOutDto, LinkReservationDto, NodeReservationDto, ReservationProceedingDto, ReservationStateDto,
};
use vrm_rust_workflow::api::workflow_dto::workflow_dto::{TaskDto, WorkflowDto};
use vrm_rust_workflow::domain::simulator::simulator::{GlobalClock, GlobalClockDto};
use vrm_rust_workflow::domain::vrm_system_model::client::client::Clients;
use vrm_rust_workflow::domain::vrm_system_model::grid_resource_management_system::aci::AcI;
use vrm_rust_workflow::domain::vrm_system_model::grid_resource_management_system::vrm_component_registry::registry_client::RegistryClient;
use vrm_rust_workflow::domain::vrm_system_model::reservation::node_reservation::NodeReservation;
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation::{Reservation, ReservationBase, ReservationProceeding, ReservationState};
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use vrm_rust_workflow::domain::vrm_system_model::utils::id::{ClientId, ReservationName};
use vrm_rust_workflow::domain::vrm_system_model::vrm_manager::VrmManager;
use vrm_rust_workflow::domain::vrm_system_model::{client, workflow};

pub fn create_node_reservation(
    res_name: ReservationName,
    capacity: i64,
    start: i64,
    end: i64,
    reservation_state: ReservationState,
    clock: Arc<GlobalClock>,
) -> Reservation {
    let client_id = ClientId::new("test_client".to_string());
    let duration = end - start;

    let base = ReservationBase {
        name: res_name.clone(),
        client_id,
        handler_id: None,
        state: reservation_state,
        request_proceeding: ReservationProceeding::Commit,
        arrival_time: clock.get_system_time_s(),
        booking_interval_start: start,
        booking_interval_end: end,
        assigned_start: start,
        assigned_end: end,
        task_duration: duration,
        reserved_capacity: capacity,
        is_moldable: false,
        moldable_work: duration,
        frag_delta: 0.0,
    };

    let node_res = NodeReservation {
        base,
        data_dependencies: HashSet::new(),
        current_working_directory: Some("/tmp".to_string()),
        environment: Some(vec!["PATH=/usr/bin:/bin".to_string()]),
        task_path: "/bin/sleep".to_string(),
        output_path: Some("/tmp/slurm_test.out".to_string()),
        error_path: Some("/tmp/slurm_test.err".to_string()),
    };

    return Reservation::Node(node_res);
}

pub async fn create_dummy_aci(clock: Arc<GlobalClock>, reservation_store: ReservationStore) -> AcI {
    let dto = get_aci_dto("ADC-001".to_string());
    return AcI::from_dto(dto, clock, reservation_store).await.expect("Error in the AcI Mock process happened.");
}

pub fn get_aci_dto(connected_to_adc: String) -> AcIDto {
    let grid_nodes = vec![
        GridNodeDto { id: "Node-001".to_string(), cpus: 256, connected_to_router: vec!["Router-001".to_string()] },
        GridNodeDto { id: "Node-002".to_string(), cpus: 256, connected_to_router: vec!["Router-002".to_string()] },
        GridNodeDto { id: "Node-003".to_string(), cpus: 256, connected_to_router: vec!["Router-003".to_string()] },
        GridNodeDto { id: "Node-004".to_string(), cpus: 256, connected_to_router: vec!["Router-001".to_string(), "Router-003".to_string()] },
    ];

    let network_links = vec![
        NetworkLinkDto {
            id: "Router-001--To--Router-002".to_string(),
            start_point: "Router-001".to_string(),
            end_point: "Router-002".to_string(),
            capacity: 10000,
        },
        NetworkLinkDto {
            id: "Router-001--To--Router-003".to_string(),
            start_point: "Router-001".to_string(),
            end_point: "Router-003".to_string(),
            capacity: 10000,
        },
        NetworkLinkDto {
            id: "Router-002--To--Router-001".to_string(),
            start_point: "Router-002".to_string(),
            end_point: "Router-001".to_string(),
            capacity: 5000,
        },
        NetworkLinkDto {
            id: "Router-002--To--Router-003".to_string(),
            start_point: "Router-002".to_string(),
            end_point: "Router-003".to_string(),
            capacity: 5000,
        },
    ];

    let dummy_rms_dto = DummyRmsDto {
        typ: "RmsNodeSimulator".to_string(),
        scheduler_typ: "SlottedSchedule".to_string(),
        num_of_slots: 10,
        slot_width: 60,
        grid_nodes,
        network_links,
    };

    let rms_system = RmsSystemWrapper::DummyRms(dummy_rms_dto);

    return AcIDto { adc_id: connected_to_adc, commit_timeout: 256, id: "AcI-001".to_string(), rms_system: rms_system };
}

pub fn get_adc_dto(adc_master_id: String, children: Vec<String>) -> ADCDto {
    return ADCDto {
        id: adc_master_id,
        scheduler_typ: "HEFT".to_string(),
        request_order: "Start-First".to_string(),
        num_of_slots: 60,
        slot_width: 60,
        timeout: 60,
        max_optimization_time: 60,
        reject_new_reservations_at: 60,
        children: children,
    };
}

pub fn get_direct_mapping_workflow_dto(
    workflow_id: String,
    workflow_proceeding: ReservationProceedingDto,
    workflow_state: ReservationStateDto,
) -> WorkflowDto {
    let cwd = Some("/tmp".to_string());
    let task_path = "#!/bin/bash\nsleep 10\nexit 0".to_string();
    let environment = Some(vec!["PATH=/usr/local/bin:/usr/bin:/bin".to_string()]);
    let output_path = Some("/data/logs/sim.out".to_string());
    let error_path = Some("/data/logs/sim.err".to_string());

    return WorkflowDto {
        id: workflow_id,
        arrival_time: 0,
        booking_interval_start: 10,
        booking_interval_end: 1000000,
        request_proceeding: workflow_proceeding,
        reservation_state: workflow_state,

        tasks: vec![
            // Task c0
            TaskDto {
                id: "c0".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                node_reservation: NodeReservationDto {
                    task_path: task_path.clone(),
                    output_path: output_path.clone(),
                    error_path: error_path.clone(),
                    current_working_directory: cwd.clone(),
                    environment: environment.clone(),
                    duration: 50,
                    is_moldable: false,
                    cpus: 2,
                    dependencies: DependencyDto { data: vec![], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "preprocessed_data".to_string(),
                        file: Some("preprocessed.h5".to_string()),
                        size: Some(50),
                        bandwidth: Some(10),
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "raw_data".to_string(),
                        file: Some("raw_detector_data.bin".to_string()),
                    }],
                },
                link_reservation: vec![
                    LinkReservationDto { start_point: "c0".to_string(), end_point: "c1".to_string(), amount: Some(50), bandwidth: Some(10) },
                    LinkReservationDto { start_point: "c0".to_string(), end_point: "c2".to_string(), amount: Some(50), bandwidth: Some(10) },
                ],
            },
            // Task c1
            TaskDto {
                id: "c1".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                node_reservation: NodeReservationDto {
                    task_path: task_path.clone(),
                    output_path: output_path.clone(),
                    error_path: error_path.clone(),
                    current_working_directory: cwd.clone(),
                    environment: environment.clone(),
                    duration: 50,
                    is_moldable: false,
                    cpus: 2,
                    dependencies: DependencyDto { data: vec!["c0".to_string()], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "preprocessed_data".to_string(),
                        file: Some("preprocessed.h5".to_string()),
                        size: Some(50),
                        bandwidth: Some(10),
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "raw_data".to_string(),
                        file: Some("raw_detector_data.bin".to_string()),
                    }],
                },
                link_reservation: vec![LinkReservationDto {
                    start_point: "c1".to_string(),
                    end_point: "c3".to_string(),
                    amount: Some(50),
                    bandwidth: Some(10),
                }],
            },
            // Task c2
            TaskDto {
                id: "c2".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                node_reservation: NodeReservationDto {
                    task_path: task_path.clone(),
                    output_path: output_path.clone(),
                    error_path: error_path.clone(),
                    current_working_directory: cwd.clone(),
                    environment: environment.clone(),
                    duration: 50,
                    is_moldable: false,
                    cpus: 2,
                    dependencies: DependencyDto { data: vec!["c0".to_string()], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "preprocessed_data".to_string(),
                        file: Some("preprocessed.h5".to_string()),
                        size: Some(50),
                        bandwidth: Some(10),
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "raw_data".to_string(),
                        file: Some("raw_detector_data.bin".to_string()),
                    }],
                },
                link_reservation: vec![LinkReservationDto {
                    start_point: "c2".to_string(),
                    end_point: "c3".to_string(),
                    amount: Some(50),
                    bandwidth: Some(10),
                }],
            },
            // Task c3
            TaskDto {
                id: "c3".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                node_reservation: NodeReservationDto {
                    task_path: task_path.clone(),
                    output_path: output_path,
                    error_path: error_path,
                    current_working_directory: cwd.clone(),
                    environment: environment.clone(),
                    duration: 50,
                    is_moldable: false,
                    cpus: 2,
                    dependencies: DependencyDto { data: vec!["c1".to_string(), "c2".to_string()], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "preprocessed_data".to_string(),
                        file: Some("preprocessed.h5".to_string()),
                        size: Some(50),
                        bandwidth: Some(10),
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "raw_data".to_string(),
                        file: Some("raw_detector_data.bin".to_string()),
                    }],
                },
                link_reservation: vec![],
            },
        ],
    };
}

pub fn get_workflow_dto_with_one_task(
    workflow_id: String,
    task_reservation_state: ReservationStateDto,
    task_reservation_proceeding: ReservationProceedingDto,
) -> WorkflowDto {
    let cwd = Some("/tmp".to_string());
    let task_path = "#!/bin/bash\nsleep 10\nexit 0".to_string();
    let environment = Some(vec!["PATH=/usr/local/bin:/usr/bin:/bin".to_string()]);
    let output_path = Some("/data/logs/sim.out".to_string());
    let error_path = Some("/data/logs/sim.err".to_string());

    return WorkflowDto {
        id: workflow_id,
        arrival_time: 0,
        booking_interval_start: 10,
        booking_interval_end: 100,
        reservation_state: task_reservation_state,
        request_proceeding: task_reservation_proceeding,

        tasks: vec![
            // Task c0
            TaskDto {
                id: "c0".to_string(),
                reservation_state: task_reservation_state,
                request_proceeding: task_reservation_proceeding,
                node_reservation: NodeReservationDto {
                    task_path: task_path.clone(),
                    output_path: output_path.clone(),
                    error_path: error_path.clone(),
                    current_working_directory: cwd.clone(),
                    environment: environment.clone(),
                    duration: 50,
                    is_moldable: false,
                    cpus: 2,
                    dependencies: DependencyDto { data: vec![], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "preprocessed_data".to_string(),
                        file: Some("preprocessed.h5".to_string()),
                        size: Some(50),
                        bandwidth: Some(10),
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "raw_data".to_string(),
                        file: Some("raw_detector_data.bin".to_string()),
                    }],
                },
                link_reservation: vec![
                    LinkReservationDto { start_point: "c0".to_string(), end_point: "c1".to_string(), amount: Some(50), bandwidth: Some(10) },
                    LinkReservationDto { start_point: "c0".to_string(), end_point: "c2".to_string(), amount: Some(50), bandwidth: Some(10) },
                ],
            },
        ],
    };
}

pub fn get_clients(client_id: String, workflow_dto: Vec<WorkflowDto>, reservation_store: ReservationStore) -> Clients {
    let client_dto = ClientDto { id: client_id, workflows: workflow_dto };
    let clients_dto = ClientsDto { clients: vec![client_dto] };
    return Clients::from_dto(clients_dto, reservation_store).expect("Getting Clients was not possible.");
}
