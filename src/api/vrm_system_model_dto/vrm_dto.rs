use crate::api::vrm_system_model_dto::aci_dto::AcIDto;
use crate::api::vrm_system_model_dto::adc_dto::ADCDto;
use crate::domain::simulator::simulator::GlobalClockDto;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VrmDto {
    pub simulator: GlobalClockDto,
    pub adc_master_id: String,
    pub adc: Vec<ADCDto>,
    pub aci: Vec<AcIDto>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SimulatorDto {
    pub end_time: i64,
    pub is_simulation: bool,
}
