use serde::{Deserialize, Serialize};

use super::workflow_dto::WorkflowDto;

#[derive(Debug, Deserialize, Serialize)]
pub struct ClientsDto {
    pub clients: Vec<ClientDto>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDto {
    pub id: String,
    pub workflows: Vec<WorkflowDto>,
}
