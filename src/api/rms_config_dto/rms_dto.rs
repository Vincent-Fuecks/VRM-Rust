use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DummyRmsDto {
    pub typ: String,
    pub scheduler_typ: String,
    pub slot_width: i64,
    pub num_of_slots: i64,
    pub entry_points: Vec<String>,
    pub grid_nodes: Vec<GridNodeDto>,
    pub network_links: Vec<NetworkLinkDto>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RmsSimulatorDto {
    pub typ: String,
    pub scheduler_typ: String,
    pub slot_width: i64,
    pub num_of_slots: i64,
    pub entry_points: Vec<String>,
    pub compute_nodes: Vec<ComputeNodeDto>,
    pub topology: Vec<SwitchDto>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputeNodeDto {
    pub id: String,
    pub cpus: i64,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlurmRmsDto {
    pub id: String,
    pub scheduler_typ: String,
    pub slot_width: i64,
    pub num_of_slots: i64,
    pub rest_api_config: SlurmConfigDto,
    pub entry_points: Vec<String>,
    pub topology: Vec<SwitchDto>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlurmConfigDto {
    pub base_url: String,
    pub version: String,
    pub user_name: String,
    pub jwt_token: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum RmsSystemWrapper {
    DummyRms(DummyRmsDto),
    Slurm(SlurmRmsDto),
    RmsSimulator(RmsSimulatorDto),
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridNodeDto {
    pub id: String,
    pub cpus: i64,
    pub connected_to_router: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkLinkDto {
    pub id: String,
    pub start_point: String,
    pub end_point: String,
    pub capacity: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchDto {
    pub switch_name: String,
    pub switches: Vec<String>,
    pub nodes: Vec<String>,
    pub link_speed: i64,
}
