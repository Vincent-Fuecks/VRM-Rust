// use std::sync::Arc;

// use vrm_rust_workflow::{
//     api::rms_config_dto::rms_dto::{GridNodeDto, NetworkLinkDto},
//     domain::{
//         simulator::simulator::GlobalClock,
//         vrm_system_model::{
//             reservation::reservation_store::ReservationStore, resource::{link_resource::LinkResource, resource_store}, schedule::slotted_schedule::strategy::link::topology::{self, NetworkTopology}, utils::id::{AciId, RouterId}
//         },
//     },
// };

// #[test]
// fn test_topology_case_1_complex_dag() {
//     // 1 -> 2; 1 -> 2; 1 -> 3; 5 -> 1; 2 -> 4; 3 -> 5; 4 -> 5
//     // Nodes equal Router (each node is to a unique router connected)
//     // Shortest way from 1 -> 5 is (two hops):  1 --> 3 --> 5 or 1 --> 4 --> 5
//     // Other ways: 1 --> 2 --> 3 --> 5 or 1 --> 2 --> 4 --> 5
//     // Edge case: (1) direct connection between 5 --> 1, but false direction
//     let simulator = Arc::new(GlobalClock::new(true));

//     let edges = vec![(1, 2), (1, 3), (1, 4), (5, 1), (2, 4), (2, 3), (3, 5), (4, 5)];

//     let dto = create_custom_topology_dto(edges);
//     let reservation_store = ReservationStore::new();

//     let topology = NetworkTopology::try_from((dto, simulator, AciId::new("case_1"), reservation_store)).unwrap();

//     // Check paths
//     // 1 --> 3 --> 5
//     // 1 --> 4 --> 5
//     // 1 --> 2 --> 4 --> 5
//     // 1 --> 2 --> 3 --> 5
//     assert_paths_contain(
//         &topology,
//         "Router-001",
//         "Router-005",
//         vec![
//             vec!["Router-001--To--Router-003", "Router-003--To--Router-005"],
//             vec!["Router-001--To--Router-004", "Router-004--To--Router-005"],
//             vec!["Router-001--To--Router-002", "Router-002--To--Router-004", "Router-004--To--Router-005"],
//             vec!["Router-001--To--Router-002", "Router-002--To--Router-003", "Router-003--To--Router-005"],
//         ],
//     );

//     // Edge case: Direct connection backwards
//     assert_paths_contain(&topology, "Router-005", "Router-001", vec![vec!["Router-005--To--Router-001"]]);
// }

// #[test]
// fn test_topology_case_2_linear_chain() {
//     // 1 -> 2; 2 -> 3; 3 -> 4; 4 -> 5
//     let edges = vec![(1, 2), (2, 3), (3, 4), (4, 5)];
//     let dto = create_custom_topology_dto(edges);
//     let simulator = Arc::new(GlobalClock::new(true));
//     let reservation_store = ReservationStore::new();

//     let topology = NetworkTopology::try_from((dto, simulator, AciId::new("case_2"), reservation_store)).unwrap();

//     // Check path form 1 to 5
//     assert_paths_contain(
//         &topology,
//         "Router-001",
//         "Router-005",
//         vec![vec!["Router-001--To--Router-002", "Router-002--To--Router-003", "Router-003--To--Router-004", "Router-004--To--Router-005"]],
//     );
// }

// #[test]
// fn test_topology_case_3_fully_connected() {
//     // 5 Nodes, fully connected
//     let mut edges = Vec::new();
//     for i in 1..=5 {
//         for j in 1..=5 {
//             if i != j {
//                 edges.push((i, j));
//             }
//         }
//     }
//     let dto = create_custom_topology_dto(edges);
//     let simulator = Arc::new(GlobalClock::new(true));
//     let reservation_store = ReservationStore::new();

//     let topology = NetworkTopology::try_from((dto, simulator, AciId::new("case_3"), reservation_store)).unwrap();

//     let r1 = RouterId::new("Router-001");
//     let r2 = RouterId::new("Router-002");
//     let paths = topology.path_cache.get(&(r1, r2)).expect("Should find many paths in clique");

//     // Since K_NUMBER_OF_PATHS is 10, it should find exactly 10 if they exist
//     assert_eq!(paths.len(), 10);

//     // Verify at least the direct path is found (shortest)
//     let direct_link = "Router-001--To--Router-002";
//     let has_direct = paths.iter().any(|p| p.network_links.len() == 1 && p.network_links[0].to_string() == direct_link);
//     assert!(has_direct, "Fully connected graph should always find direct link as one of the paths");
// }

// #[test]
// fn test_topology_case_4_disconnected_groups() {
//     // 6 Nodes: (1,2,3) fully connected and (4,5,6) fully connected
//     let mut edges = Vec::new();
//     // Group A
//     for i in 1..=3 {
//         for j in 1..=3 {
//             if i != j {
//                 edges.push((i, j));
//             }
//         }
//     }
//     // Group B
//     for i in 4..=6 {
//         for j in 4..=6 {
//             if i != j {
//                 edges.push((i, j));
//             }
//         }
//     }

//     let dto = create_custom_topology_dto(edges);
//     let simulator = Arc::new(GlobalClock::new(true));
//     let reservation_store = ReservationStore::new();

//     let topology = NetworkTopology::try_from((dto, simulator, AciId::new("case_4"), reservation_store)).unwrap();

//     let r1 = RouterId::new("Router-001");
//     let r4 = RouterId::new("Router-004");

//     // Path between disconnected groups should not exist
//     let paths = topology.path_cache.get(&(r1.clone(), r4));
//     assert!(paths.is_none() || paths.unwrap().is_empty(), "Groups should be isolated");

//     // Path within group should exist
//     let r2 = RouterId::new("Router-002");
//     assert!(topology.path_cache.get(&(r1.clone(), r2.clone())).is_some());
// }

// #[test]
// fn test_topology_case_5_empty_network() {
//     let edges = vec![];
//     let dto = create_custom_topology_dto(edges);
//     let simulator = Arc::new(GlobalClock::new(true));
//     let reservation_store = ReservationStore::new();

//     // Should not panic, but create an empty topology
//     let topology = NetworkTopology::try_from((dto, simulator, AciId::new("case_5"), reservation_store)).unwrap();

//     assert!(topology.path_cache.is_empty());
//     assert!(topology.virtual_link_resources.is_empty());
//     assert_eq!(topology.max_bandwidth_all_paths, 0);
// }

// #[test]
// fn test_topology_case_6_single_node_no_links() {
//     // 1 Node, no edges.
//     let grid_nodes = vec![GridNodeDto { id: "Node-001".into(), cpus: 1, connected_to_router: vec!["Router-001".into()] }];

//     let dto = RMSSystemDto {
//         typ: "NullBroker".to_string(),
//         scheduler_typ: "SlottedSchedule".to_string(),
//         slot_width: 256,
//         num_of_slots: 256,
//         grid_nodes,
//         network_links: vec![],
//     };

//     let simulator = Arc::new(GlobalClock::new(true));
//     let reservation_store = ReservationStore::new();

//     let topology = NetworkTopology::try_from((dto, simulator, AciId::new("case_6"), reservation_store)).unwrap();

//     // Topology is valid, but no paths can be calculated
//     assert!(topology.path_cache.is_empty());
// }

// #[test]
// fn test_topology_case_7_cycles() {
//     let edges = vec![(1, 2), (2, 1), (1, 3), (3, 1), (2, 3), (3, 3)];

//     let dto = create_custom_topology_dto(edges);
//     let simulator = Arc::new(GlobalClock::new(true));
//     let reservation_store = ReservationStore::new();

//     let topology = NetworkTopology::try_from((dto, simulator, AciId::new("case_7"), reservation_store)).unwrap();

//     // Check 1 -> 3 should not find a cycle
//     // 1 --> 3
//     // 1 --> 2 --> 3
//     assert_paths_contain(
//         &topology,
//         "Router-001",
//         "Router-003",
//         vec![vec!["Router-001--To--Router-003"], vec!["Router-001--To--Router-002", "Router-002--To--Router-003"]],
//     );

//     // Check 1 -> 2
//     assert_paths_contain(&topology, "Router-001", "Router-002", vec![vec!["Router-001--To--Router-002"]]);
// }

// /// Creates a DTO based on a list of directed edges (source -> target).
// /// Automatically creates a GridNode for every router involved to ensure they are "Grid Access Points".
// fn create_custom_topology_dto(edges: Vec<(i32, i32)>) -> RMSSystemDto {
//     let mut grid_nodes = Vec::new();
//     let mut network_links = Vec::new();
//     let mut seen_routers = std::collections::HashSet::new();

//     for (src, dst) in edges {
//         let src_id = format!("Router-{:03}", src);
//         let dst_id = format!("Router-{:03}", dst);

//         if seen_routers.insert(src) {
//             grid_nodes.push(GridNodeDto { id: format!("Node-{:03}", src), cpus: 256, connected_to_router: vec![src_id.clone()] });
//         }
//         if seen_routers.insert(dst) {
//             grid_nodes.push(GridNodeDto { id: format!("Node-{:03}", dst), cpus: 256, connected_to_router: vec![dst_id.clone()] });
//         }

//         network_links.push(NetworkLinkDto { id: format!("{}--To--{}", src_id, dst_id), start_point: src_id, end_point: dst_id, capacity: 10000 });
//     }

//     RMSSystemDto {
//         typ: "NullBroker".to_string(),
//         scheduler_typ: "SlottedSchedule".to_string(),
//         slot_width: 256,
//         num_of_slots: 256,
//         grid_nodes,
//         network_links,
//     }
// }

// fn assert_paths_contain(topology: &NetworkTopology, src: &str, dst: &str, expected_link_sequences: Vec<Vec<&str>>) {
//     let src_id = RouterId::new(src);
//     let dst_id = RouterId::new(dst);

//     let found_paths =
//         topology.path_cache.get(&(src_id.clone(), dst_id.clone())).unwrap_or_else(|| panic!("No paths found between {} and {}", src, dst));

//     assert_eq!(found_paths.len(), expected_link_sequences.len(), "Number of found paths differs from expectation for {} -> {}", src, dst);

//     let found_sequences: Vec<Vec<String>> = found_paths.iter().map(|p| p.network_links.iter().map(|l| l.to_string()).collect()).collect();

//     for expected_seq in expected_link_sequences {
//         let expected_string_seq: Vec<String> = expected_seq.iter().map(|s| s.to_string()).collect();

//         let found = found_sequences.iter().any(|seq| *seq == expected_string_seq);

//         assert!(found, "Expected path sequence {:?} was not found in results: {:?}", expected_string_seq, found_sequences);
//     }

//     fn create_topology() -> NetworkTopology {
//             let link_resource = LinkResource::new(name, source, target, capacity, schedule)
//     resource_store.add_link()

//     let topology = NetworkTopology::new(links, nodes, slot_width, num_of_slots, simulator, aci_id, reservation_store, resource_store)
//     }
// }
