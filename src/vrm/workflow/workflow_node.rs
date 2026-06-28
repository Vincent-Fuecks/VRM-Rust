use serde::{Deserialize, Serialize};

use crate::vrm::commons::id::{CoAllocationId, DataDependencyId, SyncDependencyId};
use crate::vrm::reservation::reservation_store::{ReservationId, ReservationStore};

use super::workflow::Workflow;

/// Represents a node in the workflow graph (a computation task).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub reservation_id: ReservationId,

    /// Graph structure: Keys to the Workflow's HashMaps
    pub incoming_data: Vec<DataDependencyId>,
    pub outgoing_data: Vec<DataDependencyId>,
    pub incoming_sync: Vec<SyncDependencyId>,
    pub outgoing_sync: Vec<SyncDependencyId>,

    /// Key of the Workflow.co_allocations HashMap.
    /// HashMap contains all other nodes in the same sync group, including this node.
    pub co_allocation_key: Option<CoAllocationId>,
}

impl WorkflowNode {
    /// Updates the Workflow's assigned start/end based on this node's current state in the store.
    pub fn update_reservation(&self, workflow: &mut Workflow, reservation_store: &ReservationStore) {
        let assigned_start = reservation_store.get_assigned_start(self.reservation_id);
        let assigned_end = reservation_store.get_assigned_end(self.reservation_id);

        if workflow.base.assigned_start == i64::MIN || (assigned_start < workflow.base.assigned_start && assigned_start != i64::MIN) {
            workflow.base.set_assigned_start(assigned_start);
        }

        if workflow.base.assigned_end == i64::MIN || assigned_end > workflow.base.assigned_end {
            workflow.base.set_assigned_end(assigned_end);
        }
    }
}
