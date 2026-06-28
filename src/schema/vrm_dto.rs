use crate::domain::simulator::simulator::GlobalClockDto;
use serde::Deserialize;

use super::aci_dto::AcIDto;
use super::adc_dto::ADCDto;

#[derive(Debug, Deserialize)]
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
