use crate::vrm::schedule::slotted_schedule::{
    slotted_schedule_context::SlottedScheduleContext,
    strategy::{link::link_strategy::LinkStrategy, node::node_strategy::NodeStrategy},
};

pub mod fragmentation;
pub mod schedule_base;
pub mod slot;
pub mod slotted_schedule_context;
pub mod strategy;

pub type SlottedNodeSchedule = SlottedScheduleContext<NodeStrategy>;
pub type SlottedLinkSchedule = SlottedScheduleContext<LinkStrategy>;
