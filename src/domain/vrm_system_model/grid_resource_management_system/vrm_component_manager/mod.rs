use std::collections::HashMap;
use std::sync::Arc;

use super::vrm_component_container::VrmComponentContainer;
use super::vrm_component_registry::vrm_component_proxy::VrmComponentProxy;
use super::vrm_component_trait::VrmComponent;
use crate::domain::vrm_system_model::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::utils::id::{AdcId, ComponentId, ShadowScheduleId};
use crate::vrm::global_clock::global_clock::GlobalClock;

pub mod core;
pub mod metrics;
pub mod scheduling;
pub mod shadow;
pub mod tracking;

/// Manages a collection of **VrmComponents (ADCs and/or AcIs)** for a specific **ADC**.
///
/// The `VrmComponentManager` acts as a central registry and aggregator for distributed resources. It handles:
/// * Registration and deregistration of VrmComponents.
/// * Aggregation of system-wide metrics (Satisfaction, Load).
/// * Retrieval of VrmComponents based on specific ordering strategies (Random, Load-balanced, etc.).
///
/// # Distributed Context & Synchronization
///
/// This manager operates within a distributed Grid/VRM system. While `VrmComponentManager` provides a local view
/// of the resources, operations performed on the contained `VrmComponents` objects involve network communication
/// with remote entities (ADCs local, AcIs remote). Callers should assume that state changes (like load updates)
/// require synchronization with the remote AcIs.
#[derive(Debug)]
pub struct VrmComponentManager {
    /// The ID of the ADC owning this manager.
    pub adc_id: AdcId,

    /// Map of `VrmComponentId` to their container wrappers.
    pub vrm_components: HashMap<ComponentId, VrmComponentContainer>,

    // --- Reservation Tracking (The "Who has What") ---
    /// Maps a `ReservationId` (Atomic Job or Workflow Subtask) to the `VrmComponentId` that handles it.
    pub res_to_vrm_component: HashMap<ReservationId, ComponentId>,

    /// Contains all commit reservations --> All reservations on the master schedule
    pub committed_reservations: HashMap<ReservationId, ComponentId>,

    /// Contains all not commit reservations --> All reservations on the master schedule
    pub not_committed_reservations: HashMap<ReservationId, ComponentId>,

    pub shadow_schedule_reservations: HashMap<ShadowScheduleId, (HashMap<ReservationId, ComponentId>, ReservationStore)>,

    /// Maps a `WorkflowId` (Parent) to a list of its sub-reservations (Nodes and Links).
    pub workflow_subtasks: HashMap<ReservationId, Vec<ReservationId>>,

    /// Maps a Subtask `ReservationId` back to its Parent `WorkflowId`.
    pub reverse_workflow_subtasks: HashMap<ReservationId, ReservationId>,

    /// The aggregated sum of link capacities of all registered AcIs (does not mean free capacity).
    pub total_link_capacity: i64,

    /// The aggregated sum distinct link resources of all registered AcIs.
    pub link_resource_count: usize,

    /// Monotonic counter used to assign `registration_index` to new VrmComponentContainer's.
    registration_counter: usize,

    /// Is used to create an empty Reservations struct as return value for an unsuccessful probe request
    pub reservation_store: ReservationStore,

    pub simulator: Arc<GlobalClock>,
}

impl VrmComponentManager {
    pub fn new(
        adc_id: AdcId,
        vrm_components_list: Vec<VrmComponentProxy>,
        simulator: Arc<GlobalClock>,
        reservation_store: ReservationStore,
        number_of_real_slots: i64,
        slot_width: i64,
    ) -> Self {
        let mut vrm_components = HashMap::with_capacity(vrm_components_list.len());
        let mut registration_counter = 0;
        let mut manager_total_link_capacity = 0;
        let mut manager_link_resource_count = 0;

        for vrm_component in vrm_components_list {
            let component_id = vrm_component.get_id().clone();
            let total_link_capacity = vrm_component.get_total_link_capacity();
            let link_resource_count = vrm_component.get_link_resource_count();

            manager_total_link_capacity += total_link_capacity;
            manager_link_resource_count += link_resource_count;

            let container = VrmComponentContainer::new(
                Box::new(vrm_component),
                simulator.clone(),
                reservation_store.clone(),
                registration_counter,
                number_of_real_slots,
                slot_width,
                total_link_capacity,
                link_resource_count,
            );

            registration_counter += 1;
            vrm_components.insert(component_id, container);
        }

        VrmComponentManager {
            adc_id,
            vrm_components,
            res_to_vrm_component: HashMap::new(),
            committed_reservations: HashMap::new(),
            not_committed_reservations: HashMap::new(),
            shadow_schedule_reservations: HashMap::new(),
            workflow_subtasks: HashMap::new(),
            reverse_workflow_subtasks: HashMap::new(),
            total_link_capacity: manager_total_link_capacity,
            link_resource_count: manager_link_resource_count,
            registration_counter,
            reservation_store: reservation_store.clone(),
            simulator: simulator.clone(),
        }
    }
}
