use std::collections::HashMap;

use crate::vrm::common::id::{AciId, AdcId};
use crate::vrm::vrm_component::aci::AcI;
use crate::vrm::vrm_component::adc::ADC;

#[derive(Debug)]
pub struct Vrm {
    pub adc_master: AdcId,
    pub adcs: HashMap<AdcId, ADC>,
    pub acis: HashMap<AciId, AcI>,
}
