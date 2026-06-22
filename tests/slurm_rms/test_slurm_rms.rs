use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::time::sleep;
use vrm_rust_workflow::api::rms_config_dto::rms_dto::{SlurmConfigDto, SlurmRmsDto, SwitchDto, TopologyDto};
use vrm_rust_workflow::domain::simulator::simulator::GlobalClock;
use vrm_rust_workflow::domain::vrm_system_model::reservation::link_reservation::LinkReservation;
use vrm_rust_workflow::domain::vrm_system_model::reservation::node_reservation::NodeReservation;
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation::{Reservation, ReservationBase, ReservationProceeding, ReservationState};
use vrm_rust_workflow::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use vrm_rust_workflow::domain::vrm_system_model::rms::rms::Rms;
use vrm_rust_workflow::domain::vrm_system_model::rms::slurm_rms::slurm_base::SlurmRms;
use vrm_rust_workflow::domain::vrm_system_model::utils::config::{
    SLURM_TEST_BASE_URL, SLURM_TEST_JWT_TOKEN, SLURM_TEST_USER_NAME, SLURM_TEST_VERSION,
};
use vrm_rust_workflow::domain::vrm_system_model::utils::id::{AciId, ClientId, ComponentId, ReservationName};

/// Tests the normal commit process to the local RMS
/// Reservation of state ReserveAnswer -> Committed and task is running on the local RMS.
#[tokio::test]
async fn test_slurm_rms_commit_lifecycle() {
    let clock = Arc::new(GlobalClock::new(false));
    let slurm_rms = create_test_slurm_rms(clock.clone()).await.expect("Failed to create SlurmRms");
    let reservation_store = slurm_rms.get_reservation_store().clone();

    let res_name = ReservationName::new("test_commit_job".to_string());
    let res = create_node_reservation(res_name, ReservationState::ReserveAnswer, clock);
    let res_id = reservation_store.add(res);

    slurm_rms.commit(res_id.clone());

    // Wait for the actual commit at the local RMS
    for _ in 0..10 {
        if reservation_store.get_state(res_id.clone()) == ReservationState::Committed {
            break;
        }

        sleep(Duration::from_millis(100)).await;
    }

    assert_eq!(
        reservation_store.get_state(res_id),
        ReservationState::Committed,
        "The reservation state for ID {:?} should have been Committed!",
        res_id
    );
}

/// Tests the normal commit process to the local RMS, but with five reservation.
/// Reservation of state ReserveAnswer -> Committed and task is running on the local RMS.
#[tokio::test]
async fn test_slurm_rms_commit_multiple_concurrently() {
    let clock = Arc::new(GlobalClock::new(false));
    let slurm_rms = create_test_slurm_rms(clock.clone()).await.expect("Failed to create SlurmRms");
    let reservation_store = slurm_rms.get_reservation_store().clone();

    let mut ids = Vec::new();
    for i in 0..5 {
        let res_name = ReservationName::new(format!("concurrent_commit_{}", i));
        let res = create_node_reservation(res_name, ReservationState::ReserveAnswer, clock.clone());
        ids.push(reservation_store.add(res));
    }

    for id in &ids {
        slurm_rms.commit(id.clone());
    }

    // Wait for the actual commit at the local RMS
    for id in ids {
        let mut success = false;
        for _ in 0..20 {
            if reservation_store.get_state(id.clone()) == ReservationState::Committed {
                success = true;
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        assert!(success, "Concurrent commit for {:?} failed", id);
    }
}

/// Tests to submit a link reservation --> should produce a SlurmRmsCommitFalseReservationTypeError.
#[tokio::test]
async fn test_slurm_rms_commit_link_reservation() {
    let clock = Arc::new(GlobalClock::new(false));
    let mut logger = logtest::start();

    let slurm_rms = create_test_slurm_rms(clock.clone()).await.expect("Failed to create SlurmRms");
    let reservation_store = slurm_rms.get_reservation_store().clone();

    let res_name = ReservationName::new("link_reservation".to_string());
    let res = create_link_reservation(res_name, ReservationState::ReserveAnswer, clock);
    let res_id = reservation_store.add(res);

    slurm_rms.commit(res_id.clone());

    // Wait for the actual commit at the local RMS
    for _ in 0..10 {
        if reservation_store.get_state(res_id.clone()) == ReservationState::Committed {
            break;
        }

        sleep(Duration::from_millis(100)).await;
    }

    let found = logger.any(|record| record.args().contains("SlurmRmsCommitFalseReservationTypeError"));

    assert!(found, "The expected warning log was not found!");
}

/// Tests to submit a reservation that is not in ReservationStore --> should produce a SlurmRmsCommitInValidReservationError.
#[tokio::test]
async fn test_slurm_rms_commit_reservation_not_in_store() {
    let clock = Arc::new(GlobalClock::new(false));
    let mut logger = logtest::start();

    let slurm_rms = create_test_slurm_rms(clock.clone()).await.expect("Failed to create SlurmRms");
    let mut reservation_store = slurm_rms.get_reservation_store().clone();

    let res_name = ReservationName::new("node_reservation".to_string());
    let res = create_node_reservation(res_name, ReservationState::ProbeReservation, clock);

    let res_id = reservation_store.add(res);
    reservation_store.delete_probe_reservation(res_id);

    slurm_rms.commit(res_id.clone());

    let error_message = format!("SlurmRmsCommitInValidReservationError: The reservation {:?} was not found.", res_id);
    let found = logger.any(|record| record.args().contains(&error_message));

    assert!(found, "The expected warning log was not found!");
}

///////////////////
/// delete_task ///
///////////////////

/// Test with a setup, where only the deletion on the local RMS is possible.
#[tokio::test]
async fn test_slurm_rms_delete_task_only_rms() {
    let clock = Arc::new(GlobalClock::new(false));
    let mut logger = logtest::start();
    let mut slurm_rms = create_test_slurm_rms(clock.clone()).await.expect("Failed to create SlurmRms");
    let reservation_store = slurm_rms.get_reservation_store().clone();

    let res_name = ReservationName::new("test_delete_job".to_string());
    let res = create_node_reservation(res_name, ReservationState::ReserveAnswer, clock);
    let res_id = reservation_store.add(res);

    // Commit a task to Rms
    slurm_rms.commit(res_id.clone());
    for _ in 0..10 {
        if reservation_store.get_state(res_id.clone()) == ReservationState::Committed {
            break;
        }

        sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(reservation_store.get_state(res_id), ReservationState::Committed);

    // Delete Task
    slurm_rms.delete_task(res_id.clone(), None);

    for _ in 0..10 {
        if reservation_store.get_state(res_id.clone()) == ReservationState::Deleted {
            break;
        }

        sleep(Duration::from_millis(100)).await;
    }

    // Is RMS cleaned up?
    assert_eq!(reservation_store.get_state(res_id.clone()), ReservationState::Rejected);
    assert!(!slurm_rms.task_mapping.read().contains_left(&res_id));

    let error_message = format!(
        "SlurmRmsDeletionCleanupError: The reservation {:?} was not successfully deleted from schedule, but the reservation was successfully deleted from the Rms system.",
        res_id
    );
    let found = logger.any(|record| record.args().contains(&error_message));

    assert!(found, "The expected warning log was not found!");
}

/// Test with a setup, where only the deletion on the local RMS is possible.
#[tokio::test]
async fn test_slurm_rms_delete_task_from_rms_and_schedule() {
    let clock = Arc::new(GlobalClock::new(false));
    let mut slurm_rms = create_test_slurm_rms(clock.clone()).await.expect("Failed to create SlurmRms");
    let reservation_store = slurm_rms.get_reservation_store().clone();

    let res_name = ReservationName::new("test_delete_job".to_string());
    let res = create_node_reservation(res_name, ReservationState::Open, clock);
    let res_id = reservation_store.add(res);

    // 1. Reserve Reservation at node schedule
    {
        // Lock slurm_rms only for this scope
        let mut guard = slurm_rms.node_schedule.write();
        let _reserve_id = guard.reserve(res_id).expect("Failed to reserve the Reservation on node schedule.");
        assert_eq!(reservation_store.get_state(res_id), ReservationState::ReserveAnswer, "Reservation should be reserved in the node schedule.");
    }

    // 2. Commit a task to Rms
    slurm_rms.commit(res_id.clone());
    for _ in 0..10 {
        if reservation_store.get_state(res_id.clone()) == ReservationState::Committed {
            break;
        }

        sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(reservation_store.get_state(res_id), ReservationState::Committed, "Reservation should be now committed to RMS.");

    // 3. Delete Task
    slurm_rms.delete_task(res_id.clone(), None);

    for _ in 0..10 {
        if reservation_store.get_state(res_id.clone()) == ReservationState::Deleted {
            break;
        }

        sleep(Duration::from_millis(100)).await;
    }

    // Is RMS cleaned up?
    assert_eq!(reservation_store.get_state(res_id.clone()), ReservationState::Deleted, "Reservation should be deleted form the ReservationStore.");
    assert!(!slurm_rms.task_mapping.read().contains_left(&res_id));
}

async fn create_test_slurm_rms(clock: Arc<GlobalClock>) -> Result<SlurmRms, Box<dyn std::error::Error>> {
    let component_id = ComponentId::new("test-aci".to_string());

    let reservation_store = ReservationStore::new();

    let rest_api_config: SlurmConfigDto = SlurmConfigDto {
        base_url: SLURM_TEST_BASE_URL.to_string(),
        version: SLURM_TEST_VERSION.to_string(),
        user_name: SLURM_TEST_USER_NAME.to_string(),
        jwt_token: SLURM_TEST_JWT_TOKEN.to_string(),
    };

    let slurm_switch_dto_0 =
        SwitchDto { switch_name: "s0".to_string(), switches: vec![], nodes: vec!["c0".to_string(), "c1".to_string()], link_speed: 1000 };

    let slurm_switch_dto_1 = SwitchDto {
        switch_name: "s1".to_string(),
        switches: vec![],
        nodes: vec!["c3".to_string(), "c4".to_string(), "c5".to_string(), "c6".to_string()],
        link_speed: 1000,
    };

    let slurm_switch_dto_2 = SwitchDto {
        switch_name: "s2".to_string(),
        switches: vec!["s0".to_string(), "s1".to_string()],
        nodes: vec!["c2".to_string()],
        link_speed: 1000,
    };

    let topology: TopologyDto = TopologyDto {
        egress_bandwidth_gbps: 1000,
        ingress_bandwidth_gbps: 1000,
        gateway_switch_id: "s0".to_string(),
        switches: vec![slurm_switch_dto_0, slurm_switch_dto_1, slurm_switch_dto_2],
    };

    let slurm_rms_dto: SlurmRmsDto = SlurmRmsDto {
        id: "RMS-ID".to_string(),
        scheduler_typ: "SlottedSchedule".to_string(),
        slot_width: 60 * 60,
        num_of_slots: 2,
        rest_api_config: rest_api_config,
        topology: topology,
    };

    return SlurmRms::new(slurm_rms_dto, clock, component_id, reservation_store).await;
}

fn create_node_reservation(res_name: ReservationName, reservation_state: ReservationState, clock: Arc<GlobalClock>) -> Reservation {
    let client_id = ClientId::new("test_client".to_string());

    let duration = 600; // 10 minutes
    let start_time = clock.get_system_time_s() + 60; // Start in 1 minute
    let end_time = start_time + duration;

    let base = ReservationBase {
        name: res_name.clone(),
        client_id,
        handler_id: None,
        state: reservation_state,
        request_proceeding: ReservationProceeding::Commit,
        arrival_time: clock.get_system_time_s(),
        booking_interval_start: start_time,
        booking_interval_end: end_time,
        assigned_start: start_time,
        assigned_end: end_time,
        task_duration: duration,
        reserved_capacity: 1,
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

fn create_link_reservation(res_name: ReservationName, reservation_state: ReservationState, clock: Arc<GlobalClock>) -> Reservation {
    let client_id = ClientId::new("test_client".to_string());

    let duration = 600; // 10 minutes
    let start_time = clock.get_system_time_s() + 60; // Start in 1 minute
    let end_time = start_time + duration;

    let base = ReservationBase {
        name: res_name.clone(),
        client_id,
        handler_id: None,
        state: reservation_state,
        request_proceeding: ReservationProceeding::Commit,
        arrival_time: clock.get_system_time_s(),
        booking_interval_start: start_time,
        booking_interval_end: end_time,
        assigned_start: start_time,
        assigned_end: end_time,
        task_duration: duration,
        reserved_capacity: 1,
        is_moldable: false,
        moldable_work: duration,
        frag_delta: 0.0,
    };

    let link_res = LinkReservation { base, end_point: None, start_point: None };

    return Reservation::Link(link_res);
}
