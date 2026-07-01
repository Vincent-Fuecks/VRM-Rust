use crate::vrm::common::id::{ComponentId, ResourceName, RouterId, SlottedScheduleId};
use crate::vrm::global_clock::global_clock::GlobalClock;
use crate::vrm::reservation::reservation_store::ReservationStore;
use crate::vrm::resource::link_resource::LinkResource;
use crate::vrm::resource::resource_store::{LinkResourceId, ResourceStore};
use crate::vrm::schedule::slotted_schedule::SlottedNodeSchedule;
use crate::vrm::schedule::slotted_schedule::strategy::node::node_strategy::NodeStrategy;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// The number of k shortest paths to calculate and cache between any two grid access points.
const K_NUMBER_OF_PATHS: usize = 10;

pub struct Link {
    pub id: ResourceName,
    pub source: RouterId,
    pub target: RouterId,
    pub capacity: i64,
}

pub struct Node {
    pub name: ResourceName,
    pub cpus: i64,
    pub connected_to_router: Vec<RouterId>,
}

/// Represents a physical router within the grid network.
#[derive(Debug, Clone)]
pub struct Router {
    /// The unique identifier for the router.
    pub id: RouterId,

    /// Indicates whether this router serves as an entry/exit point for resources.
    ///
    /// If `true`, this router is directly connected to a Grid Node (Resource), making it
    /// a potential source for `VirtualLinkResource` calculations.
    pub is_grid_access_point: bool,
}

/// Represents a specific route through the network, consisting of a sequence of network links.
///
/// A `Path` is the physical realization of a connection between two routers. It is
/// composed of a vector of `LinkResourceId`s that must be traversed in order.
#[derive(Debug, Clone)]
pub struct Path {
    pub network_links: Vec<LinkResourceId>,
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

impl Path {
    pub fn new() -> Self {
        Self { network_links: Vec::new() }
    }
}

/// Represents the calculated virtual resource capability between two endpoints.
///
/// Unlike a physical link, a `VirtualLinkResource` represents the aggregated potential
/// connectivity between two Grid Access Points. It summarizes the capacity available
/// across the calculated K-shortest paths.
#[derive(Debug, Clone)]
pub struct VirtualLinkResource {
    /// Represents source Router form which the link starts
    pub source_router: RouterId,

    /// Represents the target, where the link leads.
    pub target_router: RouterId,

    /// The aggregated sum of the minimum bottleneck capacity of the k-shortest paths.
    /// This value represents the total theoretical throughput if all K paths were used simultaneously
    pub capacity: i64,

    /// The average bandwidth capacity across the calculated k-shortest paths.
    pub avg_bandwidth: i64,
}

/// Models the complete grid network topology.
///
/// The `NetworkTopology` acts as the graph representation of the system. It manages:
/// * **Physical Layer**: Routers and Network Links.
/// * **Connectivity**: Adjacency matrices defining how routers connect.
/// * **Routing Logic**: Caching of K-shortest paths and calculation of virtual resources.
/// * **Heuristics**: Importance databases for link weighting.
#[derive(Debug, Clone)]
pub struct NetworkTopology {
    /// A map of all routers in the system, indexed by their ID.
    routers: HashMap<RouterId, Router>,

    /// A map of all physical network links, indexed by their ID.
    pub link_ids: HashSet<LinkResourceId>,

    /// The adjacency list representing the graph structure.
    /// Maps a `RouterId` to a set of outgoing `LinkResourceId`s, enabling efficient graph traversal.
    adjacency: HashMap<RouterId, HashSet<LinkResourceId>>,

    /// A cache storing the calculated K-shortest paths between pairs of routers.
    pub path_cache: HashMap<(RouterId, RouterId), Vec<Path>>,

    /// Stores the "virtual" resources created for endpoint pairs
    pub virtual_link_resources: Vec<VirtualLinkResource>,

    /// Tracks maximum bandwidth across all calculated paths (highest bottleneck bandwidth on all the found paths)
    pub max_bandwidth_all_paths: i64,

    pub resource_store: ResourceStore,
}

impl NetworkTopology {
    pub fn new(
        links: &Vec<Link>,
        nodes: &Vec<Node>,
        slot_width: i64,
        num_of_slots: i64,
        simulator: Arc<GlobalClock>,
        aci_id: ComponentId,
        reservation_store: ReservationStore,
        resource_store: ResourceStore,
    ) -> Self {
        // 1.  Init physical links.
        let link_ids = NetworkTopology::setup_network_links(
            links,
            num_of_slots,
            slot_width,
            aci_id,
            simulator.clone(),
            reservation_store,
            resource_store.clone(),
        );

        // 2.  Init router instances based on grid nodes and network link endpoints.
        let routers: HashMap<RouterId, Router> = NetworkTopology::setup_routers(nodes, links);
        let router_ids: Vec<RouterId> = routers.keys().cloned().collect();
        resource_store.add_routers(router_ids);

        // 3. Build the adjacency matrix
        let adjacency: HashMap<RouterId, HashSet<LinkResourceId>> =
            NetworkTopology::setup_adjacency_matrix(&link_ids, &routers, resource_store.clone());

        let mut topology = NetworkTopology {
            routers,
            link_ids,
            adjacency,
            path_cache: HashMap::new(),
            virtual_link_resources: Vec::new(),
            max_bandwidth_all_paths: -1,
            resource_store,
        };

        // 4.  Pre-calculating all K-shortest paths between Grid Access Points.
        topology.calc_all_paths();

        topology
    }

    /// Calculates the K-shortest paths between the source and target router using a Breadth-First Search (BFS) approach.
    /// # Returns
    ///
    /// Returns `Some(VirtualLinkResource)` if at least one path is found, otherwise `None`.
    fn calc_k_shortest_paths(&mut self, source_router: Router, target_router: Router) -> Option<VirtualLinkResource> {
        let mut found_solutions = Vec::new();
        let mut queue: VecDeque<Path> = VecDeque::new();

        // Initialize queue with all outgoing network links from source
        if let Some(outgoing_links) = self.adjacency.get(&source_router.id) {
            for link_id in outgoing_links {
                if self.link_ids.contains(link_id) {
                    let mut p = Path::new();
                    p.network_links.push(*link_id);
                    queue.push_back(p);
                }
            }
        }

        while let Some(current_path) = queue.pop_front() {
            let current_last_network_link = current_path.network_links.last().expect("Path should not be empty");

            let last_link_id = self.link_ids.get(current_last_network_link).expect("Network Link should exist.");

            let current_target_router_id = self.resource_store.get_target(*last_link_id);

            if current_target_router_id.eq(&target_router.id) {
                found_solutions.push(current_path);

                if found_solutions.len() >= K_NUMBER_OF_PATHS {
                    break;
                }
            } else if self.adjacency.contains_key(&current_target_router_id) {
                for outgoing_link_id in self.adjacency.get(&current_target_router_id).unwrap().clone().iter() {
                    let outgoing_link_target_id = self.resource_store.get_target(*outgoing_link_id);

                    let mut is_loop: bool = false;

                    for old_part_id in &current_path.network_links {
                        let old_part_source_id = self.resource_store.get_source(*old_part_id);

                        if old_part_source_id == outgoing_link_target_id {
                            is_loop = true;
                            break;
                        }
                    }

                    if !is_loop {
                        let mut new_path = current_path.clone();
                        new_path.network_links.push(*outgoing_link_id);
                        queue.push_back(new_path);
                    }
                }
            }
        }

        if found_solutions.is_empty() {
            log::debug!("NoPathFound: {} => {}", source_router.id, target_router.id);
            return None;
        }

        self.path_cache.insert((source_router.id.clone(), target_router.id.clone()), found_solutions.clone());

        let mut total_bw: i64 = 0;

        for solution in &found_solutions {
            let mut bandwidth_bottleneck = i64::MAX;

            // Find bottleneck (min capacity) of this path
            for link_id in &solution.network_links {
                let link_capacity = self.resource_store.get_capacity(*link_id);

                if link_capacity < bandwidth_bottleneck {
                    bandwidth_bottleneck = link_capacity;
                }
            }

            // Update global max bandwidth tracking
            if bandwidth_bottleneck > self.max_bandwidth_all_paths {
                self.max_bandwidth_all_paths = bandwidth_bottleneck;
            }

            total_bw += bandwidth_bottleneck;
        }

        log::debug!("Paths found {} => {}: {} solutions, Max/Sum BW: {}", source_router.id, target_router.id, found_solutions.len(), total_bw);

        Some(VirtualLinkResource {
            source_router: source_router.id.clone(),
            target_router: target_router.id.clone(),
            capacity: total_bw,
            avg_bandwidth: total_bw / found_solutions.len() as i64,
        })
    }

    /// Performs preprocessing to identify and cache all paths between Grid Access Points.
    /// This iterates through all router pairs. If both routers are marked as `is_grid_access_point`,
    /// it invokes `calc_k_shortest_paths` and stores the resulting `VirtualLinkResource`.
    ///
    /// Attention: Updates Paths in ReservationStore
    pub fn calc_all_paths(&mut self) {
        let router_ids: Vec<RouterId> = self.routers.keys().cloned().collect();

        for source_id in &router_ids {
            let source_router = self.routers.get(source_id).unwrap().clone();
            if !source_router.is_grid_access_point {
                continue;
            }

            for target_id in &router_ids {
                let target_router = self.routers.get(target_id).unwrap().clone();
                if !target_router.is_grid_access_point || source_id.eq(target_id) {
                    continue;
                }

                log::debug!("Searching paths: Source Router Id: {} -> Target Router Id: {}", source_id, target_id);

                if let Some(virtual_link) = self.calc_k_shortest_paths(source_router.clone(), target_router.clone()) {
                    self.virtual_link_resources.push(virtual_link);
                }
            }
        }
        // Adds calculated Paths to ReservationStore
        self.resource_store.add_k_shortest_paths(self.path_cache.clone());
    }

    /// Constructs the adjacency matrix for the network graph.
    fn setup_adjacency_matrix(
        link_ids: &HashSet<LinkResourceId>,
        routers: &HashMap<RouterId, Router>,
        resource_store: ResourceStore,
    ) -> HashMap<RouterId, HashSet<LinkResourceId>> {
        let mut adjacency: HashMap<RouterId, HashSet<LinkResourceId>> = HashMap::new();

        for link_id in link_ids {
            let source: RouterId = resource_store.get_source(*link_id);
            let target: RouterId = resource_store.get_target(*link_id);

            let mut source_found: bool = false;
            let mut target_found: bool = false;

            for (router_id, _) in routers.iter() {
                if router_id.eq(&source) {
                    source_found = true;

                    adjacency.entry(source.clone()).or_default().insert(*link_id);

                    if target_found {
                        break;
                    }
                }

                if router_id.eq(&target) {
                    target_found = true;

                    if source_found {
                        break;
                    }
                }
            }

            if !source_found {
                log::error!("InValidLinkNetworkConfiguration: The Source: {} was not found.", source)
            }

            if !target_found {
                log::error!("InValidLinkNetworkConfiguration: The Target: {} was not found.", target)
            }
        }

        adjacency
    }

    /// Derives the set of all Routers from the DTO configurations (GridNodes, LinkResources).
    fn setup_routers(nodes: &Vec<Node>, links: &Vec<Link>) -> HashMap<RouterId, Router> {
        let mut routers: HashMap<RouterId, Router> = HashMap::new();

        // Register leaf nodes (Grid Nodes) as the grid access points.
        for grid_node in nodes.iter() {
            let node_id = RouterId::new(grid_node.name.clone());
            routers.insert(node_id.clone(), Router { id: node_id, is_grid_access_point: true });
        }

        // Add remaining topological link nodes/routers without grid access point status.
        for network_link in links.iter() {
            let router_end_point_id = RouterId::new(network_link.target.clone());
            let router_start_point_id = RouterId::new(network_link.source.clone());

            if !routers.contains_key(&router_end_point_id) {
                routers.insert(router_end_point_id.clone(), Router { id: router_end_point_id, is_grid_access_point: false });
            }

            if !routers.contains_key(&router_start_point_id) {
                routers.insert(router_start_point_id.clone(), Router { id: router_start_point_id, is_grid_access_point: false });
            }
        }

        routers
    }

    /// Initializes all `LinkResource` structs and the importance database.
    fn setup_network_links(
        links: &Vec<Link>,
        num_of_slots: i64,
        slot_width: i64,
        component_id: ComponentId,
        simulator: Arc<GlobalClock>,
        reservation_store: ReservationStore,
        resource_store: ResourceStore,
    ) -> HashSet<LinkResourceId> {
        let mut links_ids: HashSet<LinkResourceId> = HashSet::new();
        for link in links.iter() {
            let link_schedule_name = format!("Schedule LinkResource {} -> {}", link.source, link.target);
            let node_strategy = NodeStrategy::default();

            let link_schedule = SlottedNodeSchedule::new(
                SlottedScheduleId::new(link_schedule_name),
                num_of_slots,
                slot_width,
                link.capacity,
                true,
                node_strategy,
                reservation_store.clone(),
                simulator.clone(),
            );

            let link_resource_name = ResourceName::new(link.id.clone());
            let link_resource = LinkResource::new(
                link_resource_name,
                RouterId::new(link.source.clone()),
                RouterId::new(link.target.clone()),
                link.capacity,
                link_schedule,
            );

            links_ids.insert(resource_store.add_link(link_resource));
        }

        if links_ids.is_empty() {
            log::info!(
                "Empty Network Cluster: The newly created Rms Network of AcI {} contains no Network. RmsNodeSimulator should be utilized instead.",
                component_id
            );
        }
        links_ids
    }
}
