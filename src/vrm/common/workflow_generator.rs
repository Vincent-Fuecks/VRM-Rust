use crate::schema::reservation_dto::{DependencyDto, LinkReservationDto, NodeReservationDto, ReservationProceedingDto, ReservationStateDto};
use crate::schema::workflow_dto::{TaskDto, WorkflowDto};

pub struct WorkflowGenerator {
    pub depth: usize,
    pub branching_degree: usize,
}
/// Generates a workflow, which is uses for benchmarking the VRM-Rust vs. the legacy system.
impl WorkflowGenerator {
    pub fn generate(&self, id: &str) -> WorkflowDto {
        let mut tasks = Vec::new();
        let mut levels: Vec<Vec<String>> = Vec::new();

        // id Generation
        for d in 0..self.depth {
            let level_char = ((b'A' + d as u8) as char).to_string();
            let mut current_level_ids = Vec::new();

            if d == 0 || d == self.depth - 1 {
                current_level_ids.push(level_char);
            } else {
                if let Some(prev_level) = levels.get(d - 1) {
                    for parent_id in prev_level {
                        for i in 0..self.branching_degree {
                            let suffix = &parent_id[1..];
                            current_level_ids.push(format!("{}{}{}", level_char, suffix, i));
                        }
                    }
                }
            }
            levels.push(current_level_ids);
        }

        // Create Tasks and Links
        for (d, current_ids) in levels.iter().enumerate() {
            for (i, task_id) in current_ids.iter().enumerate() {
                let mut links = Vec::new();
                let mut data_deps = Vec::new();
                let mut sync_deps = Vec::new();

                // Links data and sync
                if d < self.depth - 1 {
                    let next_level = &levels[d + 1];
                    if d + 1 == self.depth - 1 {
                        //Point to final node
                        links.push(self.create_link(task_id, &next_level[0]));
                    } else {
                        // Point to children
                        let start_idx = i * self.branching_degree;
                        for j in 0..self.branching_degree {
                            if let Some(target) = next_level.get(start_idx + j) {
                                links.push(self.create_link(task_id, target));
                            }
                        }
                    }
                }

                // Data dependencies
                if d > 0 {
                    let prev_level = &levels[d - 1];
                    if d == self.depth - 1 {
                        // Final node
                        data_deps = prev_level.clone();
                    } else {
                        let parent_idx = i / self.branching_degree;
                        if let Some(parent) = prev_level.get(parent_idx) {
                            data_deps.push(parent.clone());
                        }
                    }
                }

                // sync dependencies
                if d > 0 && d < self.depth - 1 {
                    let parent_idx = i / self.branching_degree;
                    let group_start = parent_idx * self.branching_degree;

                    for sibling_idx in group_start..(group_start + self.branching_degree) {
                        if sibling_idx != i {
                            // Don't sync with yourself
                            if let Some(sibling_id) = current_ids.get(sibling_idx) {
                                sync_deps.push(sibling_id.clone());
                            }
                        }
                    }
                }

                tasks.push(TaskDto {
                    id: task_id.clone(),
                    reservation_state: ReservationStateDto::Open,
                    request_proceeding: ReservationProceedingDto::Commit,
                    link_reservation: links,
                    node_reservation: self.create_default_node(data_deps, sync_deps),
                });
            }
        }

        WorkflowDto {
            id: id.to_string(),
            arrival_time: 0,
            booking_interval_start: 10,
            booking_interval_end: 1000000,
            tasks,
            request_proceeding: ReservationProceedingDto::Commit,
            reservation_state: ReservationStateDto::Open,
        }
    }

    fn create_link(&self, start: &str, end: &str) -> LinkReservationDto {
        LinkReservationDto { start_point: start.to_string(), end_point: end.to_string(), amount: Some(1), bandwidth: Some(10) }
    }

    fn create_default_node(&self, data_ids: Vec<String>, sync_ids: Vec<String>) -> NodeReservationDto {
        NodeReservationDto {
            task_path: "".to_string(),
            output_path: Some("/data/logs/sim.out".to_string()),
            error_path: Some("/data/logs/sim.err".to_string()),
            duration: 10,
            cpus: 5,
            is_moldable: true,
            current_working_directory: None,
            environment: None,
            dependencies: DependencyDto { data: data_ids, sync: sync_ids },
            data_out: vec![],
            data_in: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::BufWriter;
    use std::path::Path;

    use crate::vrm::common::legacy_workflow_adapter::LegacyWorkflowAdapter;
    use crate::schema::client_dto::{ClientDto, ClientsDto};

    use super::*;

    #[test]
    fn generate_workflow() {
        let workflow_depth = 9;
        let workflow_branching_degree = 4;
        let workflow_name = format!("Workflow-Depth-{:?}-Branching-Degree-{:?}", workflow_depth, workflow_branching_degree);
        let client_name = "Client-Tester".to_string();
        let file_name = format!("{}-{}.json", client_name, workflow_name);

        let workflow_generator = WorkflowGenerator { depth: workflow_depth, branching_degree: workflow_branching_degree };
        let workflow_dto = workflow_generator.generate("Test-Workflow");
        let client_dto = ClientDto { id: client_name.to_string(), workflows: vec![workflow_dto.clone()] };
        let clients_dto = ClientsDto { clients: vec![client_dto] };

        let project_root = env!("CARGO_MANIFEST_DIR");
        let path = Path::new(project_root).join("src").join("data").join("generated_workflows").join(file_name);

        let file = File::create(path).expect("Unable to create file");
        let writer = BufWriter::new(file);

        serde_json::to_writer(writer, &clients_dto).expect("Failed to write JSON");

        // Generate workflow for legacy system
        let workflow_xml_str = LegacyWorkflowAdapter::to_xml(&workflow_dto);
        let file_name = format!("{}-{}.xml", client_name, workflow_name);
        LegacyWorkflowAdapter::save_xml(workflow_xml_str, file_name);
    }
}
