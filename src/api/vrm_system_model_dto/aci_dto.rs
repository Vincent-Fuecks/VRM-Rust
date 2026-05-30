use serde::Deserialize;

use crate::api::rms_config_dto::rms_dto::RmsSystemWrapper;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AcIDto {
    pub id: String,
    pub adc_id: String,
    pub commit_timeout: i64,
    pub rms_system: RmsSystemWrapper,
}
