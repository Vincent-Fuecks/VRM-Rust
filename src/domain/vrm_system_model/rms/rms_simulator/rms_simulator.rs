use std::{any::Any, collections::HashMap, str::FromStr, sync::Arc};

use parking_lot::RwLock;

use crate::{
    api::rms_config_dto::rms_dto::DummyRmsDto,
    domain::{
        simulator::simulator::GlobalClock,
        vrm_system_model::{
            reservation::reservation_store::{ReservationId, ReservationStore},
            resource::{node_resource::NodeResource, resource_store::ResourceStore},
            rms::{
                rms::{Rms, RmsBase},
                rms_node_network_trait::Helper,
            },
            schedule::{schedule_trait::Schedule, slotted_schedule::strategy::link::topology::NetworkTopology},
            scheduler_type::{ScheduleContext, SchedulerType},
            utils::id::{AciId, RouterId, ShadowScheduleId, SlottedScheduleId},
        },
    },
    error::ConversionError,
};

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

impl TryFrom<(DummyRmsDto, Arc<GlobalClock>, AciId, ReservationStore)> for RmsSimulator {
    type Error = ConversionError;

    fn try_from(args: (DummyRmsDto, Arc<GlobalClock>, AciId, ReservationStore)) -> Result<Self, Self::Error> {
        let (dto, simulator, aci_id, reservation_store) = args.clone();
        let resource_store = ResourceStore::new();
        let (nodes, links) = RmsBase::get_nodes_and_links(&dto);
        let entry_points = dto.entry_points.iter().into_iter().map(|e_point| RouterId::new(e_point)).collect();

        // Setup RmsNodeSimulator
        let mut schedule_capacity = 0;

        // Add nodes to ResourceStore
        for node in nodes.iter() {
            schedule_capacity += node.cpus;
            resource_store.add_node(NodeResource::new(node.name.clone(), node.cpus));
        }

        let name = format!("AcI: {}, RmsType: {}", aci_id, dto.typ);
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

        // Setup RmsNetworkSimulator
        // Adds Links to Resource Store
        let topology = NetworkTopology::new(
            &links,
            &nodes,
            dto.slot_width,
            dto.num_of_slots,
            simulator.clone(),
            aci_id.clone(),
            reservation_store.clone(),
            resource_store.clone(),
            entry_points,
        );

        let name = format!("AcI: {}, RmsType: {}", aci_id, dto.typ);
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

        if resource_store.get_num_of_nodes() <= 0 {
            log::info!("Empty Rms: The newly created Rms of type {} of AcI {} contains no Nodes", dto.typ, aci_id);
        }

        let base = RmsBase::new(aci_id, dto.typ, reservation_store, resource_store.clone());

        Ok(RmsSimulator { base, node_schedule, network_schedule, node_shadow_schedule: HashMap::new(), network_shadow_schedule: HashMap::new() })
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
