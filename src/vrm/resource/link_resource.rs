use std::any::Any;

use crate::vrm::schedule::slotted_schedule::slotted_schedule_context::SlottedScheduleContext;
use crate::vrm::schedule::slotted_schedule::strategy::node::node_strategy::NodeStrategy;
use crate::vrm::common::id::{ResourceName, RouterId};

use super::resource_trait::{FeasibilityRequest, Resource};
use super::resources::BaseResource;

#[derive(Debug, Clone)]
pub struct LinkResource {
    pub base: BaseResource,
    pub source: RouterId,
    pub target: RouterId,

    /// The schedule manages bandwidth for this link.
    pub schedule: SlottedScheduleContext<NodeStrategy>,
}

impl LinkResource {
    pub fn new(name: ResourceName, source: RouterId, target: RouterId, capacity: i64, schedule: SlottedScheduleContext<NodeStrategy>) -> Self {
        let base = BaseResource::new(name, capacity);

        Self { base, source, target, schedule }
    }
}

impl Resource for LinkResource {
    fn get_capacity(&self) -> i64 {
        self.base.capacity
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_name(&self) -> ResourceName {
        self.base.get_name()
    }

    fn can_handle_request(&self, request: &FeasibilityRequest) -> bool {
        match request {
            FeasibilityRequest::Link { source, target, capacity, is_moldable } => {
                // Links check topology AND capacity
                log::debug!("LinkResource check with {:?} source: {:?}, target: {:?}", self.base.name, self.source, self.target);
                if source.compare(&self.source) && target.compare(&self.target) {
                    return self.base.can_handle(*is_moldable, *capacity);
                } else {
                    return false;
                }
            }
            _ => false, // A Link cannot handle a Node request
        }
    }
}
