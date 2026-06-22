use anyhow::{Result, anyhow};
use bimap::{BiHashMap, BiMap};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::i64;
use std::{str::FromStr, sync::Arc};
use tokio::runtime::Handle;
use tokio::time::{Duration, MissedTickBehavior, interval};

use crate::domain::simulator::simulator::GlobalClock;
use crate::domain::vrm_system_model::reservation::node_reservation::NodeReservation;
use crate::domain::vrm_system_model::reservation::reservation::{Reservation, ReservationState, ReservationTrait};
use crate::domain::vrm_system_model::reservation::reservation_store::ReservationId;
use crate::domain::vrm_system_model::resource::node_resource::NodeResource;
use crate::domain::vrm_system_model::resource::resource_store::ResourceStore;
use crate::domain::vrm_system_model::rms::rms::Rms;
use crate::domain::vrm_system_model::schedule::schedule_trait::Schedule;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::link::topology::NetworkTopology;
use crate::domain::vrm_system_model::scheduler_type::ScheduleContext;
use crate::domain::vrm_system_model::utils::config::SCHEDULE_SYNC_TIMEINTERVAL_S;
use crate::domain::vrm_system_model::utils::id::{ComponentId, ResourceName, RmsId, ShadowScheduleId, SlottedScheduleId};
use crate::{
    api::rms_config_dto::rms_dto::SlurmRmsDto,
    domain::vrm_system_model::{
        reservation::reservation_store::ReservationStore, rms::rms::RmsBase, scheduler_type::SchedulerType, utils::id::AciId,
    },
};

use super::api_client::response::tasks::SlurmTaskResponse;
use super::api_client::slurm_rest_api_client::SlurmRestApiClient;
use super::api_client::slurm_rest_api_trait::SlurmRestApi;

#[derive(Debug)]
pub struct SlurmRms {
    pub base: RmsBase,
    pub component_id: ComponentId,
    pub simulator: Arc<GlobalClock>,
    pub slurm_rest_client: SlurmRestApiClient,

    // Master Schedules
    pub node_schedule: Arc<RwLock<Box<dyn Schedule>>>,
    pub network_schedule: Arc<RwLock<Box<dyn Schedule>>>,

    // Shadow schedules for simulations
    pub node_shadow_schedule: HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>,
    pub network_shadow_schedule: HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>,

    // Mapping between VRM ReservationId and Slurm Task Id
    pub task_mapping: Arc<RwLock<BiMap<ReservationId, u32>>>,

    pub rt_handle: Handle,
}

impl SlurmRms {
    pub async fn new(
        dto: SlurmRmsDto,
        simulator: Arc<GlobalClock>,
        component_id: ComponentId,
        reservation_store: ReservationStore,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let rest_api_client = SlurmRestApiClient::new(dto.rest_api_config.clone())?;
        let nodes_response = rest_api_client.get_nodes().await.expect(&format!("Connection to Slurm based RMS of AcI {:?} was not possible", component_id));

        let (nodes, links) = SlurmRms::get_nodes_and_links(&dto, &nodes_response);
        let resource_store = ResourceStore::new();

        // Setup Node Schedule
        let mut schedule_capacity = 0;

        // Add nodes to ResourceStore
        for node in nodes.iter() {
            schedule_capacity += node.cpus;
            resource_store.add_node(NodeResource::new(node.name.clone(), node.cpus));
        }

        let name = format!("AcI: {}, RmsType: {}, RmsName: {}", component_id, "Slurm".to_string(), dto.id);
        let schedule_context = ScheduleContext {
            id: SlottedScheduleId::new(name.clone()),
            number_of_slots: dto.num_of_slots,
            slot_width: dto.slot_width,
            capacity: schedule_capacity,
            simulator: simulator.clone(),
            reservation_store: reservation_store.clone(),
        };

        let scheduler_type = SchedulerType::from_str(&dto.scheduler_typ)?;
        let node_schedule = Arc::new(RwLock::new(scheduler_type.get_instance(schedule_context)));

        // Setup Network Schedule
        // Adds Links to Resource Store
        let topology = NetworkTopology::new(
            &links,
            &nodes,
            dto.slot_width,
            dto.num_of_slots,
            simulator.clone(),
            component_id.clone(),
            reservation_store.clone(),
            resource_store.clone(),
        );

        let schedule_context = ScheduleContext {
            id: SlottedScheduleId::new(name.clone()),
            number_of_slots: dto.num_of_slots,
            slot_width: dto.slot_width,
            capacity: i64::MAX,
            simulator: simulator.clone(),
            reservation_store: reservation_store.clone(),
        };

        let mut scheduler_type = SchedulerType::from_str(&dto.scheduler_typ)?;
        scheduler_type = scheduler_type.get_network_scheduler_variant(topology, resource_store.clone());
        let network_schedule = Arc::new(RwLock::new(scheduler_type.get_instance(schedule_context)));

        let base = RmsBase::new(component_id.clone(), "Slurm".to_string(), reservation_store, resource_store.clone());

        let slurm_rms = SlurmRms {
            base: base,
            component_id: component_id,
            simulator: simulator,
            node_schedule,
            network_schedule,
            node_shadow_schedule: HashMap::new(),
            network_shadow_schedule: HashMap::new(),
            slurm_rest_client: rest_api_client,
            task_mapping: Arc::new(RwLock::new(BiMap::new())),
            rt_handle: Handle::current(),
        };

        slurm_rms.start_sync();
        Ok(slurm_rms)
    }
}

impl SlurmRms {
    /// Starts the background synchronization loop.
    pub fn start_sync(&self) {
        let node_schedule = self.node_schedule.clone();
        let slurm_rest_client = self.slurm_rest_client.clone();
        let task_mapping = self.task_mapping.clone();
        let resource_store = self.base.resource_store.clone();
        let reservation_store = self.base.reservation_store.clone();
        let rms_id = self.base.id.clone();
        let component_id = self.component_id.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(SCHEDULE_SYNC_TIMEINTERVAL_S));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                if let Err(e) =
                    Self::perform_sync(&slurm_rest_client, &resource_store, &reservation_store, &node_schedule, &task_mapping, &rms_id, &component_id).await
                {
                    log::error!("Slurm Schedule Sync Error: {:?}", e);
                }
            }
        });
    }

    async fn perform_sync(
        client: &SlurmRestApiClient,
        resource_store: &ResourceStore,
        reservation_store: &ReservationStore,
        node_schedule: &Arc<RwLock<Box<dyn Schedule>>>,
        task_mapping: &Arc<RwLock<BiHashMap<ReservationId, u32>>>,
        rms_id: &RmsId,
        component_id: &ComponentId,
    ) -> anyhow::Result<()> {
        let slurm_nodes = client.get_nodes().await?;
        let slurm_tasks = client.get_tasks().await?;

        let node_resources: Vec<NodeResource> =
            slurm_nodes.nodes.iter().map(|node| NodeResource::new(ResourceName::new(node.name.clone()), node.cpus as i64)).collect();
        let old_node_capacity = resource_store.get_total_node_capacity();

        // Update Nodes in ResourceStore (Changes occur, if new nodes are up or registered nodes are down).
        resource_store.update_nodes(node_resources);

        let new_node_capacity = resource_store.get_total_node_capacity();

        if old_node_capacity != new_node_capacity {
            let mut guard = node_schedule.write();
            // Adjust capacity in node schedule accordingly to the new total node capacity.
            guard.update_capacity(new_node_capacity as usize);
        }

        Self::update_reservations(reservation_store, task_mapping, node_schedule, slurm_tasks, rms_id, component_id);

        Ok(())
    }

    fn update_reservations(
        reservation_store: &ReservationStore,
        task_mapping: &Arc<RwLock<BiHashMap<ReservationId, u32>>>,
        node_schedule: &Arc<RwLock<Box<dyn Schedule>>>,
        slurm_tasks: SlurmTaskResponse,
        rms_id: &RmsId,
        component_id: &ComponentId,
    ) {
        let active_slurm_ids: HashSet<u32> = slurm_tasks.jobs.iter().map(|job| job.job_id).collect();
        let mut external_reservations = Vec::new();

        // Tasks removed by the RMS scheduling logic
        let mut to_finish = Vec::new();
        // Task, were the RMS changed the job status
        let mut to_update = Vec::new();

        {
            let mapping = task_mapping.read();

            // Find tasks deleted/finish by the RMS scheduling logic
            for (reservation_id, slurm_task_id) in mapping.iter() {
                if !active_slurm_ids.contains(slurm_task_id) {
                    to_finish.push((*reservation_id, *slurm_task_id));
                }
            }

            // Process Task Updates
            for slurm_task in &slurm_tasks.jobs {
                // Is Task tracked in Schedule?
                if let Some(reservation_id) = mapping.get_by_right(&slurm_task.job_id) {
                    if let Some(slurm_task_states) = &slurm_task.job_state {
                        if slurm_task_states.is_empty() {
                            log::debug!(
                                "The slurm job {:?} running on RMS {:?} contains no valid state. Possible due to a Slurm cluster failure.",
                                slurm_task.job_id,
                                rms_id
                            );
                        } else if slurm_task_states.len() > 1 {
                            log::debug!(
                                "The slurm job {:?} running on RMS {:?} contains multiple job states {:?}, currently only the first state is taken into account.",
                                slurm_task.job_id,
                                rms_id,
                                slurm_task_states
                            );
                        } else {
                            if let Some(first_state) = slurm_task_states.first() {
                                if let Ok(new_state) = ReservationState::from_slurm_task_state(first_state) {
                                    let current_state = reservation_store.get_state(*reservation_id);
                                    if current_state != new_state {
                                        to_update.push((*reservation_id, new_state));
                                    }
                                } else {
                                    log::warn!("Job {} on RMS {:?} has no valid state.", slurm_task.job_id, rms_id);
                                }
                            }
                        }
                    }
                } else {
                    // Aggregate External Reservations
                    let node_reservation = Reservation::Node(NodeReservation::from_slurm(slurm_task, component_id.clone().cast()));
                    external_reservations.push((slurm_task.job_id, node_reservation));
                }
            }
        }

        // Remove reservations from the mapping
        if !to_finish.is_empty() {
            let mut mapping = task_mapping.write();
            for (_, slurm_task_id) in &to_finish {
                mapping.remove_by_right(slurm_task_id);
            }
        }

        // Trigger state updates
        for (res_id, _) in to_finish {
            reservation_store.update_state(res_id, ReservationState::Finished);
        }

        for (res_id, new_state) in to_update {
            reservation_store.update_state(res_id, new_state);
        }

        // Add External Task to Schedule and ReservationStore
        Self::update_schedule(reservation_store, task_mapping, external_reservations, node_schedule, rms_id);
    }

    fn update_schedule(
        reservation_store: &ReservationStore,
        task_mapping: &Arc<RwLock<BiHashMap<ReservationId, u32>>>,
        external_reservations: Vec<(u32, Reservation)>,
        node_schedule: &Arc<RwLock<Box<dyn Schedule>>>,
        rms_id: &RmsId,
    ) {
        for (slurm_task_id, res) in external_reservations {
            log::debug!("INSERT EXTERNAL: RESERVATION {:?} into ReservationStore", res.get_name().clone());

            let res_id = reservation_store.add(res);
            task_mapping.write().insert(res_id, slurm_task_id);

            let mut guard = node_schedule.write();

            if let Some(_) = guard.reserve(res_id) {
                log::debug!(
                    "EXTERNAL: RESERVATION {:?} was successfully reserved in node schedule for RMS {:?}.",
                    reservation_store.get_name_for_key(res_id),
                    rms_id
                );
            } else {
                log::error!(
                    "Reserve of EXTERNAL: RESERVATION {:?} failed at RMS {:?} at node schedule.",
                    reservation_store.get_name_for_key(res_id),
                    rms_id
                );

                reservation_store.remove(res_id);
            }
        }
    }

    /// Returns the ReservationStore
    pub fn get_reservation_store(&self) -> &ReservationStore {
        &self.get_base().reservation_store
    }

    /// Deletes all Task from the RMS cluster.
    pub async fn delete_all_tasks(&self) -> Result<bool> {
        let mut is_rms_clean = true;
        if let Ok(slurm_task_response) = self.slurm_rest_client.get_tasks().await {
            for job in slurm_task_response.jobs {
                if let Ok(is_deleted) = self.slurm_rest_client.delete(job.job_id).await {
                    if !is_deleted {
                        log::warn!("Failed to delete task {:?} (slurm job id) from cluster {:?}", job.job_id, self.base.id);
                        is_rms_clean = false;
                    }
                }
            }
        }

        Ok(is_rms_clean)
    }

    /// Returns the count of task on the rms in the state RUNNING and PENDING.
    pub async fn get_active_task_count(&self) -> Result<usize> {
        match self.slurm_rest_client.get_tasks().await {
            Ok(tasks) => Ok(tasks
                .jobs
                .iter()
                .filter(|j| {
                    j.job_state.as_ref().map_or(false, |states| states.contains(&"RUNNING".to_string()) || states.contains(&"PENDING".to_string()))
                })
                .count()),
            Err(e) => {
                eprintln!("REST Client Error: {:?}", e);
                Err(anyhow!("The get task request failed: {}", e))
            }
        }
    }
}
