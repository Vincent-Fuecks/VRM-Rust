use crate::domain::vrm_system_model::grid_resource_management_system::scheduler::heft_sync_workflow_scheduler::HEFTSyncWorkflowScheduler;
use crate::domain::vrm_system_model::grid_resource_management_system::scheduler::workflow_scheduler::WorkflowScheduler;
use crate::vrm::reservation::reservation_store::ReservationStore;
use crate::error::ConversionError;
use std::str::FromStr;

/// Represents the available scheduling algorithms for managing workflows in a distributed environment.
#[derive(Debug)]
pub enum WorkflowSchedulerType {
    ExhaustiveEFT,
    ExhaustiveFrag,
    /// **Heterogeneous Earliest Finish Time (Synchronous)**: A heuristic-based approach
    /// for scheduling tasks on a set of heterogeneous processors.
    HEFTSync,
    HEFTFrag,
    FragWindow,
    FragWindowZHAO,
}

impl WorkflowSchedulerType {
    /// Factory method to return a concrete instance of a [`WorkflowScheduler`] based on the enum variant.
    pub fn get_instance(workflow_typ: WorkflowSchedulerType, reservation_store: ReservationStore) -> Box<dyn WorkflowScheduler> {
        match workflow_typ {
            WorkflowSchedulerType::ExhaustiveEFT => {
                todo!("Not implemented yet!")
            }
            WorkflowSchedulerType::ExhaustiveFrag => {
                todo!("Not implemented yet!")
            }
            WorkflowSchedulerType::HEFTSync => HEFTSyncWorkflowScheduler::new(reservation_store),
            WorkflowSchedulerType::HEFTFrag => {
                todo!("Not implemented yet!")
            }
            WorkflowSchedulerType::FragWindow => {
                todo!("Not implemented yet!")
            }
            WorkflowSchedulerType::FragWindowZHAO => {
                todo!("Not implemented yet!")
            }
        }
    }
}

impl FromStr for WorkflowSchedulerType {
    type Err = ConversionError;

    fn from_str(rms_type_dto: &str) -> Result<WorkflowSchedulerType, Self::Err> {
        match rms_type_dto {
            "Exhaustive-EFT" => Ok(WorkflowSchedulerType::ExhaustiveEFT),
            "Exhaustive-Frag" => Ok(WorkflowSchedulerType::ExhaustiveFrag),
            "HEFT-Sync" => Ok(WorkflowSchedulerType::HEFTSync),
            "HEFT-Frag" => Ok(WorkflowSchedulerType::HEFTFrag),
            "Frag-Window" => Ok(WorkflowSchedulerType::FragWindow),
            "Frag-Window-Zhao" => Ok(WorkflowSchedulerType::FragWindowZHAO),
            _ => Err(ConversionError::UnknownRmsType(rms_type_dto.to_string())),
        }
    }
}
