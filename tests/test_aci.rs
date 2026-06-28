// use vrm_rust_workflow::api::vrm_system_model_dto::aci_dto::{AcIDto, RMSSystemDto};
// use vrm_rust_workflow::api::vrm_system_model_dto::vrm_dto::VrmSystemModelDto;

// use vrm_rust_workflow::domain::vrm_system_model::utils::id::AciId;
// use vrm_rust_workflow::domain::{
//     simulator::simulator::SystemSimulator,
//     vrm_system_model::{
//         grid_resource_management_system::aci::AcI,
//         reservation::reservation_store::{ReservationId, ReservationStore},
//     },
// };
// use vrm_rust_workflow::{generate_system_model, generate_vrm_model};

// #[derive(Debug, Clone)]
// struct MockSimulator {
//     time: i64,
// }

// impl MockSimulator {
//     fn new() -> Self {
//         Self { time: 1000 }
//     }
//     fn set_time(&mut self, time: i64) {
//         self.time = time;
//     }
// }

// impl SystemSimulator for MockSimulator {
//     fn get_current_time_in_ms(&self) -> i64 {
//         self.time
//     }
//     fn get_current_time_in_s(&self) -> i64 {
//         self.time / 1000
//     }
//     fn clone_box(&self) -> Box<dyn SystemSimulator> {
//         Box::new(self.clone())
//     }
// }

// fn setup_aci() -> (AcI, ReservationId) {
//     let simulator = Box::new(MockSimulator::new());
//     let dto = AcIDto {
//         id: "TestAcI".to_string(),
//         adc_ids: vec![],
//         commit_timeout: 10,
//         rms_system: RMSSystemDto {
//             typ: "NullRms".to_string(),
//             scheduler_type: "FCFS".to_string(),
//             num_of_slots: 100,
//             slot_width: 1,
//             grid_nodes: vec![],
//             network_links: vec![],
//         },
//     };

//     let mut aci = AcI::try_from((dto, simulator)).expect("Failed to create AcI");

//     // Inject MockRms
//     aci.rms_system = Box::new(MockRms::new());

//     // Add a test reservation to the store
//     let res = MockReservation::new("res1", "client1");
//     let id = aci.reservation_store.add(Box::new(res));

//     (aci, id)
// }
//     println!("{:#?}", vrm_system_model);
// #[test]
// fn test_aci_commit() {
//     let vrm_system_model = generate_vrm_model("/home/vincent/Desktop/Repository/VRM-Rust-Workflow/src/data/vrm.json");
//     let aci_01 = vrm_system_model.unwrap().acis.get(&AciId::new("AcI-001")).unwrap();
//     let file_path: &str = "src/data/test/test_workflow_with_simple_co_allocation_graph.json";
//     // let vrm = generate_system_model(file_path, true);
//     println!("{:#?}", vrm);
// }
