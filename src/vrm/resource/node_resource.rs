use crate::vrm::common::id::ResourceName;

use std::any::Any;

use super::resource_trait::{FeasibilityRequest, Resource};
use super::resources::BaseResource;

#[derive(Debug, Clone)]
pub struct NodeResource {
    pub base: BaseResource,
}

impl NodeResource {
    pub fn new(name: ResourceName, capacity: i64) -> Self {
        let base = BaseResource::new(name, capacity);
        Self { base }
    }
}

impl Resource for NodeResource {
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
            FeasibilityRequest::Node { capacity, is_moldable } => {
                // Nodes only care about capacity and moldability
                self.base.can_handle(*is_moldable, *capacity)
            }
            _ => false, // A Node cannot handle a Link request
        }
    }
}
