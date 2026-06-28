use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::vrm::commons::id::{CoAllocationId, WorkflowNodeId};
use crate::vrm::reservation::reservation_store::ReservationStore;

use super::dependency::{CoAllocationDependency, DataDependency, SyncDependency};
use super::workflow_node::WorkflowNode;

/// A CoAllocation is a set of one or more compute tasks (WorkflowNodes) that must be scheduled to run at the exact same time (called "co-allocation" or "gang scheduling.").
/// A CoAllocation is formed by any WorkflowNodes that are linked, directly or indirectly, by a SyncDependency.
/// (e.g. We have the following three SyncDependencies A -> B        B -> C      D -> E => CoAllocation(A,B,C) and CoAllocation(D,E)
///
/// **Note**: The scheduler later schedules **not** the individual WorkflowNodes, it schedules the **CoAllocation** as one unit.
/// Therefore, when the scheduler reserves resources for a CoAllocation, it must find a time slot where all member nodes (e.g. A, B, and C) can run simultaneously.
// The CoAllocation's assigned_start time becomes the assigned_start time for all its member nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoAllocation {
    pub id: CoAllocationId,

    /// WorkflowNode, which is representing all WorkflowNodes in this sync group
    pub representative: Option<WorkflowNode>,

    /// Keys to Workflow.nodes, which are part of this CoAllocation.
    pub members: Vec<WorkflowNodeId>,

    /// SyncDependencies connecting WorkflowNodes of this CoAllocation.
    pub sync_dependencies: Vec<SyncDependency>,
    pub outgoing_co_allocation_dependencies: Vec<CoAllocationDependency>,
    pub outgoing_data_dependencies: Vec<DataDependency>,

    pub incoming_co_allocation_dependencies: Vec<CoAllocationDependency>,
    pub incoming_data_dependencies: Vec<DataDependency>,

    /// Upward Rank for the CoAllocation node. That is the length of the longest path through the Workflow,
    /// starting with the CoAllocation node and ending at an exit node. The DataDependencies between nodes are considered,
    /// using the average communication speed of the used network.
    /// rank_u(n_i) = w_i + max(n_j elem successor(n_i))(c_ij + rank_u(n_j)) where
    /// n elem node
    /// w_i computation time of node i
    /// c_ij average data transfer time from n_i to n_j  
    pub rank_upward: i64,

    /// Downward Rank for this OverlayNode. That is the length of the longest path through the Workflow,
    /// starting at an entry node and ending at the Overlay node. The DataDependencies between nodes are considered,
    /// using the average communication speed of the used network.
    /// rank_d(n_i) = max(n_j elem pred(n_i))(rank_d(n_j) + w_j + c_ji) where
    /// n elem node
    /// w_j computation time of node j
    /// c_ji average data transfer time from n_j to n_i  
    pub rank_downward: i64,

    /// Number of nodes on the critical path from an entry node to this node.
    pub number_of_nodes_critical_path_downwards: i64,

    /// Number of nodes on the critical path from this node to the exit node, including this node.
    pub number_of_nodes_critical_path_upwards: i64,

    // Temporary calculation values (internal state)
    /// Temporary value for topological order calculation (isInQueue).
    pub is_in_queue: bool,

    /// Temporary value for topological order calculation (unprocessedPredecessors).
    pub unprocessed_predecessor_count: i64,

    /// Temporary value for topological order calculation (unprocessed_successorCount).
    pub unprocessed_successor_count: i64,

    /// Spare time distributed temporary to this CoAllocation during calculation of booking interval.
    pub spare_time: i64,

    // FRAG-WINDOW Scheduling forces/properties
    /// Force for deadline positioning during FRAG-WINDOW scheduling algorithm.
    pub max_successor_force: f64,

    /// Force for deadline positioning during FRAG-WINDOW scheduling algorithm.
    pub max_pred_force: f64,

    // Search flags
    /// Mark flag which can be used by search algorithms.
    pub is_discovered: bool,

    /// Mark flag which can be used by search algorithms.
    pub is_processed: bool,
    pub is_moveable: bool,
    pub is_moveable_interval_start: bool,
    pub is_moveable_interval_end: bool,
    pub start_position: f64,
    pub end_position: f64,
}

impl CoAllocation {
    pub fn get_co_allocation_duration(&self, nodes: &HashMap<WorkflowNodeId, WorkflowNode>, store: &ReservationStore) -> i64 {
        let mut max_duration: i64 = 0;

        for node_key in &self.members {
            if let Some(member) = nodes.get(node_key) {
                let duration = store.get_task_duration(member.reservation_id);
                if duration > max_duration {
                    max_duration = duration;
                }
            } else {
                log::warn!("Warning: Node key '{}' not found in nodes map.", node_key);
            }
        }
        return max_duration;
    }
}
