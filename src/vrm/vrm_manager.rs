use anyhow::Ok;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::time::{Duration, sleep};

use crate::vrm::{
    common::id::{AdcId, ClientId, ComponentId},
    reservation::{
        probe_reservations::ProbeReservationComparator,
        reservation::{ReservationProceeding, ReservationState},
        reservation_store::{ReservationId, ReservationStore},
        vrm_state_listener::VrmStateListener,
    },
};
use crate::{
    error::ConversionError,
    schema::vrm_dto::VrmDto,
    vrm::global_clock::global_clock::GlobalClock,
    vrm::vrm_component::{
        aci::AcI,
        adc::ADC,
        scheduler::workflow_scheduler_type::WorkflowSchedulerType,
        vrm_component_order::VrmComponentOrder,
        vrm_component_registry::{registry_client::RegistryClient, vrm_component_proxy::VrmComponentProxy},
        vrm_component_trait::VrmComponent,
    },
};

pub struct VrmManager {
    pub adc_master: VrmComponentProxy,

    /// Reservation which were not submitted to the VRM system
    pub unprocessed_reservations: Vec<(ReservationId, i64)>,

    /// Reservations that are still processed by the VRM or RMS (ReservationState is not in a terminal state)
    /// e.g. a workflow was submitted to the RMS but predecessor task of this workflow still wait to be processed.
    pub open_reservations: Arc<RwLock<HashSet<ReservationId>>>,

    /// Contains all processed reservations of the VRM, that reached a terminal state ReservationState.
    pub processed_reservations: Arc<RwLock<HashSet<ReservationId>>>,

    pub reservation_store: ReservationStore,
    pub simulator: Arc<GlobalClock>,
}

impl VrmManager {
    fn new(
        adc_master: VrmComponentProxy,
        unprocessed_reservations: Vec<(ReservationId, i64)>,
        reservation_store: ReservationStore,
        simulator: Arc<GlobalClock>,
    ) -> Self {
        VrmManager {
            adc_master,
            unprocessed_reservations,
            open_reservations: Arc::new(RwLock::new(HashSet::new())),
            processed_reservations: Arc::new(RwLock::new(HashSet::new())),
            reservation_store,
            simulator,
        }
    }

    /// Idea: Is should be possible for the client to later request all his currently scheduled reservations on the vrm system.
    pub fn get_managed_reservations_for_client(&self, client_id: &ClientId) -> Vec<ReservationId> {
        self.reservation_store.get_client_reservations(client_id)
    }

    pub async fn init_vrm_system(
        dto: VrmDto,
        unprocessed_reservations: Vec<ReservationId>,
        simulator: Arc<GlobalClock>,
        registry: RegistryClient,
        reservation_store: ReservationStore,
    ) -> Result<Self, ConversionError> {
        let open_reservations = Arc::new(RwLock::new(HashSet::new()));
        let listener = Arc::new(RwLock::new(VrmStateListener::new(open_reservations.clone())));
        reservation_store.add_listener(listener);

        let mut proxies: HashMap<ComponentId, VrmComponentProxy> = HashMap::new();

        // Setup AcI Proxies (spawn all in own thread)
        for aci_dto in dto.aci {
            let aci = AcI::from_dto(aci_dto, simulator.clone(), reservation_store.clone()).await?;
            let component_box: Box<dyn VrmComponent + Send> = Box::new(aci);

            let proxy: VrmComponentProxy = registry.spawn_component(component_box);
            proxies.insert(proxy.get_id(), proxy);
        }

        let mut pending_adcs = dto.adc;
        let mut progress_made = true;
        let adc_master_id = ComponentId::new(dto.adc_master_id);
        let mut adc_master: Option<VrmComponentProxy> = None;

        // Setup ADC Proxies start bottom up (first init all children)(spawn all ADCs in there own thread)
        while !pending_adcs.is_empty() && progress_made {
            progress_made = false;
            let mut next_pending = Vec::new();

            for adc_dto in pending_adcs {
                let adc_id_str = adc_dto.id.clone();
                let children_ids: Vec<String> = adc_dto.children.clone();

                let all_children_ready = children_ids.iter().all(|child_id| proxies.contains_key(&ComponentId::new(child_id.clone())));

                if all_children_ready {
                    let mut children_proxies: Vec<VrmComponentProxy> = Vec::new();
                    for child_id in children_ids {
                        let proxy = proxies.get(&ComponentId::new(child_id)).unwrap().clone();

                        children_proxies.push(proxy.clone());
                    }

                    let workflow_scheduler = WorkflowSchedulerType::get_instance(WorkflowSchedulerType::HEFTSync, reservation_store.clone());

                    let vrm_component_order = VrmComponentOrder::OrderStartFirst;

                    let adc = ADC::new(
                        AdcId::new(adc_id_str),
                        children_proxies,
                        registry.clone(),
                        reservation_store.clone(),
                        Some(workflow_scheduler),
                        vrm_component_order,
                        adc_dto.timeout,
                        simulator.clone(),
                        adc_dto.num_of_slots,
                        adc_dto.slot_width,
                    );
                    let component_box: Box<dyn VrmComponent + Send> = Box::new(adc);

                    let adc_proxy = registry.spawn_component(component_box);
                    if adc_master_id.compare(&adc_proxy.get_id()) {
                        adc_master = Some(adc_proxy.clone());
                    }
                    proxies.insert(adc_proxy.get_id(), adc_proxy);

                    progress_made = true;
                } else {
                    // Not ready yet (children missing)
                    next_pending.push(adc_dto);
                }
            }
            pending_adcs = next_pending;
        }

        if !pending_adcs.is_empty() {
            panic!("Failed to create all ADCs! Possible circular dependency or missing child ID.");
        }

        log::info!("System successfully initialized with {} components.", proxies.len());

        match adc_master {
            Some(adc_master) => {
                let vrm_manager = VrmManager::new(
                    adc_master,
                    reservation_store.get_sorted_res_ids_with_arrival_time(unprocessed_reservations),
                    reservation_store,
                    simulator,
                );

                return Ok(vrm_manager).map_err(|_| ConversionError::AdcConstructionError("Master-AcI".to_string()));
            }
            None => Err(ConversionError::UnknownRmsType("Failed to find adc master".to_string())),
        }
    }

    pub async fn run_vrm(&mut self) {
        // Submit all reservation to the VRM system.
        while !self.unprocessed_reservations.is_empty() {
            let (reservation_id, res_arrival_time) = self.unprocessed_reservations.remove(0);
            let now = self.simulator.get_system_time_s();

            if res_arrival_time > now {
                let wait_seconds = res_arrival_time - now;
                if wait_seconds > 0 {
                    sleep(Duration::from_secs(wait_seconds as u64)).await;
                }
            }

            if !self.reservation_store.contains(reservation_id) {
                panic!("Reservation {:?} was not added to the ReservationStore.", self.reservation_store.get_name_for_key(reservation_id));
            }

            self.process_reservation(reservation_id).await;
        }
        log::info!("VrmManager: Submitted unprocessed reservations to the VRM system.");
        self.close_open_links();
        self.reservation_store.print_store_contents();

        // Transfer all reservation in a terminal reservation state.
        // Workflows that have the proceeding state Commit, will not transfer immediately into a terminal state.
        while !self.open_reservations.read().is_empty() {
            let mut reservations_to_remove: Vec<ReservationId> = vec![];
            let open_ids: Vec<ReservationId> = self.open_reservations.read().iter().cloned().collect();

            for open_res_id in open_ids.iter() {
                if self.reservation_store.is_reservation_at_cycle_end(*open_res_id) {
                    reservations_to_remove.push(*open_res_id);
                }

                if self.reservation_store.is_res_commit_ready(*open_res_id) {
                    self.try_to_commit_reservation(open_res_id.clone());
                }

                if self.reservation_store.is_workflow(*open_res_id) {
                    let mut is_finished = true;
                    for w_exit_res_id in self.reservation_store.get_workflow_exit_res_ids(*open_res_id).unwrap().iter() {
                        if matches!(self.reservation_store.get_state(*w_exit_res_id), ReservationState::Deleted | ReservationState::Rejected) {
                            self.reservation_store.update_state(*open_res_id, ReservationState::Rejected);
                            is_finished = false;
                        }
                        if !matches!(self.reservation_store.get_state(*w_exit_res_id), ReservationState::Finished) {
                            is_finished = false;
                        }
                    }

                    if is_finished {
                        self.reservation_store.update_state(*open_res_id, ReservationState::Finished);
                    }
                }
            }

            if !reservations_to_remove.is_empty() {
                let mut guard = self.open_reservations.write();
                for res_to_remove in reservations_to_remove.iter() {
                    guard.remove(res_to_remove);

                    if matches!(self.reservation_store.get_state(*res_to_remove), ReservationState::Rejected | ReservationState::Deleted) {
                        log::debug!(
                            "Reservation {:?} ({:?}) was closed by VrmManager, in the state {:?}, that signals in error in the life time cycle of the reservation.",
                            res_to_remove,
                            self.reservation_store.get_name_for_key(*res_to_remove),
                            self.reservation_store.get_state(*res_to_remove)
                        );
                    }
                }
            }
            sleep(Duration::from_secs(5)).await;
            self.reservation_store.print_store_contents();
        }

        log::info!("VrmManager: All reservations in the VRM system reached a terminal state.")
    }

    /// Probes, Reserves, Commits or Deletes the submitted job
    /// However, workflow will not be fully committed to the underlying rms, because there task execution can depend on previous submitted sub-jobs.
    async fn process_reservation(&mut self, process_res_id: ReservationId) {
        let use_master_schedule = None;
        log::info!("Try to submit Reservation {:?} the the master Adc.", self.reservation_store.get_name_for_key(process_res_id));

        // Step 1: Quick reserve via Probe request if Reservation is not a workflow
        if !self.reservation_store.is_workflow(process_res_id) {
            log::info!("Try to reserve Reservation {:?} via probe request.", self.reservation_store.get_name_for_key(process_res_id));
            let mut probe_reservations = self.adc_master.probe(process_res_id, use_master_schedule.clone());

            // Prompt best ProbeReservation -> Try to reserve ProbeReservation
            if let Some((_, _)) = probe_reservations.prompt_best(process_res_id, ProbeReservationComparator::ESTReservationCompare) {
                // Reserve of ProbeReservation was not possible -> Reset Reservation to original
                if !self.reservation_store.is_reservation_state_at_least(process_res_id, ReservationState::ReserveAnswer) {
                    probe_reservations.demote();
                } else {
                    log::info!(
                        "Reservation {:?} was successful reserved via probe request.",
                        self.reservation_store.get_name_for_key(process_res_id)
                    );
                }
            }
        }

        if self.reservation_store.is_reservation_proceeding(process_res_id, ReservationProceeding::Probe) {
            log::info!("Reservation {:?}, canceled by user after probe.", self.reservation_store.get_name_for_key(process_res_id));
            return;
        }

        // Step 2: Reserve
        if self.reservation_store.is_reservation_proceeding(process_res_id, ReservationProceeding::Reserve) {
            log::info!("Try to reserve Reservation {:?}.", self.reservation_store.get_name_for_key(process_res_id));
            self.adc_master.reserve(process_res_id, use_master_schedule.clone());

            if self.reservation_store.get_state(process_res_id) != ReservationState::ReserveAnswer {
                log::info!("Reservation {:?} could not be reserved. ", self.reservation_store.get_name_for_key(process_res_id));
                return;
            }

            if self.reservation_store.is_reservation_proceeding(process_res_id, ReservationProceeding::Reserve) {
                log::info!("Reservation {:?} canceled by user after reserve.", self.reservation_store.get_name_for_key(process_res_id));
                return;
            }
        }

        // Step 3: Commit or Delete Reservation
        self.try_to_commit_reservation(process_res_id);

        if self.reservation_store.is_reservation_proceeding(process_res_id, ReservationProceeding::Delete) {
            self.adc_master.delete(process_res_id, None);
            if self.reservation_store.get_state(process_res_id) == ReservationState::Deleted {
                log::info!("Reservation {:?} was successfully deleted by the user.", self.reservation_store.get_name_for_key(process_res_id));
            } else {
                log::info!("Reservation {:?} could not be deleted.", self.reservation_store.get_name_for_key(process_res_id));
            }
        }
    }

    /// There is currently now functionality, that is able to reserve links at rms site
    pub fn close_open_links(&mut self) {
        let mut link_res_ids: Vec<ReservationId> = vec![];

        for res_id in self.open_reservations.read().clone() {
            if self.reservation_store.is_link(res_id) {
                link_res_ids.push(res_id);
            }
        }

        for link_id in link_res_ids.iter() {
            self.open_reservations.write().remove(link_id);
        }
    }

    pub fn try_to_commit_reservation(&mut self, process_res_id: ReservationId) {
        if self.reservation_store.is_reservation_proceeding(process_res_id, ReservationProceeding::Commit) {
            log::info!("Try to commit Reservation {:?} via Commit request.", self.reservation_store.get_name_for_key(process_res_id));

            if self.reservation_store.is_workflow(process_res_id) {
                log::info!(
                    "Reservation {:?} is a workflow therefore first reserve all sub-task of the workflow.",
                    self.reservation_store.get_name_for_key(process_res_id)
                );
                self.adc_master.reserve(process_res_id, None);

                if self.reservation_store.get_state(process_res_id) == ReservationState::ReserveAnswer {
                    log::info!(
                        "Workflow {:?} with all its sub-task were successfully reserved.",
                        self.reservation_store.get_name_for_key(process_res_id)
                    );
                } else {
                    log::info!(
                        "Workflow {:?} reserve request was unsuccessful. The Reservation is in state {:?}",
                        self.reservation_store.get_name_for_key(process_res_id),
                        self.reservation_store.get_state(process_res_id)
                    );
                    return;
                }
            }

            self.adc_master.commit(process_res_id);

            if self.reservation_store.is_reservation_state_at_least(process_res_id, ReservationState::Committed) {
                // Manually add to open reservations on success
                let mut guard = self.open_reservations.write();
                guard.insert(process_res_id);

                // Add all workflow sub-jobs to the open_reservation list (these are properly not in state committed)
                for w_sub_job in self.reservation_store.get_workflow_res_ids(process_res_id).unwrap() {
                    guard.insert(w_sub_job);
                }
                log::info!("Reservation {:?} was committed successful.", self.reservation_store.get_name_for_key(process_res_id));
            } else {
                log::info!("Reservation {:?} could not be committed.", self.reservation_store.get_name_for_key(process_res_id));
            }
        }
    }
}
