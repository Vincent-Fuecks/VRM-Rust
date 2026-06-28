use std::{any::Any, collections::HashSet};

use serde::{Deserialize, Serialize};

use crate::domain::vrm_system_model::rms::slurm_rms::api_client::response::tasks::{SlurmOptionExt, SlurmTask};
use crate::vrm::commons::id::{ClientId, ComponentId, ReservationName};

use super::{
    reservation::{ReservationBase, ReservationProceeding, ReservationState, ReservationTrait, ReservationTyp},
    reservation_store::ReservationId,
};

/// This structure extends [`ReservationBase`] to include fields specific to
/// **computational node** (e.g., CPU cores).
///
/// The maximum task execution time (**duration**) has to be provided in advance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeReservation {
    /// The common base properties shared by all reservations.
    pub base: ReservationBase,

    /// Data dependencies before the job can start
    pub data_dependencies: HashSet<ReservationId>,

    /// Acts for the root for all provided relative paths on the RMS.
    pub current_working_directory: Option<String>,

    /// Defines the exported variables to the compute node when the task runs on the RMS.
    pub environment: Option<Vec<String>>,

    /// File system **path** pointing to the executable for this reservation/task.
    pub task_path: String,

    /// The file path where the **standard output** (stdout) during task execution will be piped.
    pub output_path: Option<String>,

    /// The file path where the **standard error** (stderr) during task execution will be piped.
    pub error_path: Option<String>,
}

impl NodeReservation {
    pub fn new(
        name: ReservationName,
        client_id: ClientId,
        handler_id: Option<ComponentId>,
        state: ReservationState,
        request_proceeding: ReservationProceeding,
        arrival_time: i64,
        booking_interval_start: i64,
        booking_interval_end: i64,
        task_duration: i64,
        reserved_capacity: i64,
        is_moldable: bool,
        frag_delta: f64,
        data_dependencies: HashSet<ReservationId>,
        current_working_directory: Option<String>,
        environment: Option<Vec<String>>,
        task_path: String,
        output_path: Option<String>,
        error_path: Option<String>,
    ) -> Self {
        // Calculate work: Capacity * Time
        let moldable_work = reserved_capacity * task_duration;

        let base = ReservationBase {
            name,
            client_id,
            handler_id,
            state,
            request_proceeding,
            arrival_time,
            booking_interval_start,
            booking_interval_end,
            assigned_start: 0, // Default to 0 until formally scheduled
            assigned_end: 0,   // Default to 0 until formally scheduled
            task_duration,
            reserved_capacity,
            is_moldable,
            moldable_work,
            frag_delta,
        };

        NodeReservation { base, task_path, output_path, error_path, current_working_directory, environment, data_dependencies }
    }
}

impl ReservationTrait for NodeReservation {
    fn get_base(&self) -> &ReservationBase {
        &self.base
    }

    fn get_base_mut(&mut self) -> &mut ReservationBase {
        &mut self.base
    }

    fn box_clone(&self) -> Box<dyn ReservationTrait> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_type(&self) -> ReservationTyp {
        ReservationTyp::Node
    }
}

impl NodeReservation {
    /// Creates NodeReservation based on the submitted SlurmTask.
    /// This reservation will be ignored by the VRM system.
    pub fn from_slurm(task: &SlurmTask, aci_id: ComponentId) -> Self {
        // Default to 1 CPU if unknown
        let capacity = task.job_resources.as_ref().unwrap().allocated_cpus.val_or(1);
        let task_id: u32 = task.job_id;
        let task_user = task.user_name.val_or("slurm_import".to_string());
        let mut arrival_time = 0;
        let mut booking_interval_start = 0;
        let mut booking_interval_end = 0;
        let mut assigned_start = 0;
        let mut assigned_end = 0;
        let mut duration = 0;

        if let Some(time) = task.time.as_ref() {
            arrival_time = time.submission.val_or(0) as i64;
            booking_interval_start = time.eligible.val_or(0) as i64;
            booking_interval_end = time.end.val_or(0) as i64;
            assigned_start = time.start.val_or(0) as i64;
            assigned_end = time.end.val_or(0) as i64;
            duration = time.limit.val_or(0) as i64;
        }

        let node_reservation = NodeReservation {
            base: ReservationBase {
                name: ReservationName::new(format!("External-Task-From-AcI-{:?}-Task-Id-{:?}", aci_id, task_id)),
                client_id: ClientId::new(format!("External-Task-From-{:?}", task_user)),
                handler_id: Some(aci_id),
                state: ReservationState::External,
                request_proceeding: ReservationProceeding::Ignore,
                arrival_time,
                booking_interval_start,
                booking_interval_end,
                assigned_start,
                assigned_end,
                task_duration: duration,
                reserved_capacity: capacity,
                is_moldable: false,
                moldable_work: capacity * duration,
                frag_delta: 0.0,
            },
            data_dependencies: HashSet::new(),
            current_working_directory: None,
            environment: None,
            task_path: "External-Task".to_string(),
            output_path: None,
            error_path: None,
        };

        return node_reservation;
    }
}
