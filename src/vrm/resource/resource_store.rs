use colored::Colorize;
use slotmap::{SlotMap, new_key_type};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use crate::vrm::{
    reservation::{
        reservation::{Reservation, ReservationTrait},
        reservation_store::{ReservationId, ReservationStore},
    },
    resource::resource_trait::Resource,
};

use crate::vrm::schedule::slotted_schedule::{
    slotted_schedule_context::SlottedScheduleContext,
    strategy::{link::topology::Path, node::node_strategy::NodeStrategy},
};

use crate::vrm::common::id::{ResourceName, RouterId};

use super::{link_resource::LinkResource, node_resource::NodeResource, resource_trait::FeasibilityRequest};

new_key_type! {
    pub struct NodeResourceId;
    pub struct LinkResourceId;
}

#[derive(Debug, Clone)]
pub struct ResourceStore {
    inner: Arc<RwLock<StoreInner>>,
}

#[derive(Debug, Default, Clone)]
struct StoreInner {
    nodes: SlotMap<NodeResourceId, Arc<RwLock<NodeResource>>>,
    links: SlotMap<LinkResourceId, Arc<RwLock<LinkResource>>>,
    k_shortest_paths: Arc<RwLock<HashMap<(RouterId, RouterId), Vec<Path>>>>,

    /// Index lookup InternalKey (NodeResourceId) using input reservation name (ResourceName).
    node_index: HashMap<ResourceName, NodeResourceId>,
    router_list: Arc<RwLock<HashSet<RouterId>>>,
}

impl Default for ResourceStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceStore {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(StoreInner::default())) }
    }

    //---------------------
    // --- Node Methods ---
    //---------------------
    pub fn add_node(&self, node: NodeResource) -> NodeResourceId {
        let mut guard = self.inner.write().unwrap();
        let node_resource_id = guard.nodes.insert(Arc::new(RwLock::new(node.clone())));
        guard.node_index.insert(node.base.get_name(), node_resource_id);
        node_resource_id
    }

    pub fn contains_node(&self, key: &ResourceName) -> bool {
        let guard = self.inner.read().unwrap();
        guard.node_index.contains_key(key)
    }

    pub fn remove_node(&self, resource_name: ResourceName) {
        let mut guard = self.inner.write().unwrap();
        let node_resource_id = guard.node_index.remove(&resource_name);

        if let Some(node_resource_id) = node_resource_id
            && guard.nodes.remove(node_resource_id).is_some()
        {
            return;
        }

        log::error!(
            "ReservationStoreRemoveOfNodeError: A failure occurred in the process of removing the node {:?} ({:?}).",
            resource_name,
            node_resource_id
        );
    }

    /// Synchronizes the local node state with the external RMS.
    ///
    /// This method ensures the **distributed resource view** remains consistent by updating
    /// differences between the `nodes_in_rms` and the current `ResourceStore`.
    /// * **New Nodes**: Any node present in the RMS but missing locally is added via `add_node`.
    /// * **Offline/Missing Nodes**: Any node existing locally that is no longer reported by the
    ///   RMS is treated as offline and purged via `remove_node`.
    ///
    /// ### Network Topology & Routing
    /// In modern high-performance computing (HPC) setups like **Fat-Tree topologies**, nodes function
    /// as leaves connected to external switches. Consequently, this synchronization does not trigger
    /// a topology recalculation.
    ///
    /// > **Important:** Nodes must be part of the initial topology to be accessible. If an RMS node
    /// > was not included during the system's initialization, it will remain unreachable even
    /// > after synchronization.
    ///
    /// ### Job Handling
    /// Committed jobs or active reservations on nodes that go offline are not handled within this
    /// function. Those lifecycle events are managed independently by the reservation logic
    /// (e.g., `slurm_base.rs` through `update_reservations`).
    pub fn update_nodes(&self, nodes_in_rms: Vec<NodeResource>) {
        let guard = self.inner.read().unwrap();
        let mut current_store_nodes = guard.node_index.clone();

        for node in nodes_in_rms {
            if current_store_nodes.remove(&node.base.get_name()).is_none() {
                self.add_node(node);
            }
        }

        for node_id in current_store_nodes.values() {
            let resource_id = guard.nodes.get(*node_id).unwrap().read().unwrap().base.get_name();
            self.remove_node(resource_id);
        }
    }

    pub fn get_node(&self, node_id: NodeResourceId) -> Option<Arc<RwLock<NodeResource>>> {
        let guard = self.inner.read().unwrap();
        guard.nodes.get(node_id).cloned()
    }

    pub fn get_total_node_capacity(&self) -> i64 {
        let guard = self.inner.read().unwrap();
        guard.nodes.values().map(|node| node.read().unwrap().get_capacity()).sum()
    }

    pub fn get_num_of_nodes(&self) -> i64 {
        let guard = self.inner.read().unwrap();
        guard.nodes.len() as i64
    }

    fn can_handle_node_request(&self, feasibility_request: &FeasibilityRequest) -> bool {
        let guard = self.inner.read().unwrap();

        for node in guard.nodes.values() {
            let node: std::sync::RwLockReadGuard<'_, NodeResource> = node.read().unwrap();

            if node.can_handle_request(feasibility_request) {
                log::debug!("Feasibility result is: {}", "TRUE".green().bold());
                return true;
            }
        }

        log::debug!("Feasibility result is: {}", "FALSE".red().bold());
        false
    }

    //---------------------
    // --- Link Methods ---
    //---------------------
    pub fn add_link(&self, link: LinkResource) -> LinkResourceId {
        let mut guard = self.inner.write().unwrap();
        guard.links.insert(Arc::new(RwLock::new(link)))
    }

    pub fn get_link(&self, link_id: LinkResourceId) -> Option<Arc<RwLock<LinkResource>>> {
        let guard = self.inner.read().unwrap();
        guard.links.get(link_id).cloned()
    }

    pub fn get_mut_link(&self, link_id: LinkResourceId) -> Option<Arc<RwLock<LinkResource>>> {
        let mut guard = self.inner.write().unwrap();
        guard.links.get_mut(link_id).cloned()
    }

    pub fn get_source(&self, link_id: LinkResourceId) -> RouterId {
        if let Some(handle) = self.get_link(link_id) {
            let link = handle.read().unwrap();
            link.source.clone()
        } else {
            panic!("LinkResource (id: {:?}) was not found in the ResourceStore.", link_id);
        }
    }

    pub fn get_target(&self, link_id: LinkResourceId) -> RouterId {
        if let Some(handle) = self.get_link(link_id) {
            let link = handle.read().unwrap();
            link.target.clone()
        } else {
            panic!("LinkResource (id: {:?}) was not found in the ResourceStore.", link_id);
        }
    }

    pub fn get_name(&self, link_id: LinkResourceId) -> ResourceName {
        if let Some(handle) = self.get_link(link_id) {
            let link = handle.read().unwrap();
            link.get_name()
        } else {
            panic!("LinkResource (id: {:?}) was not found in the ResourceStore.", link_id);
        }
    }

    pub fn get_capacity(&self, link_id: LinkResourceId) -> i64 {
        if let Some(handle) = self.get_link(link_id) {
            let link = handle.read().unwrap();
            link.get_capacity()
        } else {
            panic!("LinkResource (id: {:?}) was not found in the ResourceStore.", link_id);
        }
    }

    pub fn get_total_link_capacity(&self) -> i64 {
        let guard = self.inner.read().unwrap();
        guard.links.values().map(|link| link.read().unwrap().get_capacity()).sum()
    }

    pub fn get_num_of_links(&self) -> usize {
        let guard = self.inner.read().unwrap();
        guard.links.len()
    }

    pub fn with_mut_slotted_schedule_strategy<F, R>(&self, link_id: LinkResourceId, f: F) -> R
    where
        F: FnOnce(&mut SlottedScheduleContext<NodeStrategy>) -> R,
    {
        if let Some(handle) = self.get_mut_link(link_id) {
            let mut link = handle.write().unwrap();
            f(&mut link.schedule)
        } else {
            panic!("LinkResource {:?} not found", link_id);
        }
    }

    fn can_handle_link_request(&self, source: RouterId, target: RouterId, is_moldable: bool, capacity: i64) -> bool {
        // Early stop: same source and target
        if source.compare(&target) {
            log::debug!("Feasibility: both source and target are the same");
            log::debug!("Feasibility result is: {}", "TRUE".green().bold());
            return true;
        }

        // If no data needs to be transferred (capacity == 0), no paths are required.
        // This handles pre-scheduling feasibility checks where endpoints are workflow
        // node IDs that don't yet correspond to actual router IDs in the topology.
        if capacity <= 0 && !is_moldable {
            log::debug!("Feasibility: zero-capacity link requires no paths");
            log::debug!("Feasibility result is: {}", "TRUE".green().bold());
            return true;
        }

        // If neither endpoint is a known router, the endpoints are workflow-level
        // node IDs that will be resolved during scheduling. Defer to scheduling.
        if !self.contains_router_id(&source) && !self.contains_router_id(&target) {
            log::debug!("Feasibility: neither source nor target are known routers (pre-scheduling phase)");
            log::debug!("Feasibility result is: {}", "TRUE".green().bold());
            return true;
        }

        let paths = self.get_k_shortest_paths(source.clone(), target.clone()).unwrap_or_default();
        if paths.is_empty() {
            log::debug!("Feasibility result is: {} (No paths found)", "FALSE".red().bold());
            return false;
        }

        let guard = match self.inner.read() {
            Ok(g) => g,
            Err(_) => return false,
        };

        let mut is_path_valid;
        for shortest_path in paths {
            if shortest_path.network_links.is_empty() {
                continue;
            }

            is_path_valid = true;
            for link_resource_id in shortest_path.network_links {
                let link_lock = match guard.links.get(link_resource_id) {
                    Some(l) => l,
                    None => {
                        log::warn!("LinkResource ID {:?} not found in registry", link_resource_id);
                        is_path_valid = false;
                        break;
                    }
                };

                let link = match link_lock.read() {
                    Ok(l) => l,
                    Err(_) => {
                        log::error!("Link lock poisoned: {:?}", link_resource_id);
                        is_path_valid = false;
                        break;
                    }
                };

                if !link.can_handle_request(&FeasibilityRequest::Link {
                    source: link.source.clone(),
                    target: link.target.clone(),
                    capacity,
                    is_moldable,
                }) {
                    is_path_valid = false;
                    break;
                }
            }

            if is_path_valid {
                log::debug!("Feasibility result is: {}", "TRUE".green().bold());
                return true;
            }
        }

        log::debug!("Feasibility result is: {}", "FALSE".red().bold());
        false
    }

    //---------------------
    // --- Path Methods ---
    //---------------------

    pub fn add_k_shortest_paths(&self, k_shortest_paths: HashMap<(RouterId, RouterId), Vec<Path>>) {
        let mut guard = self.inner.write().unwrap();
        guard.k_shortest_paths = Arc::new(RwLock::new(k_shortest_paths));
    }

    pub fn get_k_shortest_paths(&self, source: RouterId, target: RouterId) -> Option<Vec<Path>> {
        let inner_guard = self.inner.read().unwrap();
        let map_guard = inner_guard.k_shortest_paths.read().unwrap();
        map_guard.get(&(source, target)).cloned()
    }

    pub fn contains_valid_path(&self, source: RouterId, target: RouterId) -> bool {
        let inner_guard = self.inner.read().unwrap();
        let map_guard = inner_guard.k_shortest_paths.read().unwrap();
        map_guard.get(&(source, target)).is_some()
    }

    pub fn dump_k_shortest_paths(&self) {
        let inner_guard = self.inner.read().unwrap();
        let map_guard = inner_guard.k_shortest_paths.read().unwrap();
        for (router_pair, paths) in map_guard.iter() {
            log::debug!("Router Pair: {:?}", router_pair.clone());
            for (i, path) in paths.iter().enumerate() {
                log::debug!("Path: {}", i);
                for link_id in &path.network_links {
                    log::debug!("{:?}", self.get_link(*link_id).unwrap().read().unwrap().base.name);
                }
                println!("\n");
            }
        }
    }

    //----------------------------
    // --- Aggregation Methods ---
    //----------------------------

    pub fn get_total_capacity(&self) -> i64 {
        self.get_total_link_capacity() + self.get_total_node_capacity()
    }

    /// Returns true if a resource can handle the reservation
    pub fn can_handle_adc_request(&self, res: Reservation) -> bool {
        log::debug!(
            "Start feasibility request for Reservation {:?}, type: {:?},  is_moldable: {:?}, reserved_capacity: {:?}",
            res.get_name(),
            res.get_type(),
            res.is_moldable(),
            res.get_reserved_capacity()
        );

        match res {
            Reservation::Link(link_reservation) => match (link_reservation.start_point.clone(), link_reservation.end_point.clone()) {
                (Some(source), Some(target)) => {
                    log::debug!("LinkReservation with source: {:?}, target: {:?}", source, target);

                    self.can_handle_link_request(source, target, link_reservation.is_moldable(), link_reservation.get_reserved_capacity())
                }

                (_, _) => {
                    log::debug!(
                        "Feasibility failed because both source ({:?}) and target ({:?}) must be Some",
                        link_reservation.start_point,
                        link_reservation.end_point
                    );
                    false
                }
            },

            Reservation::Node(node_reservation) => self.can_handle_node_request(&FeasibilityRequest::Node {
                capacity: node_reservation.get_reserved_capacity(),
                is_moldable: node_reservation.is_moldable(),
            }),

            Reservation::Workflow(_) => {
                log::error!(
                    "ERROR: Feasibility can only be checked for atomic task not for WorkflowReservations {:?}. The WorkflowScheduler should be utilized instead.",
                    res.get_name()
                );

                false
            }
        }
    }

    //----------------------------
    // --- Router-List Methods ---
    //----------------------------
    pub fn add_routers(&self, routers: Vec<RouterId>) {
        let guard = self.inner.write().unwrap();
        let mut router_list_guard = guard.router_list.write().unwrap();
        router_list_guard.extend(routers);
    }

    pub fn contains_router_id(&self, router_id: &RouterId) -> bool {
        let guard = self.inner.read().unwrap();
        return guard.router_list.read().unwrap().contains(router_id);
    }

    //----------------------
    // --- Other Methods ---
    //----------------------
    /// Returns true if a resource can handle the reservation
    pub fn can_handle_aci_request(&self, reservation_store: ReservationStore, reservation_id: ReservationId) -> bool {
        log::debug!(
            "Start feasibility request for Reservation {:?}, type: {:?}, is_moldable: {:?}, reserved_capacity: {:?}",
            reservation_store.get_name_for_key(reservation_id),
            reservation_store.get_type(reservation_id),
            reservation_store.is_moldable(reservation_id),
            reservation_store.get_reserved_capacity(reservation_id),
        );

        if reservation_store.is_link(reservation_id) {
            match (reservation_store.get_start_point(reservation_id), reservation_store.get_end_point(reservation_id)) {
                (Some(source), Some(target)) => {
                    log::debug!(
                        "LinkReservation with source: {:?}, target: {:?}",
                        reservation_store.get_start_point(reservation_id),
                        reservation_store.get_end_point(reservation_id)
                    );

                    self.can_handle_link_request(
                        source,
                        target,
                        reservation_store.is_moldable(reservation_id),
                        reservation_store.get_reserved_capacity(reservation_id),
                    )
                }

                (_, _) => {
                    log::debug!(
                        "Feasibility failed because both source ({:?}) and target ({:?}) must be Some",
                        reservation_store.get_start_point(reservation_id),
                        reservation_store.get_end_point(reservation_id)
                    );
                    false
                }
            }
        } else if reservation_store.is_node(reservation_id) {
            self.can_handle_node_request(&FeasibilityRequest::Node {
                capacity: reservation_store.get_reserved_capacity(reservation_id),
                is_moldable: reservation_store.is_moldable(reservation_id),
            })
        } else {
            log::error!(
                "ERROR: Feasibility can only be checked for atomic task not for WorkflowReservations {:?}. The WorkflowScheduler should be utilized instead.",
                reservation_store.get_name_for_key(reservation_id)
            );

            false
        }
    }

    pub fn dump_store_contents(&self) {
        let guard = self.inner.read().expect("RwLock poisoned");
        log::error!("=== RESOURCE STORE DUMP LinkResources ({} entries) ===", guard.links.values().len());

        for link in guard.links.values() {
            match link.try_read() {
                Ok(resource) => {
                    log::error!(
                        "Name: {:?}, Capacity: {:?}, Source: {:?}, Target: {:?}",
                        resource.base.name,
                        resource.base.capacity,
                        resource.source,
                        resource.target
                    );
                }
                Err(_) => {
                    log::error!("[Lock Busy/Deadlocked]");
                }
            }
        }

        log::error!("=== RESOURCE STORE DUMP NodeResources ({} entries) ===", guard.nodes.values().len());
        for node in guard.nodes.values() {
            match node.try_read() {
                Ok(resource) => {
                    log::error!("Name: {:?}, Capacity: {:?}", resource.base.name, resource.base.capacity);
                }
                Err(_) => {
                    log::error!("[Lock Busy/Deadlocked]");
                }
            }
        }
    }
}
