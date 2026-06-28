use crate::vrm::reservation::reservation::{Reservation, ReservationTrait};
use crate::vrm::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::resource::node_resource::NodeResource;
use crate::domain::vrm_system_model::resource::resource_store::ResourceStore;
use crate::domain::vrm_system_model::rms::advance_reservation_trait::AdvanceReservationRms;
use crate::domain::vrm_system_model::rms::rms::{Rms, RmsBase, RmsLoadMetric};
use crate::domain::vrm_system_model::schedule::schedule_trait::Schedule;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::link::topology::Node;
use crate::domain::vrm_system_model::scheduler_type::{ScheduleContext, SchedulerType};
use crate::domain::vrm_system_model::utils::id::{ComponentId, ResourceName, RouterId, ShadowScheduleId, SlottedScheduleId};
use crate::error::ConversionError;
use crate::schema::rms_dto::DummyRmsDto;
use crate::vrm::global_clock::global_clock::GlobalClock;
use parking_lot::RwLock;
use std::any::Any;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

/// Only simulates a cluster with nodes (a Network with link reservations etc. is not managed)
#[derive(Debug)]
pub struct RmsNodeSimulator {
    pub base: RmsBase,
    pub node_schedule: Arc<RwLock<Box<dyn Schedule>>>,
    pub node_shadow_schedule: HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>,
}

impl RmsNodeSimulator {
    pub fn new(base: RmsBase, node_schedule: Arc<RwLock<Box<dyn Schedule>>>) -> Self {
        RmsNodeSimulator { base, node_schedule, node_shadow_schedule: HashMap::new() }
    }
}

impl TryFrom<(DummyRmsDto, Arc<GlobalClock>, ComponentId, ReservationStore)> for RmsNodeSimulator {
    type Error = ConversionError;

    fn try_from(args: (DummyRmsDto, Arc<GlobalClock>, ComponentId, ReservationStore)) -> Result<Self, Self::Error> {
        let (dto, simulator, component_id, reservation_store) = args.clone();
        let resource_store = ResourceStore::new();

        let mut nodes = Vec::new();
        let mut schedule_capacity = 0;

        for node_dto in &dto.grid_nodes {
            let node = Node {
                name: ResourceName::new(node_dto.id.clone()),
                cpus: node_dto.cpus,
                connected_to_router: node_dto.connected_to_router.iter().map(|router_id| RouterId::new(router_id)).collect(),
            };

            schedule_capacity += node_dto.cpus;
            nodes.push(node);
        }

        // Add nodes to ResourceStore
        for node in nodes.iter() {
            resource_store.add_node(NodeResource::new(node.name.clone(), node.cpus));
        }

        let name = format!("AcI: {}, RmsType: {}", component_id, dto.typ);
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

        if resource_store.get_num_of_nodes() <= 0 {
            log::info!("Empty Rms: The newly created Rms of type {} of AcI {} contains no Nodes", dto.typ, component_id);
        }

        let base = RmsBase::new(component_id, dto.typ, reservation_store, resource_store.clone());

        Ok(RmsNodeSimulator { base, node_schedule, node_shadow_schedule: HashMap::new() })
    }
}

impl Rms for RmsNodeSimulator {
    fn get_base(&self) -> &RmsBase {
        &self.base
    }

    fn get_base_mut(&mut self) -> &mut RmsBase {
        &mut self.base
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_active_schedule(&self, shadow_schedule_id: Option<ShadowScheduleId>, reservation_id: ReservationId) -> Arc<RwLock<Box<dyn Schedule>>> {
        if self.base.reservation_store.is_node(reservation_id) {
            match shadow_schedule_id {
                Some(id) => self.node_shadow_schedule.get(&id).expect("node_shadow_schedule contains ShadowSchedule.").clone(),
                None => self.node_schedule.clone(),
            }
        } else {
            panic!(
                "RmsSimulatorErrorNoScheduleForReservation: The rms RmsSimulator has no Scheduler for Reservation type {:?}. ReservationName: {:?} ShadowScheduleId {:?}",
                self.base.reservation_store.get_type(reservation_id),
                self.base.reservation_store.get_name_for_key(reservation_id),
                shadow_schedule_id
            );
        }
    }
}

impl AdvanceReservationRms for RmsNodeSimulator {
    fn create_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool {
        if self.node_shadow_schedule.contains_key(shadow_schedule_id) {
            log::error!(
                "Creating new shadow schedule is not possible because shadow schedule id ({}) does already exist. Please first delete the old shadow schedule.",
                shadow_schedule_id
            );
            return false;
        }

        let schedule_clone = self.node_schedule.read().clone_box();
        self.node_shadow_schedule.insert(shadow_schedule_id.clone(), Arc::new(RwLock::new(schedule_clone)));
        return true;
    }

    fn commit_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool {
        if let Some(new_node_schedule) = self.node_shadow_schedule.remove(shadow_schedule_id) {
            self.node_schedule = new_node_schedule;
            return true;
        }

        log::error!("Finding and removing of shadow schedule with id {} was not possible", shadow_schedule_id.clone());
        return false;
    }

    fn delete_shadow_schedule(&mut self, shadow_schedule_id: &ShadowScheduleId) -> bool {
        if self.node_shadow_schedule.remove(shadow_schedule_id).is_some() {
            return true;
        }

        log::error!("Removing shadow schedule was not possible. Shadow schedule id ({}) was not found", shadow_schedule_id.clone());
        return false;
    }

    fn get_fragmentation(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        match shadow_schedule_id {
            Some(id) => {
                self.node_shadow_schedule.get(&id).expect("node_shadow_schedule contains ShadowSchedule.").write().get_fragmentation(start, end)
            }
            None => self.node_schedule.write().get_fragmentation(start, end),
        }
    }

    fn get_system_fragmentation(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        match shadow_schedule_id {
            Some(id) => self.node_shadow_schedule.get(&id).expect("node_shadow_schedule contains ShadowSchedule.").write().get_system_fragmentation(),
            None => self.node_schedule.write().get_system_fragmentation(),
        }
    }

    fn can_handle_adc_request(&self, res: Reservation) -> bool {
        if res.is_node() {
            return self.get_base().resource_store.can_handle_adc_request(res);
        }

        log::debug!(
            "The rms {:?} can not process Reservations of Type {:?} (ReservationName: {:?}) the rms can only process NodeReservations.",
            self.base.id,
            res.get_type(),
            res.get_name()
        );
        return true;
    }

    fn can_handle_aci_request(&self, reservation_store: ReservationStore, reservation_id: ReservationId) -> bool {
        if reservation_store.is_node(reservation_id) {
            return self.get_base().resource_store.can_handle_aci_request(reservation_store, reservation_id);
        }

        log::debug!(
            "The rms {:?} can not process Reservations of Type {:?} (ReservationName: {:?}) the rms can only process NodeReservations.",
            self.base.id,
            reservation_store.get_type(reservation_id),
            reservation_store.get_name_for_key(reservation_id)
        );
        return true;
    }

    fn get_load_metric(&self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        match shadow_schedule_id {
            Some(id) => RmsLoadMetric {
                node_load_metric: Some(
                    self.node_shadow_schedule.get(&id).expect("node_shadow_schedule contains ShadowSchedule.").read().get_load_metric(start, end),
                ),
                link_load_metric: None,
            },
            None => RmsLoadMetric { node_load_metric: Some(self.node_schedule.read().get_load_metric(start, end)), link_load_metric: None },
        }
    }

    fn get_load_metric_up_to_date(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        match shadow_schedule_id {
            Some(id) => RmsLoadMetric {
                node_load_metric: Some(
                    self.node_shadow_schedule
                        .get(&id)
                        .expect("node_shadow_schedule contains ShadowSchedule.")
                        .write()
                        .get_load_metric_up_to_date(start, end),
                ),
                link_load_metric: None,
            },
            None => {
                RmsLoadMetric { node_load_metric: Some(self.node_schedule.write().get_load_metric_up_to_date(start, end)), link_load_metric: None }
            }
        }
    }

    fn get_simulation_load_metric(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        match shadow_schedule_id {
            Some(id) => RmsLoadMetric {
                node_load_metric: Some(
                    self.node_shadow_schedule.get(&id).expect("node_shadow_schedule contains ShadowSchedule.").write().get_simulation_load_metric(),
                ),
                link_load_metric: None,
            },
            None => RmsLoadMetric { node_load_metric: Some(self.node_schedule.write().get_simulation_load_metric()), link_load_metric: None },
        }
    }
}
