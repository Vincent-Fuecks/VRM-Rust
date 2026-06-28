use std::sync::Arc;

use crate::domain::simulator::simulator::GlobalClock;
use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_manager::VrmComponentContainer;
use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_order::VrmComponentOrder;
use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_registry::vrm_component_proxy::VrmComponentProxy;
use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_trait::VrmComponent;
use crate::domain::vrm_system_model::reservation::reservation::Reservation;
use crate::domain::vrm_system_model::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::domain::vrm_system_model::utils::config::DELETE_ALL_VRM_MANAGED_RESERVATIONS_IF_VRM_COMPONENT_IS_DELETED;
use crate::domain::vrm_system_model::utils::id::{ComponentId, RouterId};

use rand::rng;
use rand::seq::SliceRandom;

use super::VrmComponentManager;

impl VrmComponentManager {
    pub fn get_vrm_component_container_mut(&mut self, component_id: ComponentId) -> &mut VrmComponentContainer {
        match self.vrm_components.get_mut(&component_id) {
            Some(container) => container,
            None => panic!(
                "ErrorFailedToGetVrmComponentContainer: In the VrmComponentManager of ADC {}, was the ComponentId {} not found.",
                self.adc_id,
                component_id.clone()
            ),
        }
    }

    pub fn get_vrm_component_container(&mut self, component_id: ComponentId) -> &VrmComponentContainer {
        match self.vrm_components.get(&component_id) {
            Some(container) => container,
            None => panic!(
                "ErrorFailedToGetVrmComponentContainer: In the VrmComponentManager of ADC {}, was the ComponentId {} not found.",
                self.adc_id,
                component_id.clone()
            ),
        }
    }

    pub fn get_vrm_component_mut(&mut self, component_id: ComponentId) -> &mut Box<dyn VrmComponent + Send + 'static> {
        match self.vrm_components.get_mut(&component_id) {
            Some(container) => &mut container.vrm_component,
            None => panic!(
                "ErrorFailedToGetVrmComponentContainer: In the VrmComponentManager of ADC {}, was the ComponentId {} not found.",
                self.adc_id,
                component_id.clone()
            ),
        }
    }

    pub fn is_reservation_reserved(&self, reservation_id: ReservationId) -> bool {
        self.not_committed_reservations.contains_key(&reservation_id)
    }

    pub fn get_reserved_component(&self, reservation_id: ReservationId) -> Option<ComponentId> {
        self.not_committed_reservations.get(&reservation_id).cloned()
    }

    // Should aggregate hte router list of all components
    pub fn get_component_router_list(&self, component_id: ComponentId) -> Vec<RouterId> {
        self.vrm_components.get(&component_id).unwrap();
        todo!()
    }

    pub fn can_component_handel(&self, component_id: ComponentId, res: Reservation) -> bool {
        match self.vrm_components.get(&component_id) {
            Some(vrm_component) => vrm_component.vrm_component.can_handel(res),

            None => {
                log::debug!(
                    "NotFoundGridComponent: ADC {} requested can_handel request of reservation {}",
                    self.adc_id,
                    res.get_base_reservation().get_name()
                );
                return false;
            }
        }
    }

    // Queues asks all child systems if they can handel all request.
    // Returns true if one of the child systems can handel requests otherwise this function returns false.
    /// Note, is only a feasibility request, does not ensure, that these components have still free capacity in the specified time slot etc.
    pub fn can_handel(&self, reservation_id: ReservationId) -> bool {
        let res_ids = if self.reservation_store.is_workflow(reservation_id) {
            self.reservation_store.get_workflow_res_ids(reservation_id).unwrap_or_default()
        } else {
            vec![reservation_id]
        };

        for res_id in res_ids {
            let mut found_handeler_for_this_id = false;
            if let Some(res) = self.reservation_store.get_reservation_snapshot(res_id) {
                for container in self.vrm_components.values() {
                    if container.can_handel(res.clone()) {
                        found_handeler_for_this_id = true;
                        break;
                    }
                }
            } else {
                log::debug!(
                    "ReservationSnapShotFailed: ADC {} requested can_handle of {:?}",
                    self.adc_id,
                    self.reservation_store.get_name_for_key(res_id)
                );
            }

            // End Task/Sub-Task of Workflow can not be handled by any VrmComponent
            if !found_handeler_for_this_id {
                log::debug!(
                    "CanNotHandelReservation: Vrm can not handel Reservation {:?} {:?}, because  no VrmComponent was found, which can handel Reservation. Reservation requierments: ",
                    self.reservation_store.get_name_for_key(res_id),
                    res_id,
                );
                return false;
            }
        }

        return true;
    }

    /// Get the total capacity of all connected VrmComponents
    pub fn get_total_capacity(&self) -> i64 {
        let mut total_capacity = 0;

        for (_, container) in &self.vrm_components {
            total_capacity += container.vrm_component.get_total_capacity()
        }

        total_capacity
    }

    /// Get the total link capacity of all connected VrmComponents
    pub fn get_total_link_capacity(&self) -> i64 {
        let mut total_link_capacity = 0;

        for (_, container) in &self.vrm_components {
            total_link_capacity += container.vrm_component.get_total_link_capacity()
        }

        total_link_capacity
    }

    /// Get the total node capacity of all connected VrmComponents
    pub fn get_total_node_capacity(&self) -> i64 {
        let mut total_node_capacity = 0;

        for (_, container) in &self.vrm_components {
            total_node_capacity += container.vrm_component.get_total_node_capacity()
        }

        total_node_capacity
    }

    /// Get the link resource_count of all connected VrmComponents
    pub fn get_link_resource_count(&self) -> usize {
        let mut link_resource_count = 0;

        for (_, container) in &self.vrm_components {
            link_resource_count += container.vrm_component.get_link_resource_count()
        }

        link_resource_count
    }

    /// Increments and returns the next available registration counter.
    pub fn get_new_registration_counter(&mut self) -> usize {
        let current = self.registration_counter;
        self.registration_counter += 1;
        return current;
    }

    /// Calculates the average link speed across all registered resources.
    pub fn get_average_link_speed(&self) -> f64 {
        if self.link_resource_count == 0 {
            return 0.0;
        }

        return self.total_link_capacity as f64 / self.link_resource_count as f64;
    }

    /// Registers a new **VrmComponent** with the manager.
    ///
    /// # Arguments
    /// * `vrm_component` - The `VrmComponent` instance to add.
    ///
    /// # Returns
    /// * `true` - If the VrmComponent was successfully added.
    /// * `false` - If the VrmComponent ID already exists or if an insertion error occurred (integrity compromised).
    pub fn add_vrm_component(
        &mut self,
        vrm_component: VrmComponentProxy,
        simulator: Arc<GlobalClock>,
        reservation_store: ReservationStore,
        number_of_real_slots: i64,
        slot_width: i64,
    ) -> bool {
        if self.vrm_components.contains_key(&vrm_component.get_id()) {
            log::error!(
                "Process of adding a new VrmComponent to the VrmComponentManger failed. It is not allowed to add the same VrmComponent multiple times. Please first delete the VrmComponent: {}.",
                vrm_component.get_id()
            );
            return false;
        }

        let vrm_component_id = vrm_component.get_id();
        let total_link_capacity = vrm_component.get_total_link_capacity();
        let link_resource_count = vrm_component.get_link_resource_count();
        let registration_index = self.get_new_registration_counter();

        let container = VrmComponentContainer::new(
            Box::new(vrm_component),
            simulator,
            reservation_store,
            registration_index,
            number_of_real_slots,
            slot_width,
            total_link_capacity,
            link_resource_count,
        );

        if self.vrm_components.insert(vrm_component_id.clone(), container).is_none() {
            return true;
        } else {
            log::error!(
                "Error happened in the process of adding a new VrmComponent: {} to the VrmComponentManager (Adc: {}). The VrmComponentManger is now compromised.",
                vrm_component_id,
                self.adc_id
            );
            return false;
        }
    }

    /// Removes an **VrmComponent** from the manager by its ID.
    ///
    /// Updates the total link capacity and link resource counts upon successful removal.
    ///
    /// # Arguments
    /// * `VrmComponentId` - The identifier of the VrmComponent to remove.
    ///
    /// # Returns
    /// * `true` - If the VrmComponent was found and removed.
    /// * `false` - If the VrmComponent ID was not found.
    pub fn delete_vrm_component(&mut self, del_component_id: ComponentId) -> bool {
        let container = self.vrm_components.remove(&del_component_id);

        match container {
            Some(container) => {
                self.total_link_capacity -= container.total_link_capacity;
                self.link_resource_count -= container.link_resource_count;

                // Delete all managed Reservation by VRM form the VrmComponent
                if DELETE_ALL_VRM_MANAGED_RESERVATIONS_IF_VRM_COMPONENT_IS_DELETED {
                    for (res_id, component_id) in self.res_to_vrm_component.clone() {
                        if del_component_id.eq(&component_id) {
                            if !self.delete_task_at_component(res_id, None) {
                                log::debug!(
                                    "In the process of deleting the VrmComponent {:?}, was it not possible to delete the managed reservation: {:?}.",
                                    del_component_id,
                                    res_id
                                );
                            }
                        }
                    }
                }
                return true;
            }
            None => {
                log::error!(
                    "The process of deleting the VrmComponent: {} form VrmComponentManager (Adc: {}). Failed, because the VrmComponentId was not present in the VrmComponentManager.",
                    del_component_id,
                    self.adc_id
                );
                return false;
            }
        }
    }

    /// Returns a list of all registered VrmComponent IDs in **random order**.
    ///
    /// # Returns
    /// A `Vec<VrmComponentId>` where the VrmComponentIds are in random order.
    pub fn get_random_ordered_vrm_components(&self) -> Vec<ComponentId> {
        let mut keys: Vec<ComponentId> = self.vrm_components.keys().cloned().into_iter().collect();
        keys.shuffle(&mut rng());
        return keys;
    }

    /// Returns a list of registered VrmComponent IDs sorted according to the specified strategy.
    /// If strict ordering is not required, `get_random_ordered_vrm_components` is preferred for performance.
    ///
    /// # Returns
    /// A `Vec<VrmComponentId>` sorted based on the comparator provided by `VrmComponentOrder`.
    pub fn get_ordered_vrm_components(&self, request_order: VrmComponentOrder) -> Vec<ComponentId> {
        let comparator = request_order.get_comparator();
        let mut components_vec: Vec<&VrmComponentContainer> = self.vrm_components.values().collect();

        components_vec.sort_unstable_by(|a, b| comparator(a, b));

        let sorted_keys: Vec<ComponentId> = components_vec.into_iter().map(|container| container.vrm_component.get_id()).collect();
        return sorted_keys;
    }
}
