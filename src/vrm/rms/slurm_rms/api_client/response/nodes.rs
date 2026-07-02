use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Root response object for the Slurm /nodes endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmNodesResponse {
    pub nodes: Vec<SlurmNode>,
    pub last_update: SlurmNumericValue,
    pub meta: SlurmMeta,
    pub errors: Vec<SlurmError>,
    pub warnings: Vec<SlurmWarning>,

    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

/// Represents a single compute node in the cluster
#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmNode {
    pub architecture: String,
    pub burstbuffer_network_address: String,
    pub boards: u32,
    pub boot_time: SlurmNumericValue,
    pub cluster_name: String,

    // The number of physical CPU cores on the node.
    pub cores: u32,

    // The count of cores reserved for system overhead (e.g., Slurm daemons, networking) and not available for user jobs.
    pub specialized_cores: u32,
    pub cpu_binding: u32,

    // The current system load average on the node
    // None if Node down or not responding
    pub cpu_load: Option<u32>,
    pub free_mem: SlurmNumericValue,

    // The total number of logical processors (CPUs) available on the node.
    pub cpus: u32,
    // The number of CPUs actually available for Slurm to schedule. This might differ from cpus if some are reserved for the system or specialized tasks.
    pub effective_cpus: u32,
    pub specialized_cpus: String,
    pub energy: SlurmEnergy,
    pub external_sensors: serde_json::Value,
    pub extra: String,
    pub power: serde_json::Value,
    pub features: Vec<String>,
    pub active_features: Vec<String>,
    pub gres: String,
    pub gres_drained: String,
    pub gres_used: String,
    pub instance_id: String,
    pub instance_type: String,
    pub last_busy: SlurmNumericValue,
    pub mcs_label: String,
    pub specialized_memory: u64,
    pub name: String,
    pub next_state_after_reboot: Vec<String>,
    pub address: String,
    pub hostname: String,
    pub state: Vec<String>,
    pub operating_system: String,
    pub owner: String,
    pub partitions: Vec<String>,
    pub port: u16,

    // The total amount of physical RAM (in Megabytes) available
    pub real_memory: u64,
    pub comment: String,
    pub reason: String,
    pub reason_changed_at: SlurmNumericValue,
    pub reason_set_by_user: String,
    pub resume_after: SlurmNumericValue,

    // If the node is part of an active Slurm reservation (a block of time/resources set aside for specific users or projects), the name of that reservation appears here.
    pub reservation: String,
    pub alloc_memory: u64,

    // The number of CPUs currently allocated to running jobs.
    pub alloc_cpus: u32,

    // The number of CPUs that are currently unallocated and available for new jobs.
    pub alloc_idle_cpus: u32,
    pub tres_used: String,
    pub tres_weighted: f64,
    pub slurmd_start_time: SlurmNumericValue,
    pub sockets: u32,
    pub threads: u32,
    pub temporary_disk: u32,
    pub weight: u32,
    pub tres: String,
    pub version: String,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub unknown_attributes: HashMap<String, serde_json::Value>,
}

/// Common Slurm structure for values that can be infinite or unset
#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmNumericValue {
    pub set: bool,
    pub infinite: bool,
    pub number: u64,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Energy consumption metrics for a node
#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmEnergy {
    pub average_watts: u32,
    pub base_consumed_energy: u64,
    pub consumed_energy: u64,
    pub current_watts: SlurmNumericValue,
    pub previous_consumed_energy: u64,
    pub last_collected: u64,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Metadata about the API response and plugin versions
#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmMeta {
    pub plugin: SlurmPlugin,
    pub client: SlurmClient,
    pub command: Vec<String>,
    pub slurm: SlurmVersionInfo,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmPlugin {
    #[serde(rename = "type")]
    pub plugin_type: String,
    pub name: String,
    pub data_parser: String,
    pub accounting_storage: String,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmClient {
    pub source: String,
    pub user: String,
    pub group: String,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmVersionInfo {
    pub version: SlurmVersionParts,
    pub release: String,
    pub cluster: String,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmVersionParts {
    pub major: String,
    pub micro: String,
    pub minor: String,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmError {
    pub error_number: Option<i32>,
    pub error_message: Option<String>,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlurmWarning {
    pub warning_message: Option<String>,
    pub source: Option<String>,

    /// Captures any new attributes added in future Slurm releases
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
