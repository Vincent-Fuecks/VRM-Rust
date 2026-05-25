use std::{collections::HashSet, sync::Arc};

use anyhow::Result;
use vrm_rust_workflow::{
    api::rms_config_dto::rms_dto::{SlurmConfigDto, SlurmRmsDto, SlurmSwitchDto},
    domain::{
        simulator::simulator::GlobalClock,
        vrm_system_model::{
            reservation::reservation_store::ReservationStore,
            rms::slurm_rms::{
                api_client::{
                    payload::task_properties::{JobProperties, TaskSubmission},
                    slurm_rest_api_trait::SlurmRestApi,
                },
                slurm_base::SlurmRms,
            },
            utils::{
                config::{SLURM_TEST_BASE_URL, SLURM_TEST_JWT_TOKEN, SLURM_TEST_USER_NAME, SLURM_TEST_VERSION},
                id::AciId,
            },
        },
    },
};

/// Tests the Slurm Rest API ping.
#[tokio::test]
async fn test_is_rms_alive() {
    // Setup the mock RMS
    let slurm_rms = create_slurm_rms_mock().await.expect("Error during the create_slurm_rms_mock creation process");

    // Perform the ping check
    let is_rms_alive = slurm_rms.slurm_rest_client.is_rms_alive().await.expect("Docker Slurm Cluster is offline or API key is missing");

    // Assert the status
    assert!(is_rms_alive, "Slurm reported it is not alive");
}

/// Test the deletion process of the connected rms cluster
#[tokio::test]
async fn test_task_deletion_process() {
    // Setup the mock RMS
    let mut slurm_rms = create_slurm_rms_mock().await.expect("Failed to create Slurm RMS mock");

    // Submit a task so we have something to delete
    let slurm_task_id = commit_task_to_rms(&mut slurm_rms).await.expect("Failed to commit task to RMS");

    let task_counter_after_commit = slurm_rms.get_active_task_count().await.expect("Getting active task count failed");

    // Attempt to delete the task
    let is_deleted = slurm_rms.slurm_rest_client.delete(slurm_task_id).await.expect("The delete request failed with an error");

    // Assert the API response
    assert!(is_deleted, "The API reported the task was not deleted.");

    // Give the mock/system a moment to update
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify that the task count decreased by exactly one
    let task_counter_after_delete = slurm_rms.get_active_task_count().await.expect("Failed to get tasks from RMS after deletion");

    assert_eq!(task_counter_after_commit - 1, task_counter_after_delete, "The task count did not decrease by exactly one.");
}

/// Test the submission process of the connected rms cluster
#[tokio::test]
async fn test_submission_process() {
    // Use expect() to see the actual error if create_slurm_rms_mock fails
    let mut slurm_rms = create_slurm_rms_mock().await.expect("Failed to create Slurm RMS mock");

    let count_before = slurm_rms.get_active_task_count().await.expect("Failed to get count before submission");

    commit_task_to_rms(&mut slurm_rms).await.expect("Failed to commit task to RMS");

    // Small delay to allow the mock/client to process
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let count_after = slurm_rms.get_active_task_count().await.expect("Failed to get count after submission"); // This will now show the REAL error

    assert_eq!(count_before + 1, count_after, "The task count did not increase by exactly one.");
}

/// Test the response of the RMS system, if all tasks running or pending are requested.
#[tokio::test]
async fn test_get_tasks() {
    let mut slurm_rms = create_slurm_rms_mock().await.expect("Failed to create Slurm RMS mock");

    let slurm_id = commit_task_to_rms(&mut slurm_rms).await.expect("Failed to commit task to RMS");

    let task_response = slurm_rms.slurm_rest_client.get_tasks().await.expect("Failed to get tasks form Slurm RMS.");

    let mut has_found_committed_task = false;

    for task in task_response.jobs {
        if task.job_id == slurm_id {
            has_found_committed_task = true;
        }
    }

    assert!(has_found_committed_task, "The submitted task was not found on RMS.");
}

/// Test the response of the RMS system, if all tasks running or pending are requested. But no task are running or pending on the RMS.
/// Note: This test will fail, if tasks are still in the cleanup process (COMPLETING etc.).
#[tokio::test]
async fn test_get_tasks_rms_is_empty() {
    let slurm_rms = create_slurm_rms_mock().await.expect("Failed to create Slurm RMS mock");
    let has_deleted = slurm_rms.delete_all_tasks().await.expect("Failed to delete all Tasks on RMS.");
    assert!(has_deleted, "Failed to delete all task from RMS.");

    let task_response = slurm_rms.slurm_rest_client.get_tasks().await.expect("Failed to get tasks form Slurm RMS.");
    println!("{:?}", task_response);

    assert!(task_response.jobs.is_empty(), "There where task found on the RMS.");
}

/// Test get nodes request of the Slurm REST API.
#[tokio::test]
async fn test_get_nodes() {
    let rms_node_names = vec!["c3".to_string(), "c4".to_string(), "c5".to_string(), "c6".to_string()];
    let slurm_rms = create_slurm_rms_mock().await.expect("Failed to create Slurm RMS mock");
    let nodes = slurm_rms.slurm_rest_client.get_nodes().await.expect("Failed to get the nodes from the RMS.");
    let node_names: HashSet<String> = nodes.nodes.iter().map(|node| node.name.clone()).collect();

    let is_valid = rms_node_names.iter().all(|name| node_names.contains(name));

    assert!(is_valid, "RMS contains not the specified nodes.");
}

async fn commit_task_to_rms(slurm_rms: &mut SlurmRms) -> Result<u32> {
    let task_properties = JobProperties {
        name: "task-001".to_string(),
        nodes: Some("1-2".to_string()),
        cpus_per_task: 1,
        begin_time: 0,
        time_limit: 1000,
        memory_per_node: 256,
        current_working_directory: Some("/tmp".to_string()),
        standard_error: Some("/task-001.error".to_string()),
        standard_output: Some("/task-001.out".to_string()),
        environment: Some(vec!["PATH=/usr/bin:/bin".to_string()]),
        dependency: None,
    };

    let script = "#!/bin/bash\nhostname\nsleep 10".to_string();

    let payload = TaskSubmission { job: task_properties, script: script };

    let slurm_task_id = slurm_rms.slurm_rest_client.commit(payload).await?;

    return Ok(slurm_task_id);
}

async fn create_slurm_rms_mock() -> Result<SlurmRms, Box<dyn std::error::Error>> {
    let simulator = Arc::new(GlobalClock::new(false));
    let aci_id = AciId::new("Test-AcI");
    let reservation_store = ReservationStore::new();
    let rest_api_config: SlurmConfigDto = SlurmConfigDto {
        base_url: SLURM_TEST_BASE_URL.to_string(),
        version: SLURM_TEST_VERSION.to_string(),
        user_name: SLURM_TEST_USER_NAME.to_string(),
        jwt_token: SLURM_TEST_JWT_TOKEN.to_string(),
    };

    // 2. Define the individual switches for the topology
    let slurm_switch_dto_0 =
        SlurmSwitchDto { switch_name: "s0".to_string(), switches: vec![], nodes: vec!["c0".to_string(), "c1".to_string()], link_speed: 1000 };

    let slurm_switch_dto_1 = SlurmSwitchDto {
        switch_name: "s1".to_string(),
        switches: vec![],
        nodes: vec!["c3".to_string(), "c4".to_string(), "c5".to_string(), "c6".to_string()],
        link_speed: 1000,
    };

    let slurm_switch_dto_2 = SlurmSwitchDto {
        switch_name: "s2".to_string(),
        switches: vec!["s0".to_string(), "s1".to_string()],
        nodes: vec!["c2".to_string()],
        link_speed: 1000,
    };

    // 3. Assemble the topology vector
    let topology: Vec<SlurmSwitchDto> = vec![slurm_switch_dto_0, slurm_switch_dto_1, slurm_switch_dto_2];

    let slurm_rms_dto: SlurmRmsDto = SlurmRmsDto {
        id: "RMS-ID".to_string(),
        scheduler_typ: "SlottedSchedule".to_string(),
        slot_width: 60,
        num_of_slots: 60,
        rest_api_config: rest_api_config,
        topology: topology,
    };

    return SlurmRms::new(slurm_rms_dto, simulator, aci_id, reservation_store).await;
}
