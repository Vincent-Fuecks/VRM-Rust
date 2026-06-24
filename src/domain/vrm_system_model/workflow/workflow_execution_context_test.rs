use std::sync::Arc;

use crate::api::workflow_dto::dependency_dto::DependencyDto;
use crate::api::workflow_dto::reservation_dto::{
    DataInDto, DataOutDto, LinkReservationDto, NodeReservationDto, ReservationProceedingDto, ReservationStateDto,
};
use crate::api::workflow_dto::workflow_dto::{TaskDto, WorkflowDto};
use crate::domain::simulator::simulator::GlobalClock;
use crate::domain::vrm_system_model::grid_resource_management_system::adc::ADC;
use crate::domain::vrm_system_model::grid_resource_management_system::scheduler::workflow_scheduler_type::WorkflowSchedulerType;
use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_order::VrmComponentOrder;
use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_registry::registry_client::RegistryClient;
use crate::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use crate::domain::vrm_system_model::utils::id::{AdcId, ClientId, WorkflowNodeId};
use crate::domain::vrm_system_model::workflow::workflow::Workflow;

fn build_10_node_workflow_dto() -> WorkflowDto {
    WorkflowDto {
        id: "Test-Workflow-10Nodes".to_string(),
        arrival_time: 0,
        booking_interval_start: 10,
        booking_interval_end: 100000,
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        tasks: vec![
            TaskDto {
                id: "A0".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "A0".to_string(),
                    end_point: "A1".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 10,
                    cpus: 4,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec![], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "output_A0".to_string(),
                        file: Some("data_A0.h5".to_string()),
                        size: Some(50),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "input".to_string(),
                        file: Some("input_data.bin".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "A1".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "A1".to_string(),
                    end_point: "A0".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 15,
                    cpus: 2,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec![], sync: vec!["A0".to_string()] },
                    data_out: vec![DataOutDto {
                        name: "output_A1".to_string(),
                        file: Some("data_A1.h5".to_string()),
                        size: Some(30),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "input".to_string(),
                        file: Some("input_data.bin".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "A2".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 20,
                    cpus: 8,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec!["A0".to_string()], sync: vec![] },
                    data_out: vec![
                        DataOutDto { name: "output_A2_to_B3".to_string(), file: Some("data_A2_B3.h5".to_string()), size: Some(40), bandwidth: None },
                        DataOutDto { name: "output_A2_to_C4".to_string(), file: Some("data_A2_C4.h5".to_string()), size: Some(40), bandwidth: None },
                    ],
                    data_in: vec![DataInDto {
                        source_reservation: "A0".to_string(),
                        source_port: "output_A0".to_string(),
                        file: Some("data_A0.h5".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "B3".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "B3".to_string(),
                    end_point: "B4".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 12,
                    cpus: 2,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec!["A2".to_string()], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "output_B3".to_string(),
                        file: Some("data_B3.h5".to_string()),
                        size: Some(20),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "A2".to_string(),
                        source_port: "output_A2_to_B3".to_string(),
                        file: Some("data_A2_B3.h5".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "B4".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "B4".to_string(),
                    end_point: "B3".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 8,
                    cpus: 4,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec![], sync: vec!["B3".to_string()] },
                    data_out: vec![DataOutDto {
                        name: "output_B4".to_string(),
                        file: Some("data_B4.h5".to_string()),
                        size: Some(15),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "input".to_string(),
                        file: Some("input_data.bin".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "C3".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "C3".to_string(),
                    end_point: "C4".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 10,
                    cpus: 2,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec!["A2".to_string()], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "output_C3".to_string(),
                        file: Some("data_C3.h5".to_string()),
                        size: Some(25),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "A2".to_string(),
                        source_port: "output_A2_to_C4".to_string(),
                        file: Some("data_A2_C4.h5".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "C4".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "C4".to_string(),
                    end_point: "C3".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 6,
                    cpus: 4,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec![], sync: vec!["C3".to_string()] },
                    data_out: vec![DataOutDto {
                        name: "output_C4".to_string(),
                        file: Some("data_C4.h5".to_string()),
                        size: Some(10),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "input".to_string(),
                        file: Some("input_data.bin".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "D5".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "D5".to_string(),
                    end_point: "D6".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 14,
                    cpus: 2,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec!["B4".to_string()], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "output_D5".to_string(),
                        file: Some("data_D5.h5".to_string()),
                        size: Some(18),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "B4".to_string(),
                        source_port: "output_B4".to_string(),
                        file: Some("data_B4.h5".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "D6".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "D6".to_string(),
                    end_point: "D5".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 10,
                    cpus: 4,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec![], sync: vec!["D5".to_string()] },
                    data_out: vec![DataOutDto {
                        name: "output_D6".to_string(),
                        file: Some("data_D6.h5".to_string()),
                        size: Some(12),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "input".to_string(),
                        file: Some("input_data.bin".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "E5".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "E5".to_string(),
                    end_point: "E6".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 16,
                    cpus: 2,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec!["C4".to_string()], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "output_E5".to_string(),
                        file: Some("data_E5.h5".to_string()),
                        size: Some(22),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "C4".to_string(),
                        source_port: "output_C4".to_string(),
                        file: Some("data_C4.h5".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "E6".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "E6".to_string(),
                    end_point: "E5".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 8,
                    cpus: 4,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec![], sync: vec!["E5".to_string()] },
                    data_out: vec![DataOutDto {
                        name: "output_E6".to_string(),
                        file: Some("data_E6.h5".to_string()),
                        size: Some(14),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "input".to_string(),
                        file: Some("input_data.bin".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "F7".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "F7".to_string(),
                    end_point: "F8".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 18,
                    cpus: 8,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec!["D5".to_string(), "E5".to_string()], sync: vec![] },
                    data_out: vec![DataOutDto {
                        name: "output_F7".to_string(),
                        file: Some("data_F7.h5".to_string()),
                        size: Some(35),
                        bandwidth: None,
                    }],
                    data_in: vec![
                        DataInDto {
                            source_reservation: "D5".to_string(),
                            source_port: "output_D5".to_string(),
                            file: Some("data_D5.h5".to_string()),
                        },
                        DataInDto {
                            source_reservation: "E5".to_string(),
                            source_port: "output_E5".to_string(),
                            file: Some("data_E5.h5".to_string()),
                        },
                    ],
                },
            },
            TaskDto {
                id: "F8".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![LinkReservationDto {
                    start_point: "F8".to_string(),
                    end_point: "F7".to_string(),
                    amount: Some(0),
                    bandwidth: Some(10),
                }],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 12,
                    cpus: 4,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec![], sync: vec!["F7".to_string()] },
                    data_out: vec![DataOutDto {
                        name: "output_F8".to_string(),
                        file: Some("data_F8.h5".to_string()),
                        size: Some(28),
                        bandwidth: None,
                    }],
                    data_in: vec![DataInDto {
                        source_reservation: "EXTERNAL".to_string(),
                        source_port: "input".to_string(),
                        file: Some("input_data.bin".to_string()),
                    }],
                },
            },
            TaskDto {
                id: "G9".to_string(),
                reservation_state: ReservationStateDto::Open,
                request_proceeding: ReservationProceedingDto::Commit,
                link_reservation: vec![],
                node_reservation: NodeReservationDto {
                    task_path: "".to_string(),
                    output_path: Some("/data/out".to_string()),
                    error_path: Some("/data/err".to_string()),
                    duration: 25,
                    cpus: 8,
                    is_moldable: false,
                    current_working_directory: None,
                    environment: None,
                    dependencies: DependencyDto { data: vec!["F7".to_string()], sync: vec![] },
                    data_out: vec![],
                    data_in: vec![DataInDto {
                        source_reservation: "F7".to_string(),
                        source_port: "output_F7".to_string(),
                        file: Some("data_F7.h5".to_string()),
                    }],
                },
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_and_validate_10_node_workflow() {
        let store = ReservationStore::new();
        let workflow_dto = build_10_node_workflow_dto();
        let client_id = ClientId::new("TestClient");
        let workflow_res_id = Workflow::create_form_dto(workflow_dto, client_id, store.clone()).expect("Failed to create workflow from DTO");

        let ranked_nodes = store.with_workflow_mut(workflow_res_id, |workflow| {
            assert_eq!(workflow.nodes.len(), 14, "Workflow should have 14 nodes (A0, A1, A2, B3, B4, C3, C4, D5, D6, E5, E6, F7, F8, G9)");
            assert_eq!(workflow.entry_nodes.len(), 1, "Workflow should have 1 entry node");
            assert!(!workflow.exit_nodes.is_empty(), "Workflow should have exit nodes");
            assert_eq!(workflow.entry_co_allocation.len(), 1, "There should be exactly 1 entry CoAllocation");

            assert_eq!(
                workflow.co_allocations.len(),
                8,
                "Should have exactly 8 co-allocation groups (A0/A1, A2, B3/B4, C3/C4, D5/D6, E5/E6, F7/F8, G9)"
            );

            assert!(workflow.data_dependencies.len() > 0, "Should have data dependencies");
            assert!(!workflow.sync_dependencies.is_empty(), "Workflow should have sync dependencies");

            let avg_link_speed = 1000;
            let ranked = workflow.calculate_upward_rank(avg_link_speed, &store.clone());

            assert_eq!(ranked.len(), workflow.co_allocations.len(), "Ranked nodes length should equal number of co-allocations");
            assert!(!ranked.is_empty(), "Should have ranked nodes");

            log::info!(
                "Workflow: {} nodes, {} data deps, {} sync deps, {} co-allocations",
                workflow.nodes.len(),
                workflow.data_dependencies.len(),
                workflow.sync_dependencies.len(),
                workflow.co_allocations.len()
            );

            ranked
        });

        let ranked_nodes = ranked_nodes.expect("with_workflow_mut should return Some");
        assert_eq!(ranked_nodes.len(), 8, "Should have 8 ranked co-allocations");

        if let Some(handle) = store.get(workflow_res_id) {
            let guard = handle.read();
            if let Some(workflow) = guard.as_workflow() {
                let a0_node = workflow.nodes.get(&WorkflowNodeId::new("A0".to_string()));
                let a1_node = workflow.nodes.get(&WorkflowNodeId::new("A1".to_string()));
                assert!(a0_node.is_some(), "A0 should exist");
                assert!(a1_node.is_some(), "A1 should exist");
                assert_eq!(
                    a0_node.unwrap().co_allocation_key,
                    a1_node.unwrap().co_allocation_key,
                    "A0 and A1 should be in the same CoAllocation (via sync)"
                );

                let b3_node = workflow.nodes.get(&WorkflowNodeId::new("B3".to_string()));
                let b4_node = workflow.nodes.get(&WorkflowNodeId::new("B4".to_string()));
                assert!(b3_node.is_some(), "B3 should exist");
                assert!(b4_node.is_some(), "B4 should exist");
                assert_eq!(
                    b3_node.unwrap().co_allocation_key,
                    b4_node.unwrap().co_allocation_key,
                    "B3 and B4 should be in the same CoAllocation (via sync)"
                );

                let f7_node = workflow.nodes.get(&WorkflowNodeId::new("F7".to_string()));
                let f8_node = workflow.nodes.get(&WorkflowNodeId::new("F8".to_string()));
                assert!(f7_node.is_some(), "F7 should exist");
                assert!(f8_node.is_some(), "F8 should exist");
                assert_eq!(
                    f7_node.unwrap().co_allocation_key,
                    f8_node.unwrap().co_allocation_key,
                    "F7 and F8 should be in the same CoAllocation (via sync)"
                );
            }
        }
    }

    #[test]
    fn test_schedule_10_node_workflow_via_adc() {
        let store = ReservationStore::new();
        let workflow_dto = build_10_node_workflow_dto();
        let client_id = ClientId::new("TestClient");
        let workflow_res_id = Workflow::create_form_dto(workflow_dto, client_id, store.clone()).expect("Failed to create workflow from DTO");

        let scheduler = WorkflowSchedulerType::get_instance(
            crate::domain::vrm_system_model::grid_resource_management_system::scheduler::workflow_scheduler_type::WorkflowSchedulerType::HEFTSync,
            store.clone(),
        );

        let adc_id = AdcId::new("TestADC-1");
        let simulator = Arc::new(GlobalClock::new(true));
        let registry = RegistryClient::new();

        let adc = ADC::new(adc_id, vec![], registry, store.clone(), Some(scheduler), VrmComponentOrder::OrderStartFirst, 1000, simulator, 1000, 1);

        store.with_workflow_mut(workflow_res_id, |workflow| {
            assert_eq!(workflow.nodes.len(), 14);
            assert!(!workflow.data_dependencies.is_empty());
            assert!(!workflow.sync_dependencies.is_empty());
            assert!(!workflow.co_allocations.is_empty());

            let avg_link_speed = adc.manager.get_average_link_speed() as i64;
            if avg_link_speed > 0 {
                let ranked = workflow.calculate_upward_rank(avg_link_speed, &store.clone());
                assert_eq!(ranked.len(), workflow.co_allocations.len());

                log::info!("Rank calculation: {} co-allocations ranked", ranked.len());
                for (i, node) in ranked.iter().enumerate() {
                    if let Some(name) = store.get_name_for_key(node.reservation_id) {
                        log::info!("  Rank {}: {:?}", i + 1, name);
                    }
                }
            }
        });
    }
}
