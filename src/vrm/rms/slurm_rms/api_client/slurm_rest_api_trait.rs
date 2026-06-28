use std::fmt::Debug;

use anyhow::Result;

use super::payload::task_properties::TaskSubmission;
use super::response::nodes::SlurmNodesResponse;
use super::response::tasks::SlurmTaskResponse;

/// **SlurmRestApi** provides a high-level abstraction for interacting with a
/// **Slurm Resource Management System (RMS)** via its REST interface.
///
/// This trait is designed for use within a distributed **Virtual Resource Manager (VRM)**,
/// allowing the meta-scheduler to query node states, manage job lifecycle, and monitor
/// cluster health.
#[async_trait::async_trait]
pub trait SlurmRestApi: Debug {
    /// Retrieves comprehensive metadata for all nodes managed by the Slurm cluster.
    ///
    /// Performs a `GET` request to the `/nodes` endpoint.
    ///
    /// # Returns
    /// * `Ok(SlurmTaskResponse)` if the RMS request was success.
    /// * Otherwise an `Err` is returned.
    ///
    /// # Errors
    /// Returns an error if the network request fails or if the response cannot be
    /// deserialized into a [`SlurmNodesResponse`].
    async fn get_nodes(&self) -> Result<SlurmNodesResponse>;

    /// Retrieves status and configuration metadata for all jobs (tasks) of the RMS.
    ///
    /// Performs a `GET` request to the `/jobs` endpoint.
    ///
    /// # Returns
    /// * `Ok(SlurmTaskResponse)` if the RMS request was success.
    /// * `Err` otherwise
    ///
    /// # Errors
    /// Returns an error if the network request fails or if the response cannot be
    /// deserialized into a [`SlurmTaskResponse`].
    async fn get_tasks(&self) -> Result<SlurmTaskResponse>;

    /// Performs a connectivity check (Ping) against the Slurm REST API to ensure the
    /// Resource Management System is operational.
    ///
    /// Performs a `GET` request to the `/ping` endpoint.
    ///
    /// # Returns
    /// * `Ok(true)` if the RMS responds with a success status code.
    /// * `Ok(false)` if the RMS is reachable but reports an unhealthy state.
    /// * `Err` if a connection-level error occurs.
    async fn is_rms_alive(&self) -> Result<bool>;

    /// Submits a new compute task ([`NodeReservation`]) to the Slurm cluster for execution.
    ///
    /// Performs a `POST` request to the `/submit` endpoint.
    ///
    /// # Arguments
    /// * `payload` - The slurm required [`TaskSubmission`] task submission format.
    ///
    /// # Returns
    ///  * `Ok(u32)` the **Slurm Job ID** assigned by the RMS on success.
    ///  * `Err` Otherwise
    ///
    /// # Errors
    /// Returns an error if the submission is rejected due to invalid parameters,
    /// insufficient permissions, or resource constraints.
    async fn commit(&self, payload: TaskSubmission) -> Result<u32>;

    /// Submits via the Slurm REST API a delete request to the RMS.
    /// Via: DELETE /slurm/{SLURM-API-VERSION}/{job_id}
    ///
    /// # Arguments
    /// * `task_id` - The unique identifier (Job ID) assigned by Slurm.
    ///
    /// # Returns
    ///  * `Ok(bool)` returns the result of the slurm task request removal.
    ///  * `Err` Otherwise
    async fn delete(&self, task_id: u32) -> Result<bool>;
}
