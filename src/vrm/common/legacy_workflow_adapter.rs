use std::collections::HashSet;
use std::path::Path;

use crate::schema::workflow_dto::WorkflowDto;

pub struct LegacyWorkflowAdapter;

/// Transform a VRM-Rust WorkflowDto to a legacy VRM XML workflow.
impl LegacyWorkflowAdapter {
    pub fn to_xml(workflow: &WorkflowDto) -> String {
        let mut xml = String::new();

        xml.push_str("<?xml version=\"1.0\" encoding=\"iso-8859-1\"?>\n");
        xml.push_str("<VRM xmlns=\"http://vrm.gridworkflow.net/\" version=\"1.0\">\n");

        xml.push_str("  <SIMULATOR version=\"1.1\">\n    <ENDTIME>10000</ENDTIME>\n  </SIMULATOR>\n");
        xml.push_str("  <ADC version=\"1.2\" name=\"ADC\">\n    <RequestOrder>LOAD</RequestOrder>\n    <Timeout>2</Timeout>\n    <SlotWidth>120</SlotWidth>\n    <NumOfSlots>4000</NumOfSlots>\n  </ADC>\n");

        // Client and Workflow
        xml.push_str("  <CLIENT version=\"1.0\" name=\"GeneratedClient\">\n");
        xml.push_str("    <AdcName>ADC</AdcName>\n");
        xml.push_str(&format!("    <Workflow version=\"1.1\" id=\"{}\">\n", workflow.id));
        xml.push_str("      <JobState>OPEN</JobState>\n");

        xml.push_str("      <RequestProceeding>Commit</RequestProceeding>\n");
        xml.push_str(&format!("      <ArrivalTime>{}</ArrivalTime>\n", workflow.arrival_time));
        xml.push_str(&format!("      <StartTime>{}</StartTime>\n", workflow.booking_interval_start));
        xml.push_str(&format!("      <EndTime>{}</EndTime>\n", workflow.booking_interval_end));

        //  Identify which nodes are sources
        let mut source_nodes = HashSet::new();
        for task in &workflow.tasks {
            for data_dep in &task.node_reservation.dependencies.data {
                source_nodes.insert(data_dep.clone());
            }
        }

        // Map Tasks to NodeReservations
        for task in &workflow.tasks {
            xml.push_str(&format!("      <NodeReservation name=\"{}\" version=\"1.2\">\n", task.id));
            xml.push_str("        <JobState>OPEN</JobState>\n");
            xml.push_str("        <RequestProceeding>COMMIT</RequestProceeding>\n");

            // Sync Dependencies (e.g., <Dependency sync="job0" version="1.0"/>)
            // for sync_target in &task.node_reservation.dependencies.sync {
            //     xml.push_str(&format!("        <Dependency sync=\"{}\" version=\"1.0\"/>\n", sync_target));
            // }

            // Data Dependencies (Incoming)
            for data_source in &task.node_reservation.dependencies.data {
                xml.push_str(&format!(
                    "        <DataIn sourceReservation=\"{}\" sourcePort=\"port_{}\" file=\"input_{}.txt\" version=\"1.0\"/>\n",
                    data_source, data_source, task.id
                ));
            }

            xml.push_str(&format!("        <CPUS>{}</CPUS>\n", task.node_reservation.cpus));
            xml.push_str(&format!("        <Duration>{}</Duration>\n", task.node_reservation.duration));

            // Data Out
            if source_nodes.contains(&task.id) {
                xml.push_str(&format!("        <DataOut name=\"port_{}\" file=\"output_{}.txt\" size=\"1\" version=\"1.1\"/>\n", task.id, task.id));
            }

            xml.push_str("      </NodeReservation>\n");
        }

        xml.push_str("    </Workflow>\n");
        xml.push_str("  </CLIENT>\n");
        xml.push_str("</VRM>");

        xml
    }

    pub fn save_xml(workflow_xml_str: String, file_name: String) {
        let project_root = env!("CARGO_MANIFEST_DIR");
        let path = Path::new(project_root).join("src").join("data").join("generated_workflows").join(file_name);
        std::fs::write(path, workflow_xml_str).expect("Failed to write to .xml");
    }
}
