use serde::{Deserialize, Serialize};

use crate::domain::vrm_system_model::{
    reservation::reservation_store::ReservationId,
    utils::id::{CoAllocationDependencyId, CoAllocationId, DataDependencyId, WorkflowNodeId},
};

/// Represents an edge for data transfer (file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataDependency {
    /// Reference to the actual data in ReservationStore
    pub reservation_id: ReservationId,

    /// Key to Workflow.nodes, which is the sender.
    pub source_node: Option<WorkflowNodeId>,

    /// Key to Workflow.nodes, which is the receiver.
    pub target_node: Option<WorkflowNodeId>,

    /// The TCP port to be used for communication, i.e.,
    /// the Port to be opened by the VRM. If the VRM
    /// component is behind a NAT firewall,
    pub port_name: String,

    /// Size of the file for transport.
    pub size: i64,
}

/// Represents an edge for synchronous bandwidth (e.g. Co-allocated Communication).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDependency {
    /// Reference to the actual data in ReservationStore
    pub reservation_id: ReservationId,

    /// Key to Workflow.nodes, which is the sender.
    pub source_node: Option<WorkflowNodeId>,

    /// Key to Workflow.nodes, which is the receiver.
    pub target_node: Option<WorkflowNodeId>,

    /// The TCP port to be used for communication, i.e.,
    /// the Port to be opened by the VRM. If the VRM
    /// component is behind a NAT firewall,
    pub port_name: String,

    /// Bandwidth in MB's
    pub bandwidth: i64,
}

/// An edge in the "CoAllocations graph" connecting sync groups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoAllocationDependency {
    // Underlying DataDependency ID
    pub id: CoAllocationDependencyId,

    /// Key to Workflow.co_allocations, which is the sender.
    pub source_group: CoAllocationId,

    /// Key to Workflow.co_allocations, which is the receiver.
    pub target_group: CoAllocationId,

    /// Key to the DataDependency that this CoAllocation edge represents.
    pub data_dependency: DataDependencyId,
}
