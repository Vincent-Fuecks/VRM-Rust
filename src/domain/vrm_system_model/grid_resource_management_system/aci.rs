use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_trait::VrmComponent;
use crate::domain::vrm_system_model::rms::advance_reservation_trait::AdvanceReservationRms;
use crate::domain::vrm_system_model::rms::rms::RmsLoadMetric;
use crate::domain::vrm_system_model::utils::id::{AciId, AdcId, ClientId, ComponentId, ShadowScheduleId};
use crate::domain::vrm_system_model::utils::state_logging::{AnalyticLogger, BaseLog, DetailLog, ProbeLog, VrmCommand};
use crate::error::ConversionError;
use crate::schema::aci_dto::AcIDto;
use crate::schema::rms_dto::RmsSystemWrapper;
use crate::vrm::global_clock::global_clock::GlobalClock;
use crate::vrm::reservation::probe_reservations::{ProbeReservationComparator, ProbeReservations};
use crate::vrm::reservation::reservation::{Reservation, ReservationState};
use crate::vrm::reservation::reservation_store::{ReservationId, ReservationStore};

use std::collections::{BTreeMap, HashMap};
use std::i64;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum ScheduleID {
    FreeListSchedule,
    SlottedSchedule,
    SlottedScheduleResubmitFrag,
    SlottedSchedule12,
    SlottedSchedule12000,
    UnlimitedSchedule,
}

#[derive(Debug, Clone)]
pub struct ReservationContainer {
    /// Id of client, how submitted the request
    owner: ClientId,
    ///  Until which time the reservation has to be committed, if only reserved. VRM time in s.
    commit_deadline: i64,
    /// until which time the reservation has to finish execution. VRM time in s.
    execution_deadline: i64,
}

impl ReservationContainer {
    pub fn new(owner: ClientId, commit_deadline: i64, execution_deadline: i64) -> Self {
        Self { owner, commit_deadline, execution_deadline }
    }
}

#[derive(Debug, Default)]
struct ShadowScheduleReservations {
    inner_map: BTreeMap<ShadowScheduleId, HashMap<ReservationId, ReservationContainer>>,
}

impl ShadowScheduleReservations {
    pub fn new() -> Self {
        Self { inner_map: BTreeMap::new() }
    }

    pub fn get(&self, shadow_schedule_id: &ShadowScheduleId) -> Option<&HashMap<ReservationId, ReservationContainer>> {
        self.inner_map.get(shadow_schedule_id)
    }

    pub fn get_mut(&mut self, shadow_schedule_id: &ShadowScheduleId) -> Option<&mut HashMap<ReservationId, ReservationContainer>> {
        self.inner_map.get_mut(shadow_schedule_id)
    }

    pub fn contains_shadow_schedule_id(&self, shadow_schedule_id: &ShadowScheduleId) -> bool {
        self.inner_map.contains_key(shadow_schedule_id)
    }

    pub fn insert(
        &mut self,
        shadow_schedule_id: ShadowScheduleId,
        committed_reservations: &HashMap<ReservationId, ReservationContainer>,
        aci_id: &AciId,
    ) -> bool {
        if self.inner_map.contains_key(&shadow_schedule_id) {
            log::error!("AcI {}: ShadowScheduleId {} already exists. Delete it first.", aci_id, shadow_schedule_id);
            return false;
        }

        self.inner_map.insert(shadow_schedule_id, committed_reservations.clone());
        true
    }

    pub fn delete_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId, aci_id: &AciId) -> bool {
        if self.inner_map.remove(shadow_schedule_id).is_none() {
            log::debug!("AcI {}: Could not delete ShadowScheduleId: {}. It did not exist.", aci_id, shadow_schedule_id);
            return false;
        }
        true
    }

    pub fn delete_reservation_container(
        &mut self,
        reservation_id: ReservationId,
        shadow_schedule_id: &ShadowScheduleId,
    ) -> Option<ReservationContainer> {
        if let Some(map) = self.inner_map.get_mut(shadow_schedule_id) {
            return map.remove(&reservation_id);
        }
        None
    }
}

#[derive(Debug)]
pub struct AcI {
    pub id: AciId,
    adc_id: AdcId,
    commit_timeout: i64,
    rms_system: Box<dyn AdvanceReservationRms + Send>,
    shadow_schedule_reservations: ShadowScheduleReservations,
    committed_reservations: HashMap<ReservationId, ReservationContainer>,
    not_committed_reservations: HashMap<ReservationId, ReservationContainer>,
    open_probe_reservations: HashMap<ReservationId, Option<ShadowScheduleId>>,

    simulator: Arc<GlobalClock>,
    pub reservation_store: ReservationStore,
}

impl AcI {
    pub async fn from_dto(dto: AcIDto, simulator: Arc<GlobalClock>, reservation_store: ReservationStore) -> Result<Self, ConversionError> {
        let aci_id = AciId::new(dto.id.clone());
        let adc_id: AdcId = AdcId::new(dto.adc_id);
        let rms_system = RmsSystemWrapper::get_instance(dto.rms_system, simulator.clone(), aci_id.clone().cast(), reservation_store.clone()).await?;

        Ok(AcI {
            id: aci_id,
            adc_id: adc_id,
            commit_timeout: dto.commit_timeout,
            rms_system,
            shadow_schedule_reservations: ShadowScheduleReservations::new(),
            not_committed_reservations: HashMap::new(),
            committed_reservations: HashMap::new(),
            open_probe_reservations: HashMap::new(),
            simulator: simulator,
            reservation_store: reservation_store.clone(),
        })
    }
}

impl VrmComponent for AcI {
    fn get_id(&self) -> ComponentId {
        ComponentId::new(self.id.to_string())
    }

    fn get_total_capacity(&self) -> i64 {
        self.rms_system.get_total_capacity()
    }

    fn get_total_link_capacity(&self) -> i64 {
        self.rms_system.get_total_link_capacity()
    }

    fn get_total_node_capacity(&self) -> i64 {
        self.rms_system.get_total_node_capacity()
    }

    fn get_link_resource_count(&self) -> usize {
        self.rms_system.get_link_resource_count()
    }

    fn can_handel(&self, res: Reservation) -> bool {
        self.rms_system.can_handle_adc_request(res)
    }

    fn commit(&mut self, reservation_id: ReservationId) -> bool {
        log::debug!("AcI {}: is committing reservation {:?}", self.id, reservation_id);

        let arrival_time: i64 = self.simulator.get_system_time_s();

        // Try to find reservation in not_committed
        let (container, id_to_commit) = match self.not_committed_reservations.remove(&reservation_id) {
            Some(reservation_container) => (reservation_container, reservation_id),
            None => {
                log::info!("No prior reserve for commit of {:?}. Attempting instant allocation.", reservation_id);

                // Check if RMS can handle it
                if !self.rms_system.can_handle_aci_request(self.reservation_store.clone(), reservation_id) {
                    self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                    self.log_base_info(
                        VrmCommand::Commit,
                        format!(
                            "There was no reserve for reservation ({:?}) before the commit performed and the AcI ({}) can't handle the request.",
                            reservation_id, self.id
                        ),
                        reservation_id,
                        arrival_time,
                    );

                    log::debug!(
                        "There was no reserve for reservation ({:?}) before the commit performed and the AcI ({}) can't handle the request.",
                        reservation_id,
                        self.id
                    );
                    return false;
                } else {
                    // Implicit Reserve
                    let possible_reservation_id = self.rms_system.reserve(reservation_id, None).unwrap_or(reservation_id);

                    if !self.reservation_store.is_reservation_state_at_least(possible_reservation_id, ReservationState::ReserveAnswer) {
                        log::debug!(
                            "Commit Reservation failed, because no former allocation was found. In AcI: {}, for reservation id {:?}.",
                            self.id,
                            possible_reservation_id
                        );

                        self.log_base_info(
                            VrmCommand::Commit,
                            format!(
                                "Commit Reservation failed, because no former allocation was found. In AcI: {}, for reservation id {:?}.",
                                self.id, possible_reservation_id
                            ),
                            possible_reservation_id,
                            arrival_time,
                        );
                        return false;
                    }

                    // Success: Create container and return new ID
                    let new_container = ReservationContainer {
                        owner: self.reservation_store.get_client_id(possible_reservation_id),
                        commit_deadline: i64::MAX,
                        execution_deadline: self.reservation_store.get_assigned_end(possible_reservation_id),
                    };

                    (new_container, possible_reservation_id)
                }
            }
        };

        // Is reservation invalid state, if so return
        if self.reservation_store.get_state(id_to_commit) != ReservationState::ReserveAnswer {
            self.reservation_store.update_state(id_to_commit, ReservationState::Rejected);
            return false;
        }

        self.rms_system.commit(id_to_commit);
        log::debug!("Committed reservation {:?} in AcI {} to local RMS.", reservation_id, self.id);
        self.committed_reservations.insert(id_to_commit, container);
        self.log_base_info(
            VrmCommand::Commit,
            format!("Committed reservation {:?} in AcI {} to local RMS.", id_to_commit, self.id),
            reservation_id,
            arrival_time,
        );
        return true;
    }

    fn commit_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        let shadow_schedule_committed_reservations =
            self.shadow_schedule_reservations.get_mut(&shadow_schedule_id).expect("Committed Reservations where not found.").clone();

        let is_committed = self.rms_system.commit_shadow_schedule(&shadow_schedule_id);

        if is_committed {
            self.committed_reservations = shadow_schedule_committed_reservations;

            return self.shadow_schedule_reservations.delete_shadow_schedule(&shadow_schedule_id, &self.id);
        } else {
            panic!(
                "During the process of promoting a shadow schedule ({}) to the new master schedule in Aci: {} happened an error. The current shadow schedule of aci and the underlying rms are now not synchronized anymore.",
                shadow_schedule_id, self.id
            );
        }
    }

    fn create_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        if self.rms_system.create_shadow_schedule(&shadow_schedule_id) {
            let aci_id = self.id.clone();
            if self.shadow_schedule_reservations.insert(shadow_schedule_id, &self.committed_reservations, &aci_id) {
                return true;
            } else {
                panic!(
                    "During the process of creating a new shadow schedule in Aci: {} happened an error. The current shadow schedule of aci and the underlying rms are now not synchronized.",
                    self.id
                )
            }
        }

        log::debug!(
            "The process of creating a new shadow schedule failed. However, the shadow schedule of aci: {} and the underlying rms are sill synchronized.",
            self.id
        );
        return false;
    }

    fn delete(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId {
        let arrival_time = self.simulator.get_system_time_s();
        let container;

        if !shadow_schedule_id.is_none() {
            container = self.shadow_schedule_reservations.delete_reservation_container(reservation_id, &shadow_schedule_id.clone().unwrap());
        } else {
            container = self.not_committed_reservations.remove(&reservation_id);
        }

        if container.is_none() {
            log::info!("There was no reserve before the deletion of the reservation ({:?}) was performed.", reservation_id);
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            if shadow_schedule_id.is_none() {
                self.log_base_info(
                    VrmCommand::Delete,
                    format!("There was no reserve before the deletion of the reservation ({:?}) was performed.", reservation_id),
                    reservation_id,
                    arrival_time,
                );
            }

            return reservation_id;
        }

        // Remove Task from Schedule and local Rms (if ReservationState::Committed)
        self.rms_system.delete_task(reservation_id, shadow_schedule_id.clone());

        return reservation_id;
    }

    fn delete_shadow_schedule(&mut self, shadow_schedule_id: ShadowScheduleId) -> bool {
        if self.rms_system.delete_shadow_schedule(&shadow_schedule_id) {
            let aci_id = self.id.clone();
            if self.shadow_schedule_reservations.delete_shadow_schedule(&shadow_schedule_id, &aci_id) {
                return true;
            } else {
                panic!(
                    "During the process of deleting a new shadow schedule in Aci: {} happened an error. The current shadow schedule of aci and the underlying rms are now not synchronized.",
                    self.id
                );
            }
        }

        log::debug!(
            "The process of deleting a shadow schedule failed. However, the shadow schedule of aci: {} and the underlying rms are sill synchronized.",
            self.id
        );
        return false;
    }

    fn get_load_metric_up_to_date(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.rms_system.get_load_metric_up_to_date(start, end, shadow_schedule_id)
    }

    fn get_load_metric(&self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.rms_system.get_load_metric(start, end, shadow_schedule_id)
    }

    fn get_satisfaction(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        self.rms_system.get_fragmentation(start, end, shadow_schedule_id)
    }

    fn get_simulation_load_metric(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        self.rms_system.get_simulation_load_metric(shadow_schedule_id)
    }

    fn get_system_satisfaction(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        self.rms_system.get_system_fragmentation(shadow_schedule_id)
    }

    fn probe(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ProbeReservations {
        let arrival_time = self.simulator.get_system_time_s();

        // Can Rms handle request in general?
        if !self.rms_system.can_handle_aci_request(self.reservation_store.clone(), reservation_id) {
            if shadow_schedule_id.is_none() {
                self.log_probe_info(
                    VrmCommand::Probe,
                    format!("Can Rms handle request failed for probe request of the reservation {:?}.", reservation_id),
                    reservation_id,
                    arrival_time,
                    -1,
                );
            }

            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            return ProbeReservations::new(reservation_id, self.reservation_store.clone());
        }

        let mut prob_request_answer = self.rms_system.probe(reservation_id, shadow_schedule_id.clone());
        // Way to attach this AcI to the created probeReservations.
        prob_request_answer.add_probe_meta_data(self.id.clone().cast(), shadow_schedule_id.clone());
        // Tracking for when promotion happens
        self.open_probe_reservations.insert(reservation_id, shadow_schedule_id.clone());

        if prob_request_answer.is_empty() {
            if shadow_schedule_id.is_none() {
                self.log_probe_info(
                    VrmCommand::Probe,
                    format!("No feasible ProbeReservation was found for reservation {:?}.", reservation_id),
                    reservation_id,
                    arrival_time,
                    0,
                );
            }
            return prob_request_answer;
        }

        if shadow_schedule_id.is_none() {
            self.log_probe_info(
                VrmCommand::Probe,
                format!("Probe request was performed for reservation {:?}.", reservation_id),
                reservation_id,
                arrival_time,
                prob_request_answer.len() as i64,
            );
        }

        return prob_request_answer;
    }

    fn probe_best(
        &mut self,
        reservation_id: ReservationId,
        shadow_schedule_id: Option<ShadowScheduleId>,
        probe_reservation_comparator: ProbeReservationComparator,
    ) -> ProbeReservations {
        log::debug!("In AcI {} a probeBest request based on reservation {:?} is requested.", self.id, reservation_id);

        let arrival_time = self.simulator.get_system_time_s();

        if !self.rms_system.can_handle_aci_request(self.reservation_store.clone(), reservation_id) {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);

            if shadow_schedule_id.is_none() {
                self.log_probe_info(
                    VrmCommand::ProbeBest,
                    format!("Can Rms handle request failed for probe request of the reservation {:?}.", reservation_id),
                    reservation_id,
                    arrival_time,
                    -1,
                );
            }
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            return ProbeReservations::new(reservation_id, self.reservation_store.clone());
        }

        let mut probe_best_answer = self.rms_system.probe_best(reservation_id, probe_reservation_comparator, shadow_schedule_id.clone());
        // Way to attach this AcI to the created probeReservations.
        probe_best_answer.add_probe_meta_data(self.id.clone().cast(), shadow_schedule_id.clone());

        // Init ProbeReservation tracking -> Informs AcI if VrmComponent likes to reserve a ProbeReservation
        self.open_probe_reservations.insert(reservation_id, shadow_schedule_id);
        return probe_best_answer;
    }

    fn reserve(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) -> ReservationId {
        log::debug!("In AcI {} reserve reservation {:?} for ShadowScheduleId {:?}", self.id, reservation_id, shadow_schedule_id);

        // Is reservation has in valid state stop early
        if !self.reservation_store.is_reserve_request_valid(reservation_id) {
            log::error!(
                "ErrorAcIReserveRequestInValidReservationState: AcI {} reserve reservation {:?} for ShadowScheduleId {:?}, with ReservationState: {:?} ",
                self.id,
                reservation_id,
                shadow_schedule_id,
                self.reservation_store.get_state(reservation_id)
            );
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
            return reservation_id;
        }

        let arrival_time = self.simulator.get_system_time_s();

        if !self.rms_system.can_handle_aci_request(self.reservation_store.clone(), reservation_id) {
            self.reservation_store.update_state(reservation_id, ReservationState::Rejected);

            if shadow_schedule_id.is_none() {
                self.log_base_info(
                    VrmCommand::Reserve,
                    format!("Can handle request for reserve request failed for reservation {:?}.", reservation_id),
                    reservation_id,
                    arrival_time,
                );
            }

            return reservation_id;
        }

        let reserve_answer = self.rms_system.reserve(reservation_id, shadow_schedule_id.clone());

        match reserve_answer {
            None => {
                self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                if shadow_schedule_id.is_none() {
                    self.log_base_info(
                        VrmCommand::Reserve,
                        format!("There was no feasible slot in the Schedule for the reservation {:?} found.", reservation_id),
                        reservation_id,
                        arrival_time,
                    );
                }
                return reservation_id;
            }
            Some(reservation_id_of_answer) => {
                if !self.reservation_store.is_reservation_state_at_least(reservation_id, ReservationState::ReserveAnswer) {
                    self.reservation_store.update_state(reservation_id_of_answer, ReservationState::Rejected);
                    if shadow_schedule_id.is_none() {
                        self.log_base_info(
                            VrmCommand::Reserve,
                            format!("Reservation {:?} was in reserve process rejected.", reservation_id_of_answer),
                            reservation_id_of_answer,
                            arrival_time,
                        );
                    }
                }

                let reservation_container = ReservationContainer::new(
                    self.reservation_store.get_client_id(reservation_id_of_answer),
                    self.reservation_store.get_assigned_end(reservation_id_of_answer),
                    self.simulator.get_system_time_s() + self.commit_timeout,
                );

                if !shadow_schedule_id.is_none() {
                    let mut new_committed_reservations: HashMap<ReservationId, ReservationContainer> = HashMap::new();
                    new_committed_reservations.insert(reservation_id_of_answer, reservation_container.clone());

                    if !self.shadow_schedule_reservations.insert(shadow_schedule_id.clone().unwrap(), &new_committed_reservations, &self.id) {
                        self.reservation_store.update_state(reservation_id, ReservationState::Rejected);
                        return reservation_id;
                    }
                }

                self.not_committed_reservations.insert(reservation_id_of_answer, reservation_container);

                if shadow_schedule_id.is_none() {
                    self.log_base_info(
                        VrmCommand::Reserve,
                        format!("Reserve of Reservation {:?} was successful.", reservation_id),
                        reservation_id,
                        arrival_time,
                    );
                }

                return reservation_id;
            }
        }
    }
}

impl AcI {
    pub fn log_base_info(&self, command: VrmCommand, log_description: String, reservation_id: ReservationId, arrival_time_at_aci: i64) {
        if let Some(base_log) = BaseLog::new(
            self.id.clone(),
            command,
            log_description,
            reservation_id,
            self.reservation_store.clone(),
            self.simulator.clone(),
            arrival_time_at_aci,
        ) {
            base_log.log();
        }
    }

    pub fn log_probe_info(
        &self,
        command: VrmCommand,
        log_description: String,
        reservation_id: ReservationId,
        arrival_time_at_aci: i64,
        n_probe_answers: i64,
    ) {
        if let Some(probe_log) = ProbeLog::new(
            self.id.clone(),
            command,
            log_description,
            reservation_id,
            self.reservation_store.clone(),
            self.simulator.clone(),
            arrival_time_at_aci,
            n_probe_answers,
        ) {
            probe_log.log();
        }
    }

    pub fn log_detail_info(&mut self, command: VrmCommand, log_description: String, reservation_id: ReservationId, arrival_time_at_aci: i64) {
        let start = self.reservation_store.get_assigned_start(reservation_id);
        let end = self.reservation_store.get_assigned_end(reservation_id);
        let rms_load_metric = self.rms_system.get_load_metric_up_to_date(start, end, None);
        let system_fragmentation = self.rms_system.get_system_fragmentation(None);

        if let Some(detail_log) = DetailLog::new(
            self.id.clone(),
            command,
            log_description,
            reservation_id,
            self.reservation_store.clone(),
            self.simulator.clone(),
            arrival_time_at_aci,
            system_fragmentation,
            rms_load_metric,
        ) {
            detail_log.log();
        }
    }
}
