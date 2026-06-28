use crate::vrm::commons::id::{ResourceName, RouterId};

use std::any::Any;

pub trait Resource: std::fmt::Debug + Send {
    /// Returns the capacity
    fn get_capacity(&self) -> i64;

    /// Down casting into NodeResource or LinkResource
    fn as_any(&self) -> &dyn Any;

    /// Return the Id of the Resource
    fn get_name(&self) -> ResourceName;

    fn can_handle_request(&self, request: &FeasibilityRequest) -> bool;
}

pub enum FeasibilityRequest {
    Node { capacity: i64, is_moldable: bool },
    Link { source: RouterId, target: RouterId, capacity: i64, is_moldable: bool },
}
