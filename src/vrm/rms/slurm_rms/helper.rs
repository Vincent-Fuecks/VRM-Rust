use std::collections::HashMap;

use crate::{
    schema::rms_dto::SlurmRmsDto,
    vrm::common::id::{ResourceName, RouterId},
    vrm::schedule::slotted_schedule::strategy::link::topology::{Link, Node},
};

use super::{api_client::response::nodes::SlurmNodesResponse, slurm_base::SlurmRms};

impl SlurmRms {
    pub fn get_nodes_and_links(dto: &SlurmRmsDto, nodes_response: &SlurmNodesResponse) -> (Vec<Node>, Vec<Link>) {
        let mut links = Vec::new();
        let mut nodes = Vec::new();
        let mut node_to_switches: HashMap<ResourceName, Vec<RouterId>> = HashMap::new();

        for start_switch in &dto.topology.switches {
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
                let link = Link {
                    id: ResourceName::new(format!("{}->{}", switch0.clone(), node.clone())),
                    source: switch0.clone(),
                    target: RouterId::new(node.clone()),
                    capacity: start_switch.link_speed,
                };
                links.push(link);

                let link = Link {
                    id: ResourceName::new(format!("{}->{}", node.clone(), switch0.clone())),
                    source: RouterId::new(node.clone()),
                    target: switch0.clone(),
                    capacity: start_switch.link_speed,
                };
                links.push(link);
            }

            let node_ids: Vec<ResourceName> = start_switch.nodes.iter().map(|node_id| ResourceName::new(node_id)).collect();

            for node_id in node_ids {
                node_to_switches.entry(node_id).or_insert_with(Vec::new).push(switch0.clone().cast());
            }
        }

        for slurm_node in &nodes_response.nodes {
            let node_id = ResourceName::new(slurm_node.name.clone());

            if node_to_switches.contains_key(&node_id) {
                let node = Node {
                    name: ResourceName::new(node_id.clone()),
                    cpus: slurm_node.cpus as i64,
                    connected_to_router: node_to_switches.get(&node_id).unwrap().clone(),
                };

                nodes.push(node);
            } else if !node_to_switches.is_empty() {
                log::error!(
                    "SlurmNetworkConstructionError: The compute node {} of cluster {} was not found in the topology. Please check your submitted topology.",
                    slurm_node.name,
                    slurm_node.cluster_name
                );
            }
        }
        return (nodes, links);
    }
}
