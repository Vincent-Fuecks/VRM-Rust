use crate::vrm::reservation::reservation_store::ReservationStore;
use crate::domain::vrm_system_model::rms::advance_reservation_trait::AdvanceReservationRms;
use crate::domain::vrm_system_model::rms::rms_simulator::rms_network_simulator::RmsNetworkSimulator;
use crate::domain::vrm_system_model::rms::rms_simulator::rms_node_simulator::RmsNodeSimulator;
use crate::domain::vrm_system_model::rms::slurm_rms::slurm_base::SlurmRms;
use crate::domain::vrm_system_model::utils::id::ComponentId;
use crate::error::ConversionError;
use crate::schema::rms_dto::RmsSystemWrapper;
use crate::vrm::global_clock::global_clock::GlobalClock;
use std::str::FromStr;
use std::sync::Arc;

use super::rms_simulator::rms_simulator::RmsSimulator;

#[derive(Debug)]
pub enum RmsSimulatorType {
    RmsNodeSimulator,
    RmsNetworkSimulator,
}

impl RmsSystemWrapper {
    pub async fn get_instance(
        dto: RmsSystemWrapper,
        simulator: Arc<GlobalClock>,
        component_id: ComponentId,
        reservation_store: ReservationStore,
    ) -> Result<Box<dyn AdvanceReservationRms + Send + Sync + 'static>, ConversionError> {
        match dto {
            RmsSystemWrapper::Slurm(dto) => {
                let rms_instance = SlurmRms::new(dto, simulator, component_id, reservation_store).await;

                match rms_instance {
                    Ok(rms_instance) => Ok(Box::new(rms_instance) as Box<dyn AdvanceReservationRms + Send + Sync>),
                    Err(e) => panic!("SlurmClusterInitProcessFailed: Error: {:?}", e),
                }
            }

            RmsSystemWrapper::RmsSimulator(dto) => {
                let rms_simulator = RmsSimulator::new(dto, simulator, component_id, reservation_store);

                match rms_simulator {
                    Ok(rms_instance) => Ok(Box::new(rms_instance) as Box<dyn AdvanceReservationRms + Send + Sync>),
                    Err(e) => panic!("RmsSimulatorInitProcessFailed: Error: {:?}", e),
                }
            }

            RmsSystemWrapper::DummyRms(dummy_rms_dto) => {
                let rms_type = RmsSimulatorType::from_str(&dummy_rms_dto.typ)?;

                match rms_type {
                    RmsSimulatorType::RmsNodeSimulator => {
                        let rms_node_simulator_instance = RmsNodeSimulator::try_from((dummy_rms_dto, simulator, component_id, reservation_store))?;
                        Ok(Box::new(rms_node_simulator_instance) as Box<dyn AdvanceReservationRms + Send + Sync>)
                    }

                    RmsSimulatorType::RmsNetworkSimulator => {
                        let rms_network_simulator = RmsNetworkSimulator::try_from((dummy_rms_dto, simulator, component_id, reservation_store))?;
                        Ok(Box::new(rms_network_simulator) as Box<dyn AdvanceReservationRms + Send + Sync>)
                    }
                }
            }
        }
    }
}

impl FromStr for RmsSimulatorType {
    type Err = ConversionError;

    fn from_str(rms_type_dto: &str) -> Result<RmsSimulatorType, Self::Err> {
        match rms_type_dto {
            "RmsNodeSimulator" => Ok(RmsSimulatorType::RmsNodeSimulator),
            "RmsNetworkSimulator" => Ok(RmsSimulatorType::RmsNetworkSimulator),
            _ => Err(ConversionError::UnknownRmsType(rms_type_dto.to_string())),
        }
    }
}
