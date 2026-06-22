use anyhow::Result;
use async_trait::async_trait;

use crate::api::rms_config_dto::rms_dto::DummyRmsDto;
use crate::domain::vrm_system_model::reservation::reservation::ReservationState;
use crate::domain::vrm_system_model::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::resource::resource_store::ResourceStore;
use crate::domain::vrm_system_model::schedule::schedule_trait::Schedule;
use crate::domain::vrm_system_model::schedule::slotted_schedule::strategy::link::topology::{Link, Node};
use crate::domain::vrm_system_model::utils::id::{AciId, ComponentId, ResourceName, RmsId, RouterId, ShadowScheduleId};
use crate::domain::vrm_system_model::utils::load_buffer::LoadMetric;

use parking_lot::RwLock;
use std::any::Any;
use std::sync::Arc;

pub trait Rms: std::fmt::Debug + Any {
    fn get_base(&self) -> &RmsBase;
    fn get_base_mut(&mut self) -> &mut RmsBase;
    fn as_any(&self) -> &dyn Any;

    /// Finalizes a reservation, marking it as committed.
    ///
    /// This informs the RMS that the user has accepted the reservation and it is fixed.
    /// Committed jobs should not be deleted during normal operation.
    ///
    /// # Note on Implementation
    ///
    /// The default implementation logs the commit and updates the state to `ReservationState::Committed`.
    /// Implementors interfacing with hardware or external APIs should override this to propagate
    /// the commit signal to the physical RMS if necessary.
    ///
    /// # Arguments
    ///
    /// * `reservation_id` - The identifier of the task to commit.
    ///
    /// # Returns
    ///
    /// The `ReservationId` of the committed job.
    fn commit(&self, reservation_id: ReservationId) {
        self.get_base().reservation_store.update_state(reservation_id, ReservationState::Committed);
        log::info!("Committed reservation {:?} successfully to the local RMS", reservation_id);
    }

    /// Should not be necessary in the rust implementation.  
    /// Cancels and deletes a previously submitted reservation.
    ///
    /// This removes the reservation from the local schedule. It is primarily used during
    /// the negotiation phase (before `commit`) or if a user explicitly cancels a task.
    ///
    /// # Arguments
    ///
    /// * `reservation_id` - The ID of the job to delete.
    /// * `shadow_schedule_id` - If `Some`, deletes from the specified shadow schedule.   
    fn delete_task(&mut self, reservation_id: ReservationId, shadow_schedule_id: Option<ShadowScheduleId>) {
        let active_scheduler = self.get_active_schedule(shadow_schedule_id, reservation_id);
        active_scheduler.write().delete_reservation(reservation_id);
    }

    /// Performs the routing to the correct scheduler
    ///
    /// Routs to the node_schedule or link_schedule based on the provided Reservation
    /// (LinkReservation or NodeReservation) of the master or shadowSchedule.
    /// Returns the thread-safe reference to the schedule so the caller can lock it.
    fn get_active_schedule(&self, shadow_schedule_id: Option<ShadowScheduleId>, reservation_id: ReservationId) -> Arc<RwLock<Box<dyn Schedule>>>;

    fn set_reservation_state(&mut self, id: ReservationId, new_state: ReservationState) {
        self.get_base().reservation_store.update_state(id, new_state);
    }
}

#[derive(Debug)]
pub struct RmsBase {
    pub id: RmsId,
    pub resource_store: ResourceStore,
    pub reservation_store: ReservationStore,
}

#[derive(Debug)]
pub struct RmsLoadMetric {
    pub node_load_metric: Option<LoadMetric>,
    pub link_load_metric: Option<LoadMetric>,
}

impl RmsBase {
    pub fn new(component_id: ComponentId, rms_type: String, reservation_store: ReservationStore, resource_store: ResourceStore) -> Self {
        let name = format!("AcI: {}, RmsType: {}", component_id, &rms_type);

        if resource_store.get_num_of_nodes() <= 0 {
            log::info!("Empty Rms: The newly created Rms of type {} of AcI {} contains no Nodes", rms_type, component_id);
        }

        RmsBase { id: RmsId::new(name), resource_store, reservation_store }
    }

    pub fn get_nodes_and_links(dto: &DummyRmsDto) -> (Vec<Node>, Vec<Link>) {
        let mut links = Vec::new();
        let mut nodes = Vec::new();

        for link_dto in &dto.network_links {
            let link = Link {
                id: ResourceName::new(link_dto.id.clone()),
                source: RouterId::new(link_dto.start_point.clone()),
                target: RouterId::new(link_dto.end_point.clone()),
                capacity: link_dto.capacity,
            };

            links.push(link);
        }

        for node_dto in &dto.grid_nodes {
            let node = Node {
                name: ResourceName::new(node_dto.id.clone()),
                cpus: node_dto.cpus,
                connected_to_router: node_dto.connected_to_router.iter().map(|router_id| RouterId::new(router_id)).collect(),
            };

            nodes.push(node);
        }

        return (nodes, links);
    }
}
