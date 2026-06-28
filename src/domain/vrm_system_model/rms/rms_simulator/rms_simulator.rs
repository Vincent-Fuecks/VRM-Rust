use std::{any::Any, collections::HashMap, sync::Arc};

use crate::vrm::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::vrm::resource::resource_store::ResourceStore;
use crate::{
    domain::vrm_system_model::{
        rms::{
            common::{RmsSetupContext, get_nodes_and_links},
            rms::{Rms, RmsBase},
            rms_node_network_trait::Helper,
        },
        schedule::schedule_trait::Schedule,
    },
    schema::rms_dto::RmsSimulatorDto,
    vrm::commons::id::{ComponentId, ShadowScheduleId},
    vrm::global_clock::global_clock::GlobalClock,
};
use parking_lot::RwLock;

/// Simulates both links and nodes of a cluster
#[derive(Debug)]
pub struct RmsSimulator {
    pub base: RmsBase,
    pub node_schedule: Arc<RwLock<Box<dyn Schedule>>>,
    pub network_schedule: Arc<RwLock<Box<dyn Schedule>>>,
    pub node_shadow_schedule: HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>,
    pub network_shadow_schedule: HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>>,
}

impl Rms for RmsSimulator {
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
        if self.base.reservation_store.is_link(reservation_id) {
            match shadow_schedule_id {
                Some(id) => self.network_shadow_schedule.get(&id).expect("network_shadow_schedule contains ShadowSchedule.").clone(),
                None => self.network_schedule.clone(),
            }
        } else if self.base.reservation_store.is_node(reservation_id) {
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

impl RmsSimulator {
    pub fn new(
        dto: RmsSimulatorDto,
        simulator: Arc<GlobalClock>,
        component_id: ComponentId,
        reservation_store: ReservationStore,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let resource_store = ResourceStore::new();
        let (nodes, links, _) = get_nodes_and_links(dto.topology.clone(), Some(dto.compute_nodes));
        let schedule_name = format!("AcI: {}, RmsType: {}", component_id, dto.typ);

        let rms_setup_context = RmsSetupContext::new(
            dto.num_of_slots,
            dto.slot_width,
            reservation_store,
            simulator.clone(),
            nodes,
            links,
            component_id.clone(),
            dto.scheduler_typ,
            schedule_name,
            resource_store,
        );

        let base = rms_setup_context.get_base()?;
        let node_schedule = rms_setup_context.get_node_schedule()?;
        let network_schedule = rms_setup_context.get_network_schedule()?;

        Ok(RmsSimulator {
            base: base,
            node_schedule: node_schedule,
            network_schedule: network_schedule,
            node_shadow_schedule: HashMap::new(),
            network_shadow_schedule: HashMap::new(),
        })
    }
}

impl Helper for RmsSimulator {
    fn get_node_shadow_schedule(&self) -> &HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>> {
        &self.node_shadow_schedule
    }

    fn get_mut_network_shadow_schedule(&mut self) -> &mut HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>> {
        &mut self.network_shadow_schedule
    }

    fn get_network_shadow_schedule(&self) -> &HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>> {
        &self.network_shadow_schedule
    }

    fn get_mut_node_shadow_schedule(&mut self) -> &mut HashMap<ShadowScheduleId, Arc<RwLock<Box<dyn Schedule>>>> {
        &mut self.node_shadow_schedule
    }

    fn get_node_schedule(&self) -> Arc<RwLock<Box<dyn Schedule>>> {
        self.node_schedule.clone()
    }

    fn get_network_schedule(&self) -> Arc<RwLock<Box<dyn Schedule>>> {
        self.network_schedule.clone()
    }

    fn set_node_schedule(&mut self, new_node_schedule: Arc<RwLock<Box<dyn Schedule>>>) {
        self.node_schedule = new_node_schedule;
    }

    fn set_network_schedule(&mut self, new_network_schedule: Arc<RwLock<Box<dyn Schedule>>>) {
        self.network_schedule = new_network_schedule;
    }
}
