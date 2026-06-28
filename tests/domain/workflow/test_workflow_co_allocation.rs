use vrm_rust_workflow::{
    domain::vrm_system_model::{
        reservation::{
            node_reservation::NodeReservation,
            reservation::{Reservation, ReservationProceeding, ReservationState},
            reservation_store::ReservationStore,
        },
        utils::id::{ClientId, CoAllocationId, DataDependencyId, ReservationName, SyncDependencyId, WorkflowNodeId},
        workflow::workflow::Workflow,
    },
    error::Error,
    generate_system_model,
    schema::{
        reservation_dto::{
            DataInDto, DataOutDto, DependencyDto, LinkReservationDto, NodeReservationDto, ReservationProceedingDto, ReservationStateDto,
        },
        workflow_dto::{TaskDto, WorkflowDto},
    },
};

use std::collections::HashSet;

/// The CoAllocation is formed by any WorkflowNodes that are linked, directly or indirectly, by a SyncDependency.
/// This test case, consists of three SyncDependencies A -> B        B -> C      D -> E
/// => CoAllocation(A,B,C) and CoAllocation(D,E)
/// Plus a DataDependency between the two CoAllocations (CoAllocation(A,B,C) and CoAllocation(D,E))
#[test]
fn test_co_allocation_graph_creation() {
    let file_path: &str = "src/data/test/test_workflow_with_simple_co_allocation_graph.json";
    let store = ReservationStore::new();
    let vrm = generate_system_model(file_path, store.clone());

    if let Ok(system_model) = vrm {
        let clients_map = system_model.unprocessed_reservations;

        assert_eq!(clients_map.len(), 1);
        let client_id = ClientId::new("7209cffb-259f-404b-ac91-4795b4ad39e7");
        assert!(clients_map.iter().any(|res_id| store.get_client_id(*res_id) == client_id));

        // Retrieve Workflow by searching Client's reservations
        // This avoids potential panics from get_key_for_name if the exact name key is missing or formatted differently
        let client_res_ids = store.get_client_reservations(&client_id);

        let workflow_rid = client_res_ids
            .iter()
            .find(|&&rid| {
                if let Some(res_lock) = store.get(rid) {
                    let res = res_lock.read();
                    if let Reservation::Workflow(w) = &*res {
                        // Check if this is the workflow we are looking for
                        return w.base.name.to_string() == "Simulation-Run-0";
                    }
                }
                false
            })
            .cloned()
            .expect("Workflow 'Simulation-Run-0' not found in client reservations");

        let workflow_lock = store.get(workflow_rid).expect("Workflow not found in store");
        let workflow_guard = workflow_lock.read();
        let workflow = match &*workflow_guard {
            Reservation::Workflow(w) => w,
            _ => panic!("Expected Workflow reservation"),
        };

        // Test CoAllocation Graph is correctly constructed
        // A, B, C should be in one group. D, E in another.
        // Note: The Key of the CoAllocation is the ID of its Representative Node.
        // We need to find which node is the representative for {A,B,C} and {D,E}.

        // Helper to check if a CoAllocation contains specific members
        let check_group_members = |members: &[&str], co_alloc_id: &CoAllocationId| {
            let co_alloc = workflow.co_allocations.get(co_alloc_id).unwrap();
            let member_set: HashSet<String> = co_alloc.members.iter().map(|id| id.to_string()).collect();
            for m in members {
                assert!(member_set.contains(*m), "CoAllocation {:?} missing member {}", co_alloc_id, m);
            }
        };

        let mut sorted_co_allocations: Vec<_> = workflow.co_allocations.values().collect();
        // Sort by ID to ensure deterministic testing
        sorted_co_allocations.sort_by_key(|c| c.id.clone());

        // We expect 2 co-allocations
        assert_eq!(sorted_co_allocations.len(), 2);

        // Identify which is which based on size or member content
        let (group_abc, group_de) = if sorted_co_allocations[0].members.len() == 3 {
            (sorted_co_allocations[0], sorted_co_allocations[1])
        } else {
            (sorted_co_allocations[1], sorted_co_allocations[0])
        };

        check_group_members(&["A", "B", "C"], &group_abc.id);
        check_group_members(&["D", "E"], &group_de.id);

        // Test CoAllocation Dependencies (Graph Edges)
        // We expect a link from ABC -> DE
        let has_dependency =
            workflow.co_allocation_dependencies.values().any(|dep| dep.source_group == group_abc.id && dep.target_group == group_de.id);
        assert!(has_dependency, "Missing CoAllocation Dependency from ABC -> DE");

        // Test Entry/Exit
        assert!(workflow.entry_co_allocation.contains(&group_abc.id));
        assert!(workflow.exit_co_allocation.contains(&group_de.id));
    }
}

#[test]
fn test_workflow_node_creation_for_system_model() {
    let file_path: &str = "src/data/test/test_workflow_loading_01.json";
    let store = ReservationStore::new();
    let result = generate_system_model(file_path, store.clone());

    if let Ok(system_model) = result {
        let clients_map = system_model.unprocessed_reservations;
        assert_eq!(clients_map.len(), 1);

        let client_id = ClientId::new("7209cffb-259f-404b-ac91-4795b4ad39e7");
        assert!(clients_map.iter().any(|res_id| store.get_client_id(*res_id) == client_id));

        // Retrieve Workflow via Client Reservations
        let client_res_ids = store.get_client_reservations(&client_id);

        let workflow_rid = client_res_ids
            .iter()
            .find(|&&rid| {
                if let Some(res_lock) = store.get(rid) {
                    let res = res_lock.read();
                    if let Reservation::Workflow(w) = &*res {
                        return w.base.name.to_string() == "Simulation-Run-0";
                    }
                }
                false
            })
            .cloned()
            .expect("Workflow 'Simulation-Run-0' not found in client reservations");

        let workflow_lock = store.get(workflow_rid).unwrap();
        let workflow_guard = workflow_lock.read();
        let workflow = match &*workflow_guard {
            Reservation::Workflow(w) => w,
            _ => panic!("Expected Workflow"),
        };

        let node_id = WorkflowNodeId::new("Data-Preprocessing-3");
        assert!(workflow.nodes.contains_key(&node_id));
        let node = workflow.nodes.get(&node_id).unwrap();

        // Access Reservation via Store
        let res_lock = store.get(node.reservation_id).unwrap();
        let res_guard = res_lock.read();
        let reservation = match &*res_guard {
            Reservation::Node(n) => n,
            _ => panic!("Expected Node Reservation"),
        };

        assert_eq!(reservation.base.name, ReservationName::new("Data-Preprocessing-3"));
        assert_eq!(reservation.base.state, ReservationState::Open);
        assert_eq!(reservation.base.request_proceeding, ReservationProceeding::Commit);
        assert_eq!(reservation.base.task_duration, 1800);
        assert_eq!(reservation.base.reserved_capacity, 8);
        assert_eq!(reservation.base.is_moldable, false);

        assert_eq!(reservation.task_path, "".to_string());
        assert_eq!(reservation.output_path, Some("/data/logs/sim.out".to_string()));
        assert_eq!(reservation.error_path, Some("/data/logs/sim.err".to_string()));

        let expected_incoming_data = vec![
            "Simulation-Run-0.data.Data-Preprocessing-2.Data-Preprocessing-3",
            "Simulation-Run-0.data.Data-Preprocessing-1.Data-Preprocessing-3",
        ];
        let mut actual_incoming: Vec<String> = node.incoming_data.iter().map(|id| id.to_string()).collect();
        actual_incoming.sort();
        let mut sorted_expected_inc = expected_incoming_data.clone();
        sorted_expected_inc.sort();
        assert_eq!(actual_incoming, sorted_expected_inc);

        // Outgoing Data
        let expected_outgoing = "Simulation-Run-0.data.Data-Preprocessing-3.Data-Preprocessing-4";
        assert!(node.outgoing_data.contains(&DataDependencyId::new(expected_outgoing)));

        // Incoming Sync
        let expected_incoming_sync = vec![
            "Simulation-Run-0.sync.Data-Preprocessing-2.Data-Preprocessing-3",
            "Simulation-Run-0.sync.Data-Preprocessing-1.Data-Preprocessing-3",
        ];
        let mut actual_inc_sync: Vec<String> = node.incoming_sync.iter().map(|id| id.to_string()).collect();
        actual_inc_sync.sort();
        let mut sorted_exp_sync = expected_incoming_sync.clone();
        sorted_exp_sync.sort();
        assert_eq!(actual_inc_sync, sorted_exp_sync);

        // Outgoing Sync
        let expected_outgoing_sync = "Simulation-Run-0.sync.Data-Preprocessing-3.Data-Preprocessing-4";
        assert!(node.outgoing_sync.contains(&SyncDependencyId::new(expected_outgoing_sync)));
    } else {
        println!("Error: {:?}", result);
        assert!(false, "Error during loading process!");
    }
}

#[test]
fn test_error_file_not_found() {
    let non_existent_file = "non_existent_file.json";
    let store = ReservationStore::new();
    let result = generate_system_model(non_existent_file, store);

    assert!(result.is_err());

    if let Some(err) = result.err() {
        assert!(matches!(err, Error::IoError(_)), "Expected IoError, got {:?}", err);
    } else {
        panic!("Expected an error but got Ok");
    }
}

// =========================================================================================
//  Tests for Specific Stages of Workflow Creation Process
// =========================================================================================

fn create_dummy_workflow_dto() -> (WorkflowDto, ClientId) {
    let client_id = ClientId::new("test-client");
    let mut dto = WorkflowDto {
        id: "wf-1".to_string(),
        arrival_time: 100,
        booking_interval_start: 200,
        booking_interval_end: 1000,
        tasks: vec![],
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
    };

    // Common dummy link reservation (not used for node logic, but required by DTO)
    let dummy_link_res =
        LinkReservationDto { start_point: "RouterA".to_string(), end_point: "RouterB".to_string(), amount: Some(1024), bandwidth: Some(100) };

    // Create 3 tasks: A, B, C
    // A -> B (Data), B -> C (Sync)
    let task_a = TaskDto {
        id: "A".to_string(),
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        link_reservation: vec![dummy_link_res.clone()],
        node_reservation: NodeReservationDto {
            duration: 10,
            cpus: 1,
            is_moldable: false,
            task_path: "/bin/task_a".to_string(),
            output_path: Some("/out/task_a.log".to_string()),
            error_path: Some("/err/task_a.log".to_string()),
            current_working_directory: Some("/err/task_a.log".to_string()),
            environment: Some(vec!["/err/task_a.log".to_string()]),
            data_out: vec![DataOutDto { name: "port1".to_string(), size: Some(100), bandwidth: None, file: Some("output.dat".to_string()) }],
            data_in: vec![], // A is Entry
            dependencies: DependencyDto { data: vec![], sync: vec![] },
        },
    };

    let task_b = TaskDto {
        id: "B".to_string(),
        reservation_state: ReservationStateDto::Open,
        request_proceeding: ReservationProceedingDto::Commit,
        link_reservation: vec![dummy_link_res.clone()],
        node_reservation: NodeReservationDto {
            duration: 15,
            cpus: 2,
            is_moldable: true,
            task_path: "/bin/task_a".to_string(),
            output_path: None,
            error_path: None,
            current_working_directory: Some("/err/task_a.log".to_string()),
            environment: Some(vec!["/err/task_a.log".to_string()]),
            data_out: vec![],
            data_in: vec![DataInDto { source_reservation: "A".to_string(), source_port: "port1".to_string(), file: Some("output.dat".to_string()) }],
            dependencies: DependencyDto { data: vec![], sync: vec![] },
        },
    };

    // Add Sync Out to B manually via DataOutDto with bandwidth
    let mut task_b_mod = task_b.clone();
    task_b_mod.node_reservation.data_out.push(DataOutDto {
        name: "sync_port".to_string(),
        size: None,
        bandwidth: Some(50), // Indicates Sync
        file: None,
    });

    let task_c = TaskDto {
        id: "C".to_string(),
        reservation_state: ReservationStateDto::Committed,
        request_proceeding: ReservationProceedingDto::Reserve,
        link_reservation: vec![dummy_link_res.clone()],
        node_reservation: NodeReservationDto {
            duration: 20,
            cpus: 4,
            is_moldable: false,
            task_path: "/bin/task_c".to_string(),
            output_path: None,
            error_path: None,
            current_working_directory: Some("/err/task_a.log".to_string()),
            environment: Some(vec!["/err/task_a.log".to_string()]),
            data_out: vec![],
            data_in: vec![DataInDto { source_reservation: "B".to_string(), source_port: "sync_port".to_string(), file: None }],
            dependencies: DependencyDto { data: vec![], sync: vec![] },
        },
    };

    dto.tasks.push(task_a);
    dto.tasks.push(task_b_mod);
    dto.tasks.push(task_c);

    (dto, client_id)
}

#[test]
fn test_stage_1_generate_workflow_nodes() {
    let (dto, client_id) = create_dummy_workflow_dto();
    let store = ReservationStore::new();
    let nodes = Workflow::generate_workflow_nodes(&dto, client_id, store.clone());

    assert_eq!(nodes.len(), 3);
    assert!(nodes.contains_key(&WorkflowNodeId::new("A")));
    assert!(nodes.contains_key(&WorkflowNodeId::new("B")));
    assert!(nodes.contains_key(&WorkflowNodeId::new("C")));

    // Helper to get node reservation
    let get_res = |node_id: &str| -> NodeReservation {
        let node = nodes.get(&WorkflowNodeId::new(node_id)).unwrap();
        let lock = store.get(node.reservation_id).unwrap();
        let guard = lock.read();
        match &*guard {
            Reservation::Node(n) => n.clone(),
            _ => panic!("Expected node"),
        }
    };

    // Verify Node A
    let node_a_res = get_res("A");
    assert_eq!(node_a_res.task_path, "/bin/task_a".to_string());

    // Verify Node B
    let node_b_res = get_res("B");
    assert_eq!(node_b_res.base.task_duration, 15);
    assert_eq!(node_b_res.base.is_moldable, true);

    // Verify Node C
    let node_c_res = get_res("C");
    let node_c = nodes.get(&WorkflowNodeId::new("C")).unwrap();
    assert_eq!(node_c_res.base.state, ReservationState::Committed);
    assert_eq!(node_c_res.base.request_proceeding, ReservationProceeding::Reserve);
    assert_eq!(node_c.incoming_sync.len(), 0, "Should be empty before linking");
    assert!(node_c.co_allocation_key.is_none(), "Should be None before Phase 4");
}

#[test]
fn test_stage_2_build_dependencies() {
    let (dto, client_id) = create_dummy_workflow_dto();
    let store = ReservationStore::new();
    let (data_deps, sync_deps) = Workflow::build_all_dependencies(&dto, client_id, store.clone()).expect("Should build deps");

    // A->B is Data, B->C is Sync
    assert_eq!(data_deps.len(), 1);
    assert_eq!(sync_deps.len(), 1);

    // Verify Data Dependency content
    let data_dep = data_deps.values().next().unwrap();
    assert_eq!(data_dep.source_node, Some(WorkflowNodeId::new("A")));
    assert_eq!(data_dep.target_node, Some(WorkflowNodeId::new("B")));
    assert_eq!(data_dep.size, 100);
    assert_eq!(data_dep.port_name, "port1");

    // Verify Sync Dependency content
    let sync_dep = sync_deps.values().next().unwrap();
    assert_eq!(sync_dep.source_node, Some(WorkflowNodeId::new("B")));
    assert_eq!(sync_dep.target_node, Some(WorkflowNodeId::new("C")));
    assert_eq!(sync_dep.bandwidth, 50);
    assert_eq!(sync_dep.port_name, "sync_port");
}

#[test]
fn test_stage_3_populate_adjacency() {
    let (dto, client_id) = create_dummy_workflow_dto();
    let store = ReservationStore::new();

    let mut nodes = Workflow::generate_workflow_nodes(&dto, client_id.clone(), store.clone());
    let (data_deps, sync_deps) = Workflow::build_all_dependencies(&dto, client_id, store.clone()).unwrap();

    Workflow::populate_node_adjacency_lists(&mut nodes, &data_deps, &sync_deps);

    let node_a = nodes.get(&WorkflowNodeId::new("A")).unwrap();
    let node_b = nodes.get(&WorkflowNodeId::new("B")).unwrap();
    let node_c = nodes.get(&WorkflowNodeId::new("C")).unwrap();

    // A has outgoing data
    assert_eq!(node_a.outgoing_data.len(), 1);
    // B has incoming data and outgoing sync
    assert_eq!(node_b.incoming_data.len(), 1);
    assert_eq!(node_b.outgoing_sync.len(), 1);
    // C has incoming sync
    assert_eq!(node_c.incoming_sync.len(), 1);
}

#[test]
fn test_stage_4_co_allocations() {
    let (dto, client_id) = create_dummy_workflow_dto();
    let store = ReservationStore::new();
    let mut nodes = Workflow::generate_workflow_nodes(&dto, client_id.clone(), store.clone());
    let (data_deps, sync_deps) = Workflow::build_all_dependencies(&dto, client_id, store.clone()).unwrap();

    // We must populate adjacency first or CoAllocation building might miss context (though it relies mostly on sync_deps map)
    Workflow::populate_node_adjacency_lists(&mut nodes, &data_deps, &sync_deps);

    let (co_allocs, node_map) = Workflow::build_co_allocations(&nodes, &sync_deps).expect("CoAlloc build failed");

    // B and C are connected via Sync, so they should be in ONE CoAllocation.
    // A is separate.

    let ca_id_a = node_map.get(&WorkflowNodeId::new("A")).unwrap();
    let ca_id_b = node_map.get(&WorkflowNodeId::new("B")).unwrap();
    let ca_id_c = node_map.get(&WorkflowNodeId::new("C")).unwrap();

    assert_eq!(ca_id_b, ca_id_c, "B and C should form a CoAllocation group");
    assert_ne!(ca_id_a, ca_id_b, "A should be in a separate group");

    // Check members of the BC group
    let bc_group = co_allocs.get(ca_id_b).unwrap();
    assert_eq!(bc_group.members.len(), 2);
    assert!(bc_group.members.contains(&WorkflowNodeId::new("B")));
    assert!(bc_group.members.contains(&WorkflowNodeId::new("C")));
}

#[test]
fn test_stage_5_co_allocation_dependencies() {
    let (dto, client_id) = create_dummy_workflow_dto();
    let store = ReservationStore::new();
    let mut nodes = Workflow::generate_workflow_nodes(&dto, client_id.clone(), store.clone());
    let (data_deps, sync_deps) = Workflow::build_all_dependencies(&dto, client_id, store.clone()).unwrap();
    Workflow::populate_node_adjacency_lists(&mut nodes, &data_deps, &sync_deps);
    let (mut co_allocs, node_map) = Workflow::build_co_allocations(&nodes, &sync_deps).unwrap();

    let ca_deps = Workflow::build_co_allocation_dependencies(&data_deps, &node_map, &mut co_allocs).unwrap();

    // A -> B is a Data link.
    // A is Group 1, B is Group 2 (with C).
    // So there should be a CoAllocationDependency from Group 1 -> Group 2.

    assert_eq!(ca_deps.len(), 1);
    let ca_dep = ca_deps.values().next().unwrap();

    let group_a_id = node_map.get(&WorkflowNodeId::new("A")).unwrap();
    let group_bc_id = node_map.get(&WorkflowNodeId::new("B")).unwrap();

    assert_eq!(&ca_dep.source_group, group_a_id);
    assert_eq!(&ca_dep.target_group, group_bc_id);
}

#[test]
fn test_stage_6_entry_exit_points() {
    let (dto, client_id) = create_dummy_workflow_dto();
    let store = ReservationStore::new();
    let mut nodes = Workflow::generate_workflow_nodes(&dto, client_id.clone(), store.clone());
    let (data_deps, sync_deps) = Workflow::build_all_dependencies(&dto, client_id, store.clone()).unwrap();
    Workflow::populate_node_adjacency_lists(&mut nodes, &data_deps, &sync_deps);
    let (mut co_allocs, node_map) = Workflow::build_co_allocations(&nodes, &sync_deps).unwrap();
    let _ = Workflow::build_co_allocation_dependencies(&data_deps, &node_map, &mut co_allocs).unwrap();

    let (entry_nodes, exit_nodes, entry_groups, exit_groups) = Workflow::find_entry_exit_points(&nodes, &co_allocs);

    // Nodes: Entry = A, Exit = C (B is internal, outputting sync to C)
    // Actually B has NO data output, but it has Sync output.
    // Logic in find_entry_exit_points:
    // entry = no incoming data AND no incoming sync
    // exit = no outgoing data AND no outgoing sync

    // A: No incoming (Entry)
    // B: Incoming Data (Not Entry), Outgoing Sync (Not Exit)
    // C: Incoming Sync (Not Entry), No outgoing (Exit)

    assert!(entry_nodes.contains(&WorkflowNodeId::new("A")));
    assert_eq!(entry_nodes.len(), 1);

    assert!(exit_nodes.contains(&WorkflowNodeId::new("C")));
    assert_eq!(exit_nodes.len(), 1);

    // Groups:
    // Group A: Entry (No incoming CA deps)
    // Group BC: Exit (No outgoing CA deps)
    let group_a_id = node_map.get(&WorkflowNodeId::new("A")).unwrap();
    let group_bc_id = node_map.get(&WorkflowNodeId::new("B")).unwrap();

    assert!(entry_groups.contains(group_a_id));
    assert!(exit_groups.contains(group_bc_id));
}
