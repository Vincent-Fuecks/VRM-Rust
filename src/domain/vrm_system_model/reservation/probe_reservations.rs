use std::{cmp::Ordering, collections::HashMap, fmt::Debug};

use crate::domain::vrm_system_model::{
    reservation::{
        reservation::{Reservation, ReservationTrait},
        reservation_store::{ReservationId, ReservationStore},
    },
    utils::id::{ComponentId, ProbeReservationId, ShadowScheduleId},
};

#[derive(Clone, Debug)]
pub enum ProbeReservationComparator {
    EFTReservationCompare,
    ESTReservationCompare,
}

impl ProbeReservationComparator {
    pub fn compare(&self, a: &Reservation, b: &Reservation) -> Ordering {
        let base_a = a.get_base_reservation();
        let base_b = b.get_base_reservation();

        match self {
            ProbeReservationComparator::EFTReservationCompare => base_a.get_assigned_end().cmp(&base_b.get_assigned_end()),
            ProbeReservationComparator::ESTReservationCompare => base_a.get_assigned_start().cmp(&base_b.get_assigned_start()),
        }
    }
}

/// ProbeReservations are hypotitic Reservations, which are only tracked by this
/// ProbeReservations Object.
/// If the ProbeReservation should replace the actual Reservation use `promote_reservation`
#[derive(Debug)]
pub struct ProbeReservations {
    pub original_reservation_id: ReservationId,
    pub local_reservation_store: HashMap<ProbeReservationId, Reservation>,
    original_reservation: Reservation,
    reservation_store: ReservationStore,
    reservation_idx: usize,
    pub probe_meta_data: HashMap<ProbeReservationId, ProbeMetadata>,
}

#[derive(Clone, Debug)]
pub struct ProbeMetadata {
    pub source_component_id: ComponentId,
    pub shadow_schedule_id: Option<ShadowScheduleId>,
    pub adc: ComponentId,
}

impl ProbeReservations {
    pub fn new(original_reservation_id: ReservationId, reservation_store: ReservationStore) -> ProbeReservations {
        if let Some(original_reservation) = reservation_store.get_reservation_snapshot(original_reservation_id) {
            return ProbeReservations {
                original_reservation_id,
                local_reservation_store: HashMap::new(),
                original_reservation,
                reservation_store,
                reservation_idx: 0,
                probe_meta_data: HashMap::new(),
            };
        } else {
            panic!("ProbeReservationOriginalReservationNotFound");
        }
    }

    pub fn add_adc(&mut self, adc_component: ComponentId) {
        for meta in self.probe_meta_data.iter_mut() {
            meta.1.adc = adc_component.clone();
        }
    }

    pub fn add_reservation(&mut self, reservation: Reservation) -> Result<(), String> {
        let probe_reservation_id = ProbeReservationId::new(format!("{}-{}", reservation.get_name(), self.reservation_idx));

        // Check if Reservation is valid
        if reservation.get_assigned_start() < reservation.get_booking_interval_start()
            || reservation.get_assigned_end() > reservation.get_booking_interval_end()
        {
            log::error!("ProbeReservationIsNotValid");
        }

        if self.local_reservation_store.insert(probe_reservation_id, reservation).is_some() {
            log::error!("Can not add two ProbeReservations with the same name to the local store.");
            return Err("Duplicate ProbeReservation".to_string());
        }

        self.reservation_idx += 1;
        Ok(())
    }

    pub fn add_probe_reservations(&mut self, mut other: ProbeReservations) {
        if self.original_reservation_id == other.original_reservation_id {
            for (old_id, res) in other.local_reservation_store.drain() {
                let meta = other.probe_meta_data.remove(&old_id);

                //Generates a new ID for ProbeReservation
                let new_id = ProbeReservationId::new(format!("{}-{}", res.get_name(), self.reservation_idx));
                self.local_reservation_store.insert(old_id, res);

                if let Some(m) = meta {
                    self.probe_meta_data.insert(new_id, m);
                }
                self.reservation_idx += 1;
            }
        }
    }

    /// Resets Promotion of Reservation to original Reservation.
    pub fn demote(&mut self) {
        let res_base = self.original_reservation.get_base_reservation();
        self.reservation_store.set_booking_interval_start(self.original_reservation_id, res_base.get_booking_interval_start());
        self.reservation_store.set_booking_interval_end(self.original_reservation_id, res_base.get_booking_interval_end());
        self.reservation_store.set_assigned_start(self.original_reservation_id, res_base.get_assigned_start());
        self.reservation_store.set_assigned_end(self.original_reservation_id, res_base.get_assigned_end());
        self.reservation_store.update_state(self.original_reservation_id, res_base.get_state());
    }

    /// Finds in the ProbeReservations, the Reservation, which is according to the ProbeReservationComparator
    /// the best Reservation und updates the original Reservation with the information of the ProbeReservation.
    ///
    /// Return:
    /// If promotion was successful the component_id, is returned, where the Reservation must be reserved.
    pub fn prompt_best(&mut self, original_res_id: ReservationId, comparator: ProbeReservationComparator) -> Option<ProbeMetadata> {
        let best_probe_res_id = self.get_best_probe_reservation_id(original_res_id, comparator)?;

        let best_probe_reservation = self.local_reservation_store.remove(&best_probe_res_id);
        let meta_data = self.probe_meta_data.remove(&best_probe_res_id);

        match (best_probe_reservation, meta_data) {
            (Some(res), Some(probe_meta_data)) => {
                self.reservation_store.set_booking_interval_start(original_res_id, res.get_booking_interval_start());
                self.reservation_store.set_booking_interval_end(original_res_id, res.get_booking_interval_end());
                self.reservation_store.set_assigned_start(original_res_id, res.get_assigned_start());
                self.reservation_store.set_assigned_end(original_res_id, res.get_assigned_end());
                self.reservation_store.update_state(original_res_id, res.get_state());

                Some(probe_meta_data)
            }
            _ => {
                log::warn!("Promotion failed: Reservation or Metadata missing for {:?}", best_probe_res_id);
                None
            }
        }
    }

    /// Finds in the ProbeReservations, the Reservation, which is according to the ProbeReservationComparator
    /// the best Reservation und updates the original Reservation with the information of the ProbeReservation.
    ///
    /// Return:
    /// If promotion was successful the component_id, is returned, where the Reservation must be reserved.
    pub fn only_prompt_best(&mut self, original_res_id: ReservationId, comparator: ProbeReservationComparator) -> bool {
        if let Some(best_probe_res_id) = self.get_best_probe_reservation_id(original_res_id, comparator) {
            if let Some(res) = self.local_reservation_store.remove(&best_probe_res_id) {
                self.reservation_store.set_booking_interval_start(original_res_id, res.get_booking_interval_start());
                self.reservation_store.set_booking_interval_end(original_res_id, res.get_booking_interval_end());
                self.reservation_store.set_assigned_start(original_res_id, res.get_assigned_start());
                self.reservation_store.set_assigned_end(original_res_id, res.get_assigned_end());
                self.reservation_store.update_state(original_res_id, res.get_state());

                return true;
            }
        }
        return false;
    }

    /// Finds in the ProbeReservations, the Reservation, which is according to the ProbeReservationComparator
    /// the best Reservation.
    ///
    /// Return:
    /// Returns a new ProbeReservation object, which only contains the "best ProbeReservation"
    /// If ProbeReservation is Empty an empty ProbeReservation object is returned.
    pub fn get_best_probe_reservation_id(
        &self,
        original_res_id: ReservationId,
        comparator: ProbeReservationComparator,
    ) -> Option<ProbeReservationId> {
        if !self.is_request_valid(original_res_id) || self.local_reservation_store.is_empty() {
            return None;
        }

        let mut best_id: Option<ProbeReservationId> = None;
        let mut best_res: Option<&Reservation> = None;

        for (candidate_id, res_candidate) in &self.local_reservation_store {
            match best_res {
                None => {
                    best_id = Some(candidate_id.clone());
                    best_res = Some(res_candidate);
                }
                Some(current_best) => {
                    if comparator.compare(current_best, res_candidate) == Ordering::Greater {
                        best_id = Some(candidate_id.clone());
                        best_res = Some(res_candidate);
                    }
                }
            }
        }
        best_id
    }

    pub fn get_ids(&self) -> Vec<ProbeReservationId> {
        self.local_reservation_store.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.local_reservation_store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.local_reservation_store.is_empty()
    }

    pub fn get_mut_reservations(&mut self) -> Vec<&mut Reservation> {
        self.local_reservation_store.values_mut().collect()
    }

    /// Checks if request_id and original ReservationId are the same
    fn is_request_valid(&self, test_res_id: ReservationId) -> bool {
        if test_res_id.eq(&self.original_reservation_id) {
            return true;
        } else {
            log::error!(
                "ProbeReservationsGetResWithFistSlotIncorrectUse: ProbeAnswer of original Reservation {:?} was requested form ReservationId {:?}. Signals a improper use of ProbeReservations, which will lead to an unexpected outcome.",
                self.reservation_store.get_name_for_key(test_res_id),
                self.original_reservation_id
            );
            return false;
        }
    }

    /// Adds to all ProbeReservation in this ProbeReservations object the provided component_id.
    /// This component_id is later in the promotion process utilized to submit this probeReservation to reserve this probeReservation by the vrm_component, that created the probeReservation.
    pub fn add_probe_meta_data(
        &mut self,
        aci_component_id: ComponentId,
        adc_component_id: ComponentId,
        shadow_schedule_id: Option<ShadowScheduleId>,
    ) {
        for probe_id in self.local_reservation_store.keys() {
            self.probe_meta_data.insert(
                probe_id.clone(),
                ProbeMetadata {
                    source_component_id: aci_component_id.clone(),
                    shadow_schedule_id: shadow_schedule_id.clone(),
                    adc: adc_component_id.clone(),
                },
            );
        }
    }

    pub fn create_new_probe_reservation_with_best_probe(
        &mut self,
        original_res_id: ReservationId,
        comparator: ProbeReservationComparator,
    ) -> Option<ProbeReservations> {
        let mut new_probe_reservations = ProbeReservations::new(original_res_id, self.reservation_store.clone());

        if !self.is_request_valid(original_res_id) || self.local_reservation_store.is_empty() {
            return Some(new_probe_reservations);
        }

        if let Some(best_id) = self.get_best_probe_reservation_id(original_res_id, comparator) {
            if let Some(res) = self.local_reservation_store.get(&best_id) {
                let _ = new_probe_reservations.add_reservation(res.clone());
                if let Some(probe_meta_data) = self.probe_meta_data.get(&best_id) {
                    new_probe_reservations.probe_meta_data.insert(best_id.clone(), probe_meta_data.clone());
                }
            }
        }

        Some(new_probe_reservations)
    }
}
