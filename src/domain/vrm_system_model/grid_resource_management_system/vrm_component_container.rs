use std::sync::Arc;

use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_trait::VrmComponent;
use crate::domain::vrm_system_model::reservation::reservation::Reservation;
use crate::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use crate::domain::vrm_system_model::schedule::schedule_trait::Schedule;
use crate::domain::vrm_system_model::schedule::slotted_schedule::SlottedNodeSchedule;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::node::node_strategy::NodeStrategy;
use crate::domain::vrm_system_model::utils::id::SlottedScheduleId;
use crate::vrm::global_clock::global_clock::GlobalClock;

/// Container holds a VrmComponents (**AcI** or **ADC**) instance and metadata required for sorting and management.
#[derive(Debug)]
pub struct VrmComponentContainer {
    // Contains a AcI or ADC
    pub vrm_component: Box<dyn VrmComponent + Send>,

    // Internal schedule of the VrmComponent (is e.g. a SlottedSchedule)
    pub schedule: Box<dyn Schedule>,

    /// The sequence number assigned at registration time, used for stable sorting.
    pub registration_index: usize,

    /// A counter of how many times operations on this VrmComponent have failed.
    pub failures: u32,

    /// The total bandwidth available on all links of the VrmComponent (does not mean free capacity).
    pub total_link_capacity: i64,

    /// The number of distinct link resources of the VrmComponent.
    pub link_resource_count: usize,
}

impl VrmComponentContainer {
    pub fn new(
        vrm_component: Box<dyn VrmComponent + Send>,
        simulator: Arc<GlobalClock>,
        reservation_store: ReservationStore,
        registration_index: usize,
        number_of_real_slots: i64,
        slot_width: i64,
        total_link_capacity: i64,
        link_resource_count: usize,
    ) -> Self {
        let component_id = vrm_component.get_id();
        let scheduler_id = SlottedScheduleId::new(format!("Scheduler of VrmComponent: {:?}", component_id));
        let total_capacity = vrm_component.get_total_capacity();
        let node_strategy = NodeStrategy::default();
        let slotted_schedule_nodes = SlottedNodeSchedule::new(
            scheduler_id,
            number_of_real_slots,
            slot_width,
            total_capacity,
            false,
            node_strategy,
            reservation_store.clone(),
            simulator.clone(),
        );

        let schedule = Box::new(slotted_schedule_nodes);

        Self { vrm_component, schedule, registration_index, total_link_capacity, link_resource_count, failures: 0 }
    }

    pub fn can_handel(&self, res: Reservation) -> bool {
        self.vrm_component.can_handel(res)
    }
}
