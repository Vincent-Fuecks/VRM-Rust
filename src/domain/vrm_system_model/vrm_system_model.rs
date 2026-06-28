use std::collections::HashMap;

use crate::domain::vrm_system_model::grid_resource_management_system::aci::AcI;
use crate::domain::vrm_system_model::grid_resource_management_system::adc::ADC;
use crate::vrm::commons::id::{AciId, AdcId};

#[derive(Debug)]
pub struct Vrm {
    pub adc_master: AdcId,
    pub adcs: HashMap<AdcId, ADC>,
    pub acis: HashMap<AciId, AcI>,
}
