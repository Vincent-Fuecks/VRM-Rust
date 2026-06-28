use crate::domain::vrm_system_model::schedule::schedule_trait::Schedule;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::link::link_strategy::LinkStrategy;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::link::topology::NetworkTopology;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::node::node_strategy::NodeStrategy;
use crate::vrm::commons::id::SlottedScheduleId;
use crate::vrm::reservation::reservation_store::ReservationStore;
use crate::vrm::resource::resource_store::ResourceStore;

use crate::error::ConversionError;
use crate::vrm::global_clock::global_clock::GlobalClock;
use std::str::FromStr;
use std::sync::Arc;

use super::schedule::slotted_schedule::{SlottedLinkSchedule, SlottedNodeSchedule};

#[derive(Debug, Clone)]
pub enum SchedulerType {
    // Node Scheduler
    FreeListSchedule,
    SlottedSchedule,
    SlottedSchedule12,
    SlottedSchedule12000,
    UnlimitedSchedule,

    // Link Scheduler
    SlottedScheduleLinks { topology: NetworkTopology, resource_store: ResourceStore },
}
#[derive(Debug, Clone)]
pub struct ScheduleContext {
    pub id: SlottedScheduleId,
    pub number_of_slots: i64,
    pub slot_width: i64,
    pub capacity: i64,
    pub simulator: Arc<GlobalClock>,
    pub reservation_store: ReservationStore,
}

impl FromStr for SchedulerType {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "FreeListSchedule" => Ok(SchedulerType::FreeListSchedule),
            "SlottedSchedule" => Ok(SchedulerType::SlottedSchedule),
            "SlottedSchedule12" => Ok(SchedulerType::SlottedSchedule12),
            "SlottedSchedule12000" => Ok(SchedulerType::SlottedSchedule12000),
            "UnlimitedSchedule" => Ok(SchedulerType::UnlimitedSchedule),
            _ => Err(ConversionError::UnknownSchedulerType(s.to_string())),
        }
    }
}

impl SchedulerType {
    // Factory method to create a concrete Schedule implementation
    pub fn get_instance(&self, ctx: ScheduleContext) -> Box<dyn Schedule> {
        match self {
            Self::FreeListSchedule => {
                todo!()
            }
            Self::SlottedSchedule => {
                let node_strategy = NodeStrategy::default();
                let node_schedule = SlottedNodeSchedule::new(
                    ctx.id,
                    ctx.number_of_slots,
                    ctx.slot_width,
                    ctx.capacity,
                    true,
                    node_strategy,
                    ctx.reservation_store.clone(),
                    ctx.simulator.clone(),
                );

                Box::new(node_schedule)
            }
            Self::SlottedScheduleLinks { topology, resource_store } => {
                let link_strategy = LinkStrategy::new(topology.clone(), resource_store.clone());
                let link_schedule = SlottedLinkSchedule::new(
                    ctx.id,
                    ctx.number_of_slots,
                    ctx.slot_width,
                    ctx.capacity,
                    true,
                    link_strategy,
                    ctx.reservation_store.clone(),
                    ctx.simulator.clone(),
                );

                Box::new(link_schedule)
            }
            Self::SlottedSchedule12 => {
                let number_of_real_slots = (ctx.number_of_slots * (ctx.slot_width + 11)) / 12;
                let node_strategy = NodeStrategy::default();
                let node_schedule = SlottedNodeSchedule::new(
                    ctx.id,
                    number_of_real_slots,
                    ctx.slot_width,
                    12,
                    true,
                    node_strategy,
                    ctx.reservation_store.clone(),
                    ctx.simulator.clone(),
                );

                Box::new(node_schedule)
            }
            Self::SlottedSchedule12000 => {
                let number_of_real_slots = (ctx.number_of_slots * (ctx.slot_width + 11999)) / 12000;
                let node_strategy = NodeStrategy::default();
                let node_schedule = SlottedNodeSchedule::new(
                    ctx.id,
                    number_of_real_slots,
                    ctx.slot_width,
                    1200,
                    true,
                    node_strategy,
                    ctx.reservation_store.clone(),
                    ctx.simulator.clone(),
                );

                Box::new(node_schedule)
            }
            Self::UnlimitedSchedule => {
                todo!()
            }
        }
    }

    pub fn get_network_scheduler_variant(&self, topology: NetworkTopology, resource_store: ResourceStore) -> SchedulerType {
        match self {
            Self::SlottedSchedule => SchedulerType::SlottedScheduleLinks { topology, resource_store },
            _ => {
                log::error!("The specified Scheduler {:?} is not implemented as NetworkScheduler. Default to SlottedSchedule", self);
                SchedulerType::SlottedScheduleLinks { topology, resource_store }
            }
        }
    }
}
