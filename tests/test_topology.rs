// use vrm_rust_workflow::api::vrm_system_model_dto::aci_dto::{GridNodeDto, NetworkLinkDto, RMSSystemDto};
// use vrm_rust_workflow::domain::vrm_system_model::schedule::slotted_schedule::network_slotted_schedule::topology::NetworkTopology;
// use vrm_rust_workflow::domain::vrm_system_model::utils::id::AciId;
// use vrm_rust_workflow::domain::{
//     simulator::{
//         simulator::SystemSimulator,
//         simulator_mock::{MockSimulator, SharedMockSimulator},
//     },
//     vrm_system_model::{
//         grid_resource_management_system::aci::AcI,
//         reservation::reservation_store::{ReservationId, ReservationStore},
//         resource::resource_trait::Resource,
//         utils::id::{LinkResourceId, RouterId},
//     },
// };

// use std::sync::Arc;

// fn create_vrm_test_dto() -> RMSSystemDto {
//     RMSSystemDto {
//         typ: "NullBroker".to_string(),
//         scheduler_typ: "SlottedSchedule".to_string(),
//         slot_width: 256,
//         num_of_slots: 256,
//         grid_nodes: vec![
//             GridNodeDto { id: "Node-001".to_string(), cpus: 256, connected_to_router: vec!["Router-001".to_string()] },
//             GridNodeDto { id: "Node-002".to_string(), cpus: 256, connected_to_router: vec!["Router-002".to_string()] },
//             GridNodeDto { id: "Node-003".to_string(), cpus: 256, connected_to_router: vec!["Router-003".to_string()] },
//             GridNodeDto { id: "Node-004".to_string(), cpus: 256, connected_to_router: vec!["Router-001".to_string(), "Router-003".to_string()] },
//         ],
//         network_links: vec![
//             NetworkLinkDto {
//                 id: "Router-001--To--Router-002".to_string(),
//                 start_point: "Router-001".to_string(),
//                 end_point: "Router-002".to_string(),
//                 capacity: 10000,
//             },
//             NetworkLinkDto {
//                 id: "Router-001--To--Router-003".to_string(),
//                 start_point: "Router-001".to_string(),
//                 end_point: "Router-003".to_string(),
//                 capacity: 10000,
//             },
//             NetworkLinkDto {
//                 id: "Router-002--To--Router-001".to_string(),
//                 start_point: "Router-002".to_string(),
//                 end_point: "Router-001".to_string(),
//                 capacity: 5000,
//             },
//             NetworkLinkDto {
//                 id: "Router-002--To--Router-003".to_string(),
//                 start_point: "Router-002".to_string(),
//                 end_point: "Router-003".to_string(),
//                 capacity: 5000,
//             },
//             // Note: Duplicate Link
//             NetworkLinkDto {
//                 id: "Router-002--To--Router-001".to_string(),
//                 start_point: "Router-002".to_string(),
//                 end_point: "Router-001".to_string(),
//                 capacity: 5000,
//             },
//         ],
//     }
// }

// #[test]
// fn test_setup_routers() {
//     let dto = create_vrm_test_dto();

//     let routers = NetworkTopology::setup_routers(&dto);

//     assert_eq!(routers.len(), 3, "Should have created exactly 3 routers (Router-001, Router-002, Router-003)");

//     let r1_id = RouterId::new("Router-001");
//     let r2_id = RouterId::new("Router-002");
//     let r3_id = RouterId::new("Router-003");

//     assert!(routers.contains_key(&r1_id));
//     assert!(routers.contains_key(&r2_id));
//     assert!(routers.contains_key(&r3_id));

//     // Check is_grid_access_point logic
//     // All routers are connected to nodes, so all should be true.
//     assert!(routers.get(&r1_id).unwrap().is_grid_access_point);
//     assert!(routers.get(&r2_id).unwrap().is_grid_access_point);
//     assert!(routers.get(&r3_id).unwrap().is_grid_access_point);
// }

// #[test]
// fn test_setup_network_links() {
//     let dto = create_vrm_test_dto();
//     let simulator: Arc<dyn SystemSimulator> = Arc::new(MockSimulator::new(0));
//     let reservation_store = ReservationStore::new();
//     let aci_id = AciId::new("Test_setup_network_links_id");
//     let (links, importance_db) = NetworkTopology::setup_network_links(&dto, aci_id, simulator, reservation_store);

//     // Assert
//     // Setup has 5 links, but 2 have the exact same ID "Router-002--To--Router-001".
//     // The HashMap should overwrite the first with the second, resulting in 4 unique links.
//     assert_eq!(links.len(), 4, "Should have 4 unique links (duplicate ID should be overwritten)");
//     assert_eq!(importance_db.len(), 4, "Importance DB should match link count");

//     // Verify a specific link
//     let link_id = LinkResourceId::new("Router-001--To--Router-002");
//     let link = links.get(&link_id).unwrap();

//     assert!(links.contains_key(&link_id));
//     assert_eq!(link.get_capacity(), 10000);
//     assert_eq!(link.source, RouterId::new("Router-001"));
//     assert_eq!(link.target, RouterId::new("Router-002"));
//     assert_eq!(*importance_db.get(&link_id).unwrap(), 1.0);
// }

// #[test]
// fn test_setup_adjacency_matrix() {
//     let dto = create_vrm_test_dto();
//     let simulator: Arc<dyn SystemSimulator> = Arc::new(MockSimulator::new(0));
//     let reservation_store = ReservationStore::new();
//     let aci_id = AciId::new("test_setup_adjacency_matrix");

//     let (links, _) = NetworkTopology::setup_network_links(&dto, aci_id, simulator, reservation_store);
//     let routers = NetworkTopology::setup_routers(&dto);

//     let adjacency = NetworkTopology::setup_adjacency_matrix(&links, &routers);
//     let r1 = RouterId::new("Router-001");
//     let r2 = RouterId::new("Router-002");
//     let r3 = RouterId::new("Router-003");

//     // Check Router-001 outgoing links
//     // R1 -> R2, R1 -> R3
//     let r1_adj = adjacency.get(&r1).expect("Router 1 should be in adjacency matrix");
//     assert_eq!(r1_adj.len(), 2, "Router 1 should have 2 outgoing links");
//     assert!(r1_adj.contains(&LinkResourceId::new("Router-001--To--Router-002")));
//     assert!(r1_adj.contains(&LinkResourceId::new("Router-001--To--Router-003")));

//     // Check Router-002 outgoing links
//     // R2 -> R1, R2 -> R3
//     let r2_adj = adjacency.get(&r2).expect("Router 2 should be in adjacency matrix");
//     assert_eq!(r2_adj.len(), 2, "Router 2 should have 2 outgoing links");
//     assert!(r2_adj.contains(&LinkResourceId::new("Router-002--To--Router-001")));
//     assert!(r2_adj.contains(&LinkResourceId::new("Router-002--To--Router-003")));

//     // Check Router-003 outgoing links
//     // R3 has no outgoing links --> is not present in the adjacency matrix.
//     assert!(!adjacency.contains_key(&r3), "Router 3 has no outgoing links, so it should not be a key in adjacency map");
// }

// // TODO Extent the test case
// #[test]
// fn test_full_topology_creation_integration() {
//     let dto = create_vrm_test_dto();
//     let simulator: Arc<dyn SystemSimulator> = Arc::new(MockSimulator::new(0));
//     let reservation_store = ReservationStore::new();
//     let rms_id = AciId::new("test_rms");
//     let result = NetworkTopology::try_from((dto, simulator, rms_id, reservation_store));

//     assert!(result.is_ok());
//     let topology = result.unwrap();

//     // Check K-shortest paths
//     assert!(topology.path_cache.len() > 0, "Path cache should be populated after initialization");

//     // Check virtual links
//     // Example: Node 1 (R1) to Node 2 (R2) should have a path
//     // R1 -> R2 is direct
//     let r1 = RouterId::new("Router-001");
//     let r2 = RouterId::new("Router-002");

//     // We can inspect the path cache directly
//     let paths = topology.path_cache.get(&(r1.clone(), r2.clone()));
//     assert!(paths.is_some());
//     assert!(paths.unwrap().len() > 0);
// }
