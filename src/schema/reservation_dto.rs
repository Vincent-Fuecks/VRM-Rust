use serde::{Deserialize, Serialize};

use crate::domain::vrm_system_model::reservation::reservation::{ReservationProceeding, ReservationState};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LinkReservationDto {
    pub start_point: String,
    pub end_point: String,
    pub amount: Option<i64>,
    pub bandwidth: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NodeReservationDto {
    pub current_working_directory: Option<String>,
    pub environment: Option<Vec<String>>,
    pub task_path: String,
    pub output_path: Option<String>,
    pub error_path: Option<String>,
    pub duration: i64,
    pub cpus: i64,
    pub is_moldable: bool,
    pub dependencies: DependencyDto,
    pub data_out: Vec<DataOutDto>,
    pub data_in: Vec<DataInDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReservationDto {
    pub id: Option<String>,
    pub proceeding: ReservationProceedingDto,
    pub used_aci_hierarchy: Vec<String>,

    pub arrival_time: i64,
    pub booking_interval_start: i64,
    pub booking_interval_end: i64,
    pub assigned_start: i64,
    pub assigned_end: i64,

    pub frag_delta: f32,
    pub job_duration: i32,
    pub reserved_capacity: i32,
    pub moldable: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReservationStateDto {
    Rejected,
    Deleted,
    Open,
    ProbeAnswer,
    ReserveAnswer,
    Committed,
    Finished,
}

impl ReservationStateDto {
    pub fn to_reservation_state(&self) -> ReservationState {
        match self {
            Self::Rejected => ReservationState::Rejected,
            Self::Deleted => ReservationState::Deleted,
            Self::Open => ReservationState::Open,
            Self::ProbeAnswer => ReservationState::ProbeAnswer,
            Self::ReserveAnswer => ReservationState::ReserveAnswer,
            Self::Committed => ReservationState::Committed,
            Self::Finished => ReservationState::Finished,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservationProceedingDto {
    Probe,
    Reserve,
    Commit,
    Delete,
}

impl ReservationProceedingDto {
    pub fn to_reservation_proceeding(&self) -> ReservationProceeding {
        match self {
            Self::Probe => ReservationProceeding::Probe,
            Self::Reserve => ReservationProceeding::Reserve,
            Self::Commit => ReservationProceeding::Commit,
            Self::Delete => ReservationProceeding::Delete,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DataOutDto {
    pub name: String,
    pub file: Option<String>,
    pub size: Option<i64>,
    pub bandwidth: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DataInDto {
    pub source_reservation: String,
    pub source_port: String,
    pub file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DependencyDto {
    pub data: Vec<String>,
    pub sync: Vec<String>,
}
