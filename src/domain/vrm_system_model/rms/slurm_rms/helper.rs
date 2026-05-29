use std::collections::HashMap;

use crate::{
    api::rms_config_dto::rms_dto::SlurmRmsDto,
    domain::vrm_system_model::{
        rms::common::get_nodes_and_links,
        schedule::slotted_schedule::strategy::link::topology::{Link, Node},
        utils::id::{ResourceName, RouterId},
    },
};

use super::{api_client::response::nodes::SlurmNodesResponse, slurm_base::SlurmRms};

impl SlurmRms {
    pub fn get_nodes_and_links(dto: &SlurmRmsDto, nodes_response: &SlurmNodesResponse) -> (Vec<Node>, Vec<Link>) {
        let (_, links, node_to_switches) = get_nodes_and_links(dto.topology.clone());
        let mut nodes = Vec::new();

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
