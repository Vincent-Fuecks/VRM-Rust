use anyhow::Ok;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};
use tokio::time::{Duration, sleep};

use crate::{
    api::vrm_system_model_dto::vrm_dto::VrmDto,
    domain::{
        simulator::simulator::GlobalClock,
        vrm_system_model::{
            grid_resource_management_system::{
                aci::AcI,
                adc::ADC,
                scheduler::workflow_scheduler_type::WorkflowSchedulerType,
                vrm_component_order::VrmComponentOrder,
                vrm_component_registry::{registry_client::RegistryClient, vrm_component_proxy::VrmComponentProxy},
                vrm_component_trait::VrmComponent,
            },
            reservation::{
                probe_reservations::ProbeReservationComparator,
                reservation::{ReservationProceeding, ReservationState},
                reservation_store::{ReservationId, ReservationStore},
                vrm_state_listener::VrmStateListener,
            },
            utils::id::{AdcId, ComponentId},
        },
    },
    error::ConversionError,
};

use super::utils::id::ClientId;

pub struct VrmManager {
    pub adc_master: VrmComponentProxy,
    pub unprocessed_reservations: Vec<(ReservationId, i64)>,
    pub open_reservations: Arc<RwLock<HashSet<ReservationId>>>,

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
        VrmManager { adc_master, unprocessed_reservations, open_reservations: Arc::new(RwLock::new(HashSet::new())), reservation_store, simulator }
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
        log::info!("VrmManager: Finished processing all unprocessed reservations.");
    }

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
                    log::info!("Reservation {:?} was sucessful reseved via probe request.", self.reservation_store.get_name_for_key(process_res_id));
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
        if self.reservation_store.is_reservation_proceeding(process_res_id, ReservationProceeding::Commit) {
            self.adc_master.commit(process_res_id);

            if self.reservation_store.get_state(process_res_id) == ReservationState::Committed {
                // Manually add to open reservations on success
                let mut guard = self.open_reservations.write().unwrap();
                guard.insert(process_res_id);
                log::info!("Reservation {:?} was committed successful.", self.reservation_store.get_name_for_key(process_res_id));
            } else {
                log::info!("Reservation {:?} could not be committed.", self.reservation_store.get_name_for_key(process_res_id));
            }
        } else if self.reservation_store.is_reservation_proceeding(process_res_id, ReservationProceeding::Delete) {
            self.adc_master.delete(process_res_id, None);
            if self.reservation_store.get_state(process_res_id) == ReservationState::Deleted {
                log::info!("Reservation {:?} was successfully deleted by the user.", self.reservation_store.get_name_for_key(process_res_id));
            } else {
                log::info!("Reservation {:?} could not be deleted.", self.reservation_store.get_name_for_key(process_res_id));
            }
        } else {
            log::error!(
                "Unknown Request ProceedingState {:?} for Reservation {:?}.",
                self.reservation_store.get_reservation_proceeding(process_res_id),
                self.reservation_store.get_name_for_key(process_res_id)
            );
        }
    }
}
