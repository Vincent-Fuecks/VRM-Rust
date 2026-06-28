use std::fmt::Debug;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::vrm_system_model::reservation::reservation::{ReservationProceeding, ReservationState};
use crate::domain::vrm_system_model::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::rms::rms::RmsLoadMetric;
use crate::domain::vrm_system_model::utils::statistics::ANALYTICS_TARGET;
use crate::vrm::global_clock::global_clock::GlobalClock;

use super::id::{AciId, ReservationName};

#[derive(Debug)]
pub struct BaseLog {
    pub log_description: String,
    pub component_id: AciId,
    pub time: u64,
    pub command: VrmCommand,
    pub processing_time: i64,
    pub res_name: ReservationName,
    pub res_start: i64,
    pub res_end: i64,
    pub res_cap: i64,
    pub res_workload: i64,
    pub res_state: ReservationState,
    pub res_proceeding: ReservationProceeding,
    pub n_tasks: usize,
}

#[derive(Debug)]
pub struct ProbeLog {
    base: BaseLog,
    n_probe_answers: i64,
}

#[derive(Debug)]
pub struct DetailLog {
    base: BaseLog,
    node_utilization: Option<f64>,
    node_possible_capacity: Option<f64>,
    network_utilization: Option<f64>,
    network_possible_capacity: Option<f64>,
    system_fragmentation: f64,
}

#[derive(Debug)]
pub enum VrmCommand {
    Reserve,
    Commit,
    Delete,
    Probe,
    ProbeBest,
}
pub trait AnalyticLogger: Debug {
    fn log(&self);
}

impl AnalyticLogger for BaseLog {
    fn log(&self) {
        tracing::info!(
            target: ANALYTICS_TARGET,
            LogDescription = self.log_description,
            ComponentType = %self.component_id,
            Time = self.time,
            ProcessingTime = self.processing_time,
            Command = ?self.command,
            ReservationName = %self.res_name,
            ReservationCapacity = self.res_cap,
            ReservationWorkload = self.res_workload,
            ReservationState = ?self.res_state,
            ReservationProceeding = ?self.res_proceeding,
            NumberOfTasks = self.n_tasks,
        );
    }
}

impl BaseLog {
    pub fn new(
        component_id: AciId,
        command: VrmCommand,
        log_description: String,
        reservation_id: ReservationId,
        reservation_store: ReservationStore,
        simulator: Arc<GlobalClock>,
        arrival_time_at_aci: i64,
    ) -> Option<Self> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let processing_time = simulator.get_system_time_s() - arrival_time_at_aci;

        if let Some(res_handle) = reservation_store.get(reservation_id) {
            let (start, end, res_name, capacity, workload, state, proceeding, num_tasks) = {
                let res = res_handle.read();

                let start = res.get_base_reservation().get_assigned_start();
                let end = res.get_base_reservation().get_assigned_end();
                let name = res.get_base_reservation().get_name().clone();
                let cap = res.get_base_reservation().get_reserved_capacity();
                let workload = res.get_base_reservation().get_task_duration() * cap;
                let state = res.get_base_reservation().get_state();
                let proceeding = res.get_base_reservation().get_reservation_proceeding();

                let mut tasks = 1;
                if res.is_workflow() {
                    tasks = res.as_workflow().unwrap().get_all_reservation_ids().len();
                }

                (start, end, name, cap, workload, state, proceeding, tasks)
            };

            let base_log = BaseLog {
                log_description: log_description,
                component_id: component_id,
                time: now,
                command,
                processing_time,
                res_name,
                res_start: start,
                res_end: end,
                res_cap: capacity,
                res_workload: workload,
                res_state: state,
                res_proceeding: proceeding,
                n_tasks: num_tasks,
            };
            return Some(base_log);
        }
        return None;
    }
}

impl AnalyticLogger for ProbeLog {
    fn log(&self) {
        tracing::info!(
            target: ANALYTICS_TARGET,
            LogDescription = self.base.log_description,
            ComponentType = %self.base.component_id,
            Time = self.base.time,
            ProcessingTime = self.base.processing_time,
            Command = ?self.base.command,
            ReservationName = %self.base.res_name,
            ReservationCapacity = self.base.res_cap,
            ReservationWorkload = self.base.res_workload,
            ReservationState = ?self.base.res_state,
            ReservationProceeding = ?self.base.res_proceeding,
            NumberOfTasks = self.base.n_tasks,
            ProbeAnswers = self.n_probe_answers,
        );
    }
}

impl ProbeLog {
    pub fn new(
        component_id: AciId,
        command: VrmCommand,
        log_description: String,
        reservation_id: ReservationId,
        reservation_store: ReservationStore,
        simulator: Arc<GlobalClock>,
        arrival_time_at_aci: i64,
        num_of_answers: i64,
    ) -> Option<Self> {
        let base_log = BaseLog::new(component_id, command, log_description, reservation_id, reservation_store, simulator, arrival_time_at_aci);

        if let Some(base_log) = base_log {
            return Some(ProbeLog { base: base_log, n_probe_answers: num_of_answers });
        }
        return None;
    }
}

impl AnalyticLogger for DetailLog {
    fn log(&self) {
        tracing::info!(
            target: ANALYTICS_TARGET,
            Time = self.base.time,
            ProcessingTime = self.base.processing_time,
            Command = ?self.base.command,
            ReservationName = %self.base.res_name,
            ReservationCapacity = self.base.res_cap,
            ReservationWorkload = self.base.res_workload,
            ReservationState = ?self.base.res_state,
            ReservationProceeding = ?self.base.res_proceeding,
            NumberOfTasks = self.base.n_tasks,
            NodeComponentUtilization = self.node_utilization,
            NodeComponentCapacity = self.node_possible_capacity,
            NetworkComponentUtilization = self.network_utilization,
            NetworkComponentCapacity = self.network_possible_capacity,
            ComponentFragmentation = self.system_fragmentation,
        );
    }
}

impl DetailLog {
    pub fn new(
        component_id: AciId,
        command: VrmCommand,
        log_description: String,
        reservation_id: ReservationId,
        reservation_store: ReservationStore,
        simulator: Arc<GlobalClock>,
        arrival_time_at_aci: i64,
        system_fragmentation: f64,
        rms_load_metric: RmsLoadMetric,
    ) -> Option<Self> {
        let base_log = BaseLog::new(component_id, command, log_description, reservation_id, reservation_store, simulator, arrival_time_at_aci);

        if let Some(base_log) = base_log {
            // Map the metrics safely
            let node_utilization = rms_load_metric.node_load_metric.as_ref().map(|n| n.utilization);
            let node_possible_capacity = rms_load_metric.node_load_metric.as_ref().map(|n| n.possible_capacity);
            let network_utilization = rms_load_metric.link_load_metric.as_ref().map(|n| n.utilization);
            let network_possible_capacity = rms_load_metric.link_load_metric.as_ref().map(|n| n.possible_capacity);

            return Some(DetailLog {
                base: base_log,
                node_utilization,
                node_possible_capacity,
                network_utilization,
                network_possible_capacity,
                system_fragmentation,
            });
        }
        return None;
    }
}
