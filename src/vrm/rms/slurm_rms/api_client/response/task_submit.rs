use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct TaskSubmitResponse {
    pub job_id: Option<u32>,
    pub step_id: Option<String>,
    pub error: Option<String>,
}