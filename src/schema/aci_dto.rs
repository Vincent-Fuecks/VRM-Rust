use serde::Deserialize;

use super::rms_dto::RmsSystemWrapper;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcIDto {
    pub id: String,
    pub adc_id: String,
    pub commit_timeout: i64,
    pub rms_system: RmsSystemWrapper,
}
