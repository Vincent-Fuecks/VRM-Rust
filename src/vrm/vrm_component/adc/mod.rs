mod helpers;
mod vrm_component;

use crate::vrm::common::id::AdcId;
use crate::vrm::reservation::{reservation_store::ReservationStore, reservation_sync_gate::SyncRegistry};
use crate::{
    vrm::global_clock::global_clock::GlobalClock,
    vrm::vrm_component::{
        scheduler::workflow_scheduler::WorkflowScheduler,
        vrm_component_order::VrmComponentOrder,
        vrm_component_registry::{registry_client::RegistryClient, vrm_component_proxy::VrmComponentProxy},
    },
};
use std::sync::Arc;

use super::vrm_component_manager::VrmComponentManager;

/// The **Administrative Domain Controller (ADC)** acts as the central Grid Broker within the VRM system.
///
/// It operates in a dual capacity:
/// 1. **Consumer**: Acts as a reservation submitter to underlying **VrmComponentManager**.
/// 2. **Provider**: Functions as an `VrmComponent` for end-users or higher-level ADCs.
///
/// The ADC provides an abstracted view of all resources within its administrative domain. It handles
/// **Atomic Jobs** by delegating them to the most suitable VrmComponent based on an optimization strategy,
/// and **Complex Workflows** by decomposing them into sub-jobs via the `WorkflowScheduler`.
#[derive(Debug)]
pub struct ADC {
    pub id: AdcId,
    simulator: Arc<GlobalClock>,
    pub reservation_store: ReservationStore,

    /// Registry and management interface for all connected VrmComponents in the domain.
    pub manager: VrmComponentManager,

    /// Registry and management interface for all connected VrmComponents.
    pub registry: RegistryClient,

    /// Logic for decomposing and scheduling workflows.
    pub workflow_scheduler: Option<Box<dyn WorkflowScheduler>>,

    /// Defines the ordering and selection priority for underlying VrmComponents.
    pub vrm_component_order: VrmComponentOrder,

    /// The maximum duration (in seconds) allowed for a reservation to move from 'Reserved' to 'Committed'
    pub commit_timeout: i64,

    /// Total number of discrete scheduling slots available across the domain.
    pub num_of_slots: i64,

    /// The duration of a single resource slot.
    pub slot_width: i64,

    pub sync_registry: SyncRegistry,
}

impl ADC {
    pub fn new(
        adc_id: AdcId,
        vrm_components_list: Vec<VrmComponentProxy>,
        registry: RegistryClient,
        reservation_store: ReservationStore,
        workflow_scheduler: Option<Box<dyn WorkflowScheduler>>,
        vrm_component_order: VrmComponentOrder,
        commit_timeout: i64,
        simulator: Arc<GlobalClock>,
        num_of_slots: i64,
        slot_width: i64,
    ) -> Self {
        let vrm_component_manager =
            VrmComponentManager::new(adc_id.clone(), vrm_components_list, simulator.clone(), reservation_store.clone(), num_of_slots, slot_width);

        ADC {
            id: adc_id,
            manager: vrm_component_manager,
            registry: registry,
            workflow_scheduler: workflow_scheduler,
            reservation_store: reservation_store,
            vrm_component_order: vrm_component_order,
            commit_timeout: commit_timeout,
            simulator: simulator,
            num_of_slots: num_of_slots,
            slot_width: slot_width,
            sync_registry: SyncRegistry::new(),
        }
    }
}
