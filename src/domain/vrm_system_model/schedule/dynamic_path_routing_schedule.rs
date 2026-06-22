use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use log::debug;

use crate::domain::vrm_system_model::reservation::probe_reservations::ProbeReservations;
use crate::domain::vrm_system_model::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::resource::resource_store::ResourceStore;
use crate::domain::vrm_system_model::utils::config::RMS_GATEWAY_NAME;
use crate::domain::vrm_system_model::utils::id::{ReservationName, RouterId};

use super::schedule_trait::Schedule;

#[derive(Debug)]
pub struct DynamicPathRoutingSchedule<S: Schedule> {
    /// The underlying physical schedule (e.g., SlottedScheduleContext)
    inner_schedule: S,

    /// Thread-safe reference to the globally updated topology
    resource_store: ResourceStore,

    reservation_store: ReservationStore,
    /// Original Reservation ot all virtual reservations
    dynamic_paths: HashMap<ReservationId, ReservationId>,
}

impl<S: Schedule> DynamicPathRoutingSchedule<S> {
    pub fn new(inner_schedule: S, resource_store: ResourceStore, reservation_store: ReservationStore) -> Self {
        Self { inner_schedule, resource_store, reservation_store, dynamic_paths: HashMap::new() }
    }

    pub fn create_dynamic_path_routing(&mut self, reservation_id: ReservationId) -> bool {
        fn log_error(start: Option<RouterId>, end: Option<RouterId>, reservation_id: ReservationId, res_name: Option<ReservationName>) {
            log::error!(
                "Dynamic path routing was not possible, because not one of the start point ({:?}) or end ({:?}) point RouterIds of the LinkReservation ({:?} | {:?}) are part of the local RMS.",
                start,
                end,
                res_name,
                reservation_id
            );
        }
        // Early stop, dynamic path routing was already performed.
        if self.dynamic_paths.contains_key(&reservation_id) {
            return true;
        }

        let start = self.reservation_store.get_start_point(reservation_id);
        let end = self.reservation_store.get_end_point(reservation_id);

        match (start, end) {
            (Some(start), Some(end)) => {
                let virtual_res_id;
                // Target and Source in the local RMS --> no dynamic scheduling is necessary
                if self.resource_store.contains_router_id(&start) && self.resource_store.contains_router_id(&end) {
                    self.dynamic_paths.insert(reservation_id, reservation_id);
                    return true;

                    // Start or End is outside of local RMS system --> create virtual reservation form AcI Gateway to start/end point
                } else if self.resource_store.contains_router_id(&start) {
                    virtual_res_id = self.reservation_store.add_virtual_reservation_diff_end(reservation_id, RouterId::new(RMS_GATEWAY_NAME));
                } else if self.resource_store.contains_router_id(&end) {
                    virtual_res_id = self.reservation_store.add_virtual_reservation_diff_start(reservation_id, RouterId::new(RMS_GATEWAY_NAME));
                } else {
                    log_error(Some(start), Some(end), reservation_id, self.reservation_store.get_name_for_key(reservation_id));
                    return false;
                }

                if let Some(virtual_res_id) = virtual_res_id {
                    self.dynamic_paths.insert(reservation_id, virtual_res_id);
                    return true;
                } else {
                    log_error(Some(start), Some(end), reservation_id, self.reservation_store.get_name_for_key(reservation_id));
                    return false;
                }
            }
            (Some(start), None) => {
                if self.resource_store.contains_router_id(&start) {
                    if let Some(virtual_res_id) =
                        self.reservation_store.add_virtual_reservation_diff_end(reservation_id, RouterId::new(RMS_GATEWAY_NAME))
                    {
                        self.dynamic_paths.insert(reservation_id, virtual_res_id);
                        return true;
                    }
                }

                log_error(Some(start), None, reservation_id, self.reservation_store.get_name_for_key(reservation_id));
                return false;
            }

            (None, Some(end)) => {
                if self.resource_store.contains_router_id(&end) {
                    if let Some(virtual_res_id) =
                        self.reservation_store.add_virtual_reservation_diff_start(reservation_id, RouterId::new(RMS_GATEWAY_NAME))
                    {
                        self.dynamic_paths.insert(reservation_id, virtual_res_id);
                        return true;
                    }
                }

                log_error(None, Some(end), reservation_id, self.reservation_store.get_name_for_key(reservation_id));
                return false;
            }

            (None, None) => {
                log_error(None, None, reservation_id, self.reservation_store.get_name_for_key(reservation_id));
                return false;
            }
        }
    }
}

impl<S: Schedule> Schedule for DynamicPathRoutingSchedule<S> {
    fn clear(&mut self) {
        todo!()
    }

    fn clone_box(&self) -> Box<dyn Schedule> {
        todo!()
    }

    fn delete_reservation(&mut self, id: crate::domain::vrm_system_model::reservation::reservation_store::ReservationId) {
        todo!()
    }

    fn get_fragmentation(&mut self, frag_start_time: i64, frag_end_time: i64) -> f64 {
        todo!()
    }

    fn get_load_metric(&self, start_time: i64, end_time: i64) -> crate::domain::vrm_system_model::utils::load_buffer::LoadMetric {
        todo!()
    }

    fn get_load_metric_up_to_date(&mut self, start_time: i64, end_time: i64) -> crate::domain::vrm_system_model::utils::load_buffer::LoadMetric {
        todo!()
    }

    fn get_simulation_load_metric(&mut self) -> crate::domain::vrm_system_model::utils::load_buffer::LoadMetric {
        todo!()
    }

    fn get_system_fragmentation(&mut self) -> f64 {
        todo!()
    }

    fn probe(&mut self, reservation_id: ReservationId) -> ProbeReservations {
        todo!()
    }

    fn probe_best(
        &mut self,
        reservation_id: crate::domain::vrm_system_model::reservation::reservation_store::ReservationId,
        probe_reservation_comparator: crate::domain::vrm_system_model::reservation::probe_reservations::ProbeReservationComparator,
    ) -> crate::domain::vrm_system_model::reservation::probe_reservations::ProbeReservations {
        todo!()
    }

    fn reserve(
        &mut self,
        id: crate::domain::vrm_system_model::reservation::reservation_store::ReservationId,
    ) -> Option<crate::domain::vrm_system_model::reservation::reservation_store::ReservationId> {
        todo!()
    }

    fn reserve_without_check(&mut self, id: crate::domain::vrm_system_model::reservation::reservation_store::ReservationId) {
        todo!()
    }

    fn update(&mut self) {
        todo!()
    }

    fn update_capacity(&mut self, capacity: usize) {
        todo!()
    }
}
