use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use parking_lot::RawRwLock;
use parking_lot::lock_api::RwLock;

use crate::api::rms_config_dto::rms_dto::{ComputeNodeDto, SwitchDto};
use crate::domain::simulator::simulator::GlobalClock;
use crate::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use crate::domain::vrm_system_model::resource::node_resource::NodeResource;
use crate::domain::vrm_system_model::resource::resource_store::ResourceStore;
use crate::domain::vrm_system_model::schedule::schedule_trait::Schedule;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::link::topology::{Link, NetworkTopology, Node};
use crate::domain::vrm_system_model::scheduler_type::{ScheduleContext, SchedulerType};
use crate::domain::vrm_system_model::utils::id::{ComponentId, ResourceName, RouterId, SlottedScheduleId};

use super::rms::RmsBase;

pub struct RmsSetupContext {
    num_of_slots: i64,
    slot_width: i64,
    reservation_store: ReservationStore,
    simulator: Arc<GlobalClock>,
    nodes: Vec<Node>,
    links: Vec<Link>,
    component_id: ComponentId,
    scheduler_typ: String,
    schedule_name: String,
    resource_store: ResourceStore,
    entry_points: HashSet<RouterId>,
}

impl RmsSetupContext {
    pub fn new(
        num_of_slots: i64,
        slot_width: i64,
        reservation_store: ReservationStore,
        simulator: Arc<GlobalClock>,
        nodes: Vec<Node>,
        links: Vec<Link>,
        component_id: ComponentId,
        scheduler_typ: String,
        schedule_name: String,
        resource_store: ResourceStore,
        entry_points: HashSet<RouterId>,
    ) -> Self {
        RmsSetupContext {
            num_of_slots,
            slot_width,
            reservation_store,
            simulator,
            nodes,
            links,
            component_id,
            scheduler_typ,
            schedule_name,
            resource_store,
            entry_points,
        }
    }

    pub fn get_node_schedule(&self) -> Result<Arc<RwLock<RawRwLock, Box<dyn Schedule>>>, Box<dyn std::error::Error>> {
        let resource_store = ResourceStore::new();

        // Setup Node Schedule
        let mut schedule_capacity = 0;

        // Add nodes to ResourceStore
        for node in self.nodes.iter() {
            schedule_capacity += node.cpus;
            resource_store.add_node(NodeResource::new(node.name.clone(), node.cpus));
        }

        let schedule_context = ScheduleContext {
            id: SlottedScheduleId::new(self.schedule_name.clone()),
            number_of_slots: self.num_of_slots,
            slot_width: self.slot_width,
            capacity: schedule_capacity,
            simulator: self.simulator.clone(),
            reservation_store: self.reservation_store.clone(),
        };

        let scheduler_type = SchedulerType::from_str(&self.scheduler_typ)?;
        let node_schedule = Arc::new(RwLock::new(scheduler_type.get_instance(schedule_context)));
        Ok(node_schedule)
    }

    pub fn get_network_schedule(&self) -> Result<Arc<RwLock<RawRwLock, Box<dyn Schedule>>>, Box<dyn std::error::Error>> {
        // Setup Network Schedule
        // Adds Links to Resource Store
        let topology = NetworkTopology::new(
            &self.links,
            &self.nodes,
            self.slot_width,
            self.num_of_slots,
            self.simulator.clone(),
            self.component_id.clone(),
            self.reservation_store.clone(),
            self.resource_store.clone(),
            self.entry_points.clone(),
        );

        let schedule_context = ScheduleContext {
            id: SlottedScheduleId::new(self.schedule_name.clone()),
            number_of_slots: self.num_of_slots,
            slot_width: self.slot_width,
            capacity: i64::MAX,
            simulator: self.simulator.clone(),
            reservation_store: self.reservation_store.clone(),
        };

        let mut scheduler_type = SchedulerType::from_str(&self.scheduler_typ)?;
        scheduler_type = scheduler_type.get_network_scheduler_variant(topology, self.resource_store.clone());
        let network_schedule = Arc::new(RwLock::new(scheduler_type.get_instance(schedule_context)));

        Ok(network_schedule)
    }

        pub fn get_base(&self) -> Result<RmsBase, Box<dyn std::error::Error>> {
        Ok(RmsBase { id: , resource_store: self.resource_store, reservation_store: self.reservation_store })
    }
}

/// Used for RmsSimulator and SlurmRms
pub fn get_nodes_and_links(
    topology: Vec<SwitchDto>,
    compute_node_dtos: Option<Vec<ComputeNodeDto>>,
) -> (Vec<Node>, Vec<Link>, HashMap<ResourceName, Vec<RouterId>>) {
    let mut links = Vec::new();
    let mut nodes = Vec::new();
    let mut node_to_switches: HashMap<ResourceName, Vec<RouterId>> = HashMap::new();

    for start_switch in &topology {
        let switch0 = RouterId::new(start_switch.switch_name.clone());
        for end_switch in &start_switch.switches {
            let switch1 = RouterId::new(end_switch.clone());
            // links are Bidirectional
            let link = Link {
                id: ResourceName::new(format!("{}->{}", switch0, switch1)),
                source: switch0.clone(),
                target: switch1.clone(),
                capacity: start_switch.link_speed,
            };
            links.push(link);

            let link = Link {
                id: ResourceName::new(format!("{}->{}", switch1, switch0.clone())),
                source: switch1.clone(),
                target: switch0.clone(),
                capacity: start_switch.link_speed,
            };
            links.push(link);
        }

        for node in &start_switch.nodes {
            // From switch to node
            let link = Link {
                id: ResourceName::new(format!("{}->{}", switch0.clone(), node.clone())),
                source: switch0.clone(),
                target: RouterId::new(node.clone()),
                capacity: start_switch.link_speed,
            };
            links.push(link);

            // From node to switch
            let link = Link {
                id: ResourceName::new(format!("{}->{}", node.clone(), switch0.clone())),
                source: RouterId::new(node.clone()),
                target: switch0.clone(),
                capacity: start_switch.link_speed,
            };
            links.push(link);

            // Add node, specific values are later initialized
            let node = Node { name: ResourceName::new(node.clone()), connected_to_router: vec![switch0.clone()], cpus: -1 };
            nodes.push(node);
        }

        let node_ids: Vec<ResourceName> = start_switch.nodes.iter().map(|node_id| ResourceName::new(node_id)).collect();

        for node_id in node_ids {
            node_to_switches.entry(node_id).or_insert_with(Vec::new).push(switch0.clone().cast());
        }
    }

    if let Some(compute_node_dtos) = compute_node_dtos {
        let mut compute_nodes_map = HashMap::new();
        for compute_node_dto in compute_node_dtos.iter() {
            compute_nodes_map.insert(ResourceName::new(compute_node_dto.id.clone()), ComputeNodeResources { cpus: compute_node_dto.cpus });
        }

        for node in nodes.iter_mut() {
            if let Some(compute_node_resources) = compute_nodes_map.get(&node.name) {
                node.cpus = compute_node_resources.cpus;
            } else {
                log::error!(
                    "VRM-JSON-TopologyAndComputeNodesContainsNotTheSameNodes: The topology node with the name {:?} is not in the ComputeNode List.",
                    node.name
                );
            }
        }
    }

    return (nodes, links, node_to_switches);
}

pub struct ComputeNodeResources {
    cpus: i64,
}

pub fn add_node_information(compute_node_dtos: Vec<ComputeNodeDto>, nodes: &mut Vec<Node>) {
    let mut compute_nodes_map = HashMap::new();
    for compute_node_dto in compute_node_dtos.iter() {
        compute_nodes_map.insert(ResourceName::new(compute_node_dto.id.clone()), ComputeNodeResources { cpus: compute_node_dto.cpus });
    }

    for node in nodes.iter_mut() {
        if let Some(compute_node_resources) = compute_nodes_map.get(&node.name) {
            node.cpus = compute_node_resources.cpus;
        } else {
            log::error!(
                "VRM-JSON-TopologyAndComputeNodesContainsNotTheSameNodes: The topology node with the name {:?} is not in the ComputeNode List.",
                node.name
            );
        }
    }
}
