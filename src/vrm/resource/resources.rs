use std::collections::HashSet;

use crate::vrm::common::id::{ResourceName, RouterId};

use super::link_resource::LinkResource;
use super::node_resource::NodeResource;
use super::resource_trait::Resource;

#[derive(Debug, Clone)]
pub struct BaseResource {
    pub name: ResourceName,
    pub capacity: i64,
}

impl BaseResource {
    pub fn new(name: ResourceName, capacity: i64) -> Self {
        Self { name, capacity }
    }

    /// Check if this resource can handle the task theoretically, does not guarantee that the request an be handled.
    /// If reservation not moldable, resource must have the requested capacity.
    /// If reservation moldable or requested capacity is 0 true is returned.
    /// Otherwise false is returned.
    pub fn can_handle(&self, is_res_moldable: bool, res_reserved_capacity: i64) -> bool {
        log::debug!("Resource: {:?}, has capacity: {:?}", self.name, self.capacity);

        if !is_res_moldable && res_reserved_capacity > 0 {
            res_reserved_capacity <= self.capacity
        } else {
            true
        }
    }

    pub fn get_name(&self) -> ResourceName {
        self.name.clone()
    }
}

#[derive(Debug)]
pub struct Resources {
    inner: Vec<Box<dyn Resource>>,

    /// Router list contains all router of the rms.
    router_list: Vec<RouterId>,
}

impl Resources {
    pub fn new(inner: Vec<Box<dyn Resource>>, router_list: Vec<RouterId>) -> Self {
        Self { inner, router_list }
    }

    pub fn add(&mut self, resource: Box<dyn Resource>, router_list: HashSet<RouterId>) {
        self.inner.push(resource);
        self.router_list.extend(router_list);
    }

    pub fn get_total_capacity(&self) -> i64 {
        self.inner.iter().map(|r| r.get_capacity()).sum()
    }

    /// Returns the number of NodeResources in Resources
    pub fn get_node_resource_count(&self) -> usize {
        self.inner.iter().filter(|r| r.as_any().is::<NodeResource>()).count()
    }

    /// Returns the number of LinkResources in Resources
    pub fn get_link_resource_count(&self) -> usize {
        self.inner.iter().filter(|r| r.as_any().is::<LinkResource>()).count()
    }

    /// Returns the aggregated sum of the link resource capacities
    pub fn get_total_link_capacity(&self) -> i64 {
        self.inner.iter().filter_map(|r| r.as_any().downcast_ref::<LinkResource>()).map(|link| link.get_capacity()).sum()
    }

    /// Returns the aggregated sum of the node resource capacities
    pub fn get_total_node_capacity(&self) -> i64 {
        self.inner.iter().filter_map(|r| r.as_any().downcast_ref::<NodeResource>()).map(|node: &NodeResource| node.get_capacity()).sum()
    }

    /// Returns true, if provided RouterId exists in Router-List of RMS
    pub fn contains_router(&self, router_id: RouterId) -> bool {
        self.router_list.contains(&router_id)
    }

    /// Return the, the list of all RouterIds, which the Resource contains
    pub fn get_router_list(&self) -> Vec<RouterId> {
        self.router_list.clone()
    }
}
