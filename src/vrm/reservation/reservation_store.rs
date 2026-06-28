use slotmap::{SlotMap, new_key_type};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;

use parking_lot::RwLock;
use std::sync::Arc;

use crate::domain::vrm_system_model::utils::id::{ClientId, ComponentId, ReservationName, RouterId};
use crate::vrm::workflow::workflow::Workflow;
use crate::vrm::workflow::workflow_node::WorkflowNode;

use super::link_reservation::LinkReservation;
use super::reservation::Reservation;
use super::reservation::ReservationProceeding;
use super::reservation::ReservationState;
use super::reservation::ReservationTrait;
use super::reservation::ReservationTyp;
use super::reservation_notification_listener::ReservationNotificationListener;

new_key_type! {
    pub struct ReservationId;
}

/// A thread-safe, indexed repository for managing the lifecycle of resource reservations.
///
/// The `ReservationStore` serves as the central source of truth for all **Link**, **Node**,
/// and **Workflow** reservations in the distributed VRM system. It provides high-performance
/// lookups via multiple indices (Name, Client, and Handler) and supports an observer
/// pattern through `ReservationNotificationListener`.
///
/// ### Thread Safety
/// This store utilizes an `Arc<RwLock<StoreInner>>` pattern, allowing multiple components
/// to read concurrently while ensuring atomic updates during write operations.
#[derive(Debug, Clone)]
pub struct ReservationStore {
    /// Both maps are protected with a single lock.
    inner: Arc<RwLock<StoreInner>>,
}

/// The internal data structure for `ReservationStore`.
///
/// This structure holds the primary data storage and secondary indices required
/// for efficient system-wide queries.
#[derive(Debug, Clone)]
struct StoreInner {
    /// Reservation Storage.
    slots: SlotMap<ReservationId, Arc<RwLock<Reservation>>>,

    /// Index lookup InternalKey (ReservationId) using input reservation name (ReservationName).
    name_index: HashMap<ReservationName, ReservationId>,

    /// Lookup table of all Reservation of a client.
    client_index: HashMap<ClientId, HashSet<ReservationId>>,

    /// Lookup table of all Reservation of a component is currently handling (Acd or AcI).
    handler_index: HashMap<ComponentId, HashSet<ReservationId>>,

    // TODO
    original_to_virtual: HashMap<ReservationId, Vec<ReservationId>>,

    /// Listener for changes
    listeners: Vec<Arc<RwLock<dyn ReservationNotificationListener>>>,
}

impl ReservationStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreInner {
                slots: SlotMap::with_key(),
                name_index: HashMap::new(),
                client_index: HashMap::new(),
                handler_index: HashMap::new(),
                listeners: Vec::new(),
                original_to_virtual: HashMap::new(),
            })),
        }
    }

    /// Subscribes a component to state change notifications.
    /// The listener will be triggered whenever `update_state` is called on a reservation.
    pub fn add_listener(&self, listener: Arc<RwLock<dyn ReservationNotificationListener>>) {
        let mut guard = self.inner.write();
        guard.listeners.push(listener);
    }

    /// Adds Reservation to ReservationStore.
    ///
    /// # Returns
    /// Returns the ReservationId (internal Key for ReservationStore).
    pub fn add(&self, reservation: Reservation) -> ReservationId {
        let mut guard = self.inner.write();

        let name = reservation.get_name().clone();
        let client = reservation.get_client_id().clone();
        let handler = reservation.get_handler_id().clone();

        let key = guard.slots.insert(Arc::new(RwLock::new(reservation)));

        guard.name_index.insert(name, key);
        guard.client_index.entry(client).or_default().insert(key);
        if let Some(h) = handler {
            guard.handler_index.entry(h).or_default().insert(key);
        }

        return key;
    }

    /// Creates a virtual reservation with a modified start point based on an original Link reservation.
    pub fn add_virtual_reservation_diff_start(&self, original_res_id: ReservationId, start: RouterId) -> Option<ReservationId> {
        let mut cloned_reservation = {
            let guard = self.inner.read();
            guard.slots.get(original_res_id).map(|arc_lock| {
                let res_guard = arc_lock.read();
                res_guard.clone()
            })
        }?;

        // Modify original reservation
        match &mut cloned_reservation {
            Reservation::Link(link_res) => {
                let original_res_name = self.get_name_for_key(original_res_id);

                link_res.base.name = ReservationName::new(format!("Original-Res: {:?} | Start: {:?}", original_res_name, start));

                link_res.set_start_point(Some(start));
            }
            _ => {
                log::error!("The provided original reservation (id: {:?}) is not a LinkReservation.", original_res_id);
                return None;
            }
        }

        let virtual_reservation_id = self.add(cloned_reservation);

        // Updated tracking map
        let mut write_guard = self.inner.write();
        write_guard.original_to_virtual.entry(original_res_id).or_default().push(virtual_reservation_id);

        Some(virtual_reservation_id)
    }

    /// Creates a virtual reservation with a modified end point based on an original Link reservation.
    pub fn add_virtual_reservation_diff_end(&self, original_res_id: ReservationId, end: RouterId) -> Option<ReservationId> {
        let mut cloned_reservation = {
            let guard = self.inner.read();
            guard.slots.get(original_res_id).map(|arc_lock| {
                let res_guard = arc_lock.read();
                res_guard.clone()
            })
        }?;

        // Modify original reservation
        match &mut cloned_reservation {
            Reservation::Link(link_res) => {
                let original_res_name = self.get_name_for_key(original_res_id);

                link_res.base.name = ReservationName::new(format!("Original-Res: {:?} | End: {:?}", original_res_name, end));

                link_res.set_end_point(Some(end));
            }
            _ => {
                log::error!("The provided original reservation (id: {:?}) is not a LinkReservation.", original_res_id);
                return None;
            }
        }

        let virtual_reservation_id = self.add(cloned_reservation);

        // Updated tracking map
        let mut write_guard = self.inner.write();
        write_guard.original_to_virtual.entry(original_res_id).or_default().push(virtual_reservation_id);

        Some(virtual_reservation_id)
    }

    /// Removes a reservation and its associated name index from the store.
    /// Note: This operation removes the reservation from the name index and the slot map,
    /// effectively ending its lifecycle in the store.
    pub fn remove(&self, reservation_id: ReservationId) {
        let res_name = self.get_name_for_key(reservation_id);

        if let Some(name) = res_name {
            let mut guard = self.inner.write();
            guard.name_index.remove(&name);
            guard.slots.remove(reservation_id);
        } else {
            log::error!("ReservationStoreRemoveError: Failed to remove reservation, because res_name was None.")
        }
    }

    pub fn remove_virtual_reservation(&self, original_res_id: ReservationId, virtual_res_id: ReservationId) {
        let mut guard = self.inner.write();

        if let std::collections::hash_map::Entry::Occupied(mut entry) = guard.original_to_virtual.entry(original_res_id) {
            let vec = entry.get_mut();

            // Remove virtual reservation
            vec.retain(|&id| id != virtual_res_id);

            // Del if not other virtual reservations present
            if vec.is_empty() {
                entry.remove();
            }
        }
    }

    /// Adds a temporary "Probe" reservation to the store (only allowed by the SlottedScheduleContext logic).
    /// The reservation is immediately deleted.
    pub fn add_probe_reservation(&self, reservation: Reservation) -> ReservationId {
        let mut guard = self.inner.write();
        let name = ReservationName::new(format!("{}-ProbeReservation", reservation.get_name().clone()));
        let key = guard.slots.insert(Arc::new(RwLock::new(reservation)));
        guard.name_index.insert(name, key);

        return key;
    }

    /// Deletes the specialized "Probe" reservation in the store (only allowed by the SlottedScheduleContext logic).
    pub fn delete_probe_reservation(&mut self, reservation_id: ReservationId) {
        let res_state = self.get_state(reservation_id);
        if res_state != ReservationState::ProbeReservation {
            log::error!(
                "ReservationStoreDelError: It was not possible to delete Reservation {:?} from the ReservationStore, because the Reservation was in State {:?} and not in state ReservationState::ProbeReservation can be deleted.",
                self.get_name_for_key(reservation_id),
                res_state
            );
            return;
        }
        let res_name = self.get_name_for_key(reservation_id);

        if let Some(name) = res_name {
            let mut guard = self.inner.write();
            guard.name_index.remove(&name);
            guard.slots.remove(reservation_id);
        } else {
            log::error!("ReservationStoreRemoveError: Failed to remove reservation, because res_name was None.")
        }
    }

    /// Logs the detailed debugging information for a specific reservation.
    pub fn print_reservation(&self, reservation_id: ReservationId) {
        if let Some(handle) = self.get(reservation_id) {
            let guard = handle.read();
            match &*guard {
                Reservation::Link(link_res) => {
                    log::debug!("LinkReservation {:#?}", link_res);
                }
                Reservation::Node(node_res) => {
                    log::debug!("NodeReservation {:#?}", node_res);
                }
                Reservation::Workflow(workflow_res) => {
                    log::debug!("WorkflowReservation {:#?}", workflow_res);
                }
            }
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
        }
    }

    /// Checks if the provided reservation ids are in the ReservationStore
    ///
    /// # Returns
    /// Returns true, if all reservation ids are in the store otherwise false is returned.     
    pub fn contains_reservations(&self, reservation_ids: Vec<ReservationId>) -> bool {
        let guard = self.inner.read();

        for reservation_id in reservation_ids {
            if !guard.slots.contains_key(reservation_id) {
                return false;
            }
        }
        return true;
    }

    /// Get Reservation with internal Id (ReservationId).
    ///  
    /// # Returns
    /// Returns the Some(Reservation) if ReservationId was present in SlotMap else return None.  
    pub fn get(&self, key: ReservationId) -> Option<Arc<RwLock<Reservation>>> {
        let guard = self.inner.read();
        guard.slots.get(key).cloned()
    }

    /// Returns true, if provided ReservationId is in store otherwise return false.
    pub fn contains(&self, reservation_id: ReservationId) -> bool {
        match self.get(reservation_id) {
            Some(_) => true,
            None => false,
        }
    }

    /// Takes a static snapshot (clone) of a specific reservation.
    pub fn get_reservation_snapshot(&self, reservation_id: ReservationId) -> Option<Reservation> {
        let guard = self.inner.read();

        guard.slots.get(reservation_id).map(|arc_lock| {
            let res_guard = arc_lock.read();
            res_guard.clone()
        })
    }

    /// Get Reservation with User reservation name (ReservationName).
    ///  
    /// # Returns
    /// Returns Some(Reservation) if ReservationName was present in SlotMap else return None.  
    pub fn get_by_name(&self, name: &ReservationName) -> Option<Arc<RwLock<Reservation>>> {
        let guard = self.inner.read();
        let key = guard.name_index.get(name)?;
        guard.slots.get(*key).cloned()
    }

    /// Get Reservation user name (ReservationName) with internal reservation id (ReservationId).
    ///  
    /// # Returns
    /// Returns Some(ReservationName) if ReservationId was present in SlotMap else return None.  
    pub fn get_name_for_key(&self, key: ReservationId) -> Option<ReservationName> {
        self.get(key).map(|handle| handle.read().get_name().clone())
    }

    /// Get Reservation id (ReservationId) for user name (ReservationName).
    ///  
    /// # Returns
    /// Returns Some(ReservationId) if ReservationName was present in SlotMap else return None.  
    pub fn get_key_for_name(&self, name: &ReservationName) -> Option<ReservationId> {
        let guard = self.inner.read();
        guard.name_index.get(name).cloned()
    }

    /// Retrieve all keys belonging to a specific Client
    pub fn get_client_reservations(&self, client_id: &ClientId) -> Vec<ReservationId> {
        let guard = self.inner.read();
        guard.client_index.get(client_id).map(|set| set.iter().cloned().collect()).unwrap_or_default()
    }

    /// Retrieve all keys managed by a specific ADC/AI
    pub fn get_managed_reservations(&self, component_id: &ComponentId) -> Vec<ReservationId> {
        let guard = self.inner.read();
        guard.handler_index.get(component_id).map(|set| set.iter().cloned().collect()).unwrap_or_default()
    }

    /// Retrieves form the provided reservation id the reserved_capacity
    pub fn get_reserved_capacity(&self, reservation_id: ReservationId) -> i64 {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_reserved_capacity();
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return 0;
        }
    }

    /// Retrieves form the provided reservation id the start_point, if it is a LinkReservation
    pub fn get_start_point(&self, reservation_id: ReservationId) -> Option<RouterId> {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            let res = res.as_any().downcast_ref::<LinkReservation>();

            return res.unwrap().start_point.clone();
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            self.dump_store_contents(reservation_id);
            return None;
        }
    }

    /// Retrieves form the provided reservation id the end_point, if it is a LinkReservation.
    pub fn get_end_point(&self, reservation_id: ReservationId) -> Option<RouterId> {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            let res = res.as_any().downcast_ref::<LinkReservation>();
            return res.unwrap().end_point.clone();
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return None;
        }
    }

    /// Returns the client_id of the provided reservation_id. Panics if no client id was found.
    pub fn get_client_id(&self, reservation_id: ReservationId) -> ClientId {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_client_id();
        } else {
            panic!("Reservation (id: {:?}) does not contain a client id.", reservation_id);
        }
    }

    /// Returns the handler_id of the provided reservation_id. Panics if no handler_id was found.
    pub fn get_handler_id(&self, reservation_id: ReservationId) -> Option<ComponentId> {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_handler_id();
        } else {
            panic!("Reservation (id: {:?}) does not contain a handler id.", reservation_id);
        }
    }

    /// Returns the assigned_start of the provided reservation_id. Panics if no client id was found.
    pub fn get_assigned_start(&self, reservation_id: ReservationId) -> i64 {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_assigned_start();
        } else {
            panic!("Reservation (id: {:?}) does not contain a assigned end time.", reservation_id);
        }
    }

    /// Returns the assigned_end of the provided reservation_id. Panics if no client id was found.
    pub fn get_assigned_end(&self, reservation_id: ReservationId) -> i64 {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_assigned_end();
        } else {
            panic!("Reservation (id: {:?}) does not contain a assigned end time.", reservation_id);
        }
    }

    /// Returns the state of the provided reservation_id.
    /// Default option if not state was found is Rejected.
    pub fn get_state(&self, reservation_id: ReservationId) -> ReservationState {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_state();
        } else {
            log::error!("Get state for reservation (id: {:?}) was not possible.", reservation_id);
            self.dump_store_contents(reservation_id);
            return ReservationState::Rejected;
        }
    }

    /// Returns the task_duration of the provided reservation_id. Panics if no state was found.
    pub fn get_task_duration(&self, reservation_id: ReservationId) -> i64 {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_task_duration();
        } else {
            panic!("Reservation (id: {:?}) does not contain a assigned end time.", reservation_id);
        }
    }

    /// Returns the ReservationProceeding state of the provided reservation_id.
    /// Default option if no ReservationProceeding was found is Delete.
    pub fn get_reservation_proceeding(&self, reservation_id: ReservationId) -> ReservationProceeding {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_reservation_proceeding();
        } else {
            log::error!("Get reservation_proceeding for reservation (id: {:?}) was not possible.", reservation_id);
            self.dump_store_contents(reservation_id);
            return ReservationProceeding::Delete;
        }
    }

    /// Returns the booking_interval_start of the provided reservation_id. Panics if no value was found.
    pub fn get_booking_interval_start(&self, reservation_id: ReservationId) -> i64 {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_booking_interval_start();
        } else {
            self.dump_store_contents(reservation_id);
            panic!("Reservation (id: {:?}) does not contain a booking interval start time.", reservation_id);
        }
    }

    /// Returns the booking_interval_end of the provided reservation_id. Panics if no value was found.
    pub fn get_booking_interval_end(&self, reservation_id: ReservationId) -> i64 {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_booking_interval_end();
        } else {
            panic!("Reservation (id: {:?}) does not contain a booking interval end time.", reservation_id);
        }
    }

    // Updates the frag_delta value of the corresponding reservation of the provided reservation_id.
    pub fn set_frag_delta(&mut self, reservation_id: ReservationId, frag_delta: f64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.set_frag_delta(frag_delta);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    // Updates the booking_interval_start value of the corresponding reservation of the provided reservation_id.
    pub fn set_booking_interval_start(&mut self, reservation_id: ReservationId, booking_interval_start: i64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.set_booking_interval_start(booking_interval_start);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    // Updates the booking_interval_end value of the corresponding reservation of the provided reservation_id.
    pub fn set_booking_interval_end(&mut self, reservation_id: ReservationId, booking_interval_end: i64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.set_booking_interval_end(booking_interval_end);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    // Updates the assigned_start value of the corresponding reservation of the provided reservation_id.
    pub fn set_assigned_start(&mut self, reservation_id: ReservationId, assigned_start: i64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.set_assigned_start(assigned_start);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    // Updates the assigned_end value of the corresponding reservation of the provided reservation_id.
    pub fn set_assigned_end(&mut self, reservation_id: ReservationId, assigned_end: i64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.set_assigned_end(assigned_end);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    // Updates the reserved_capacity value of the corresponding reservation of the provided reservation_id.
    pub fn set_reserved_capacity(&mut self, reservation_id: ReservationId, reserved_capacity: i64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.set_reserved_capacity(reserved_capacity);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    // Updates the task_duration value of the corresponding reservation of the provided reservation_id.
    pub fn set_task_duration(&mut self, reservation_id: ReservationId, task_duration: i64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.set_task_duration(task_duration);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    // Updates the is_moldable value of the corresponding reservation of the provided reservation_id.
    pub fn set_is_moldable(&mut self, reservation_id: ReservationId, is_moldable: bool) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.set_is_moldable(is_moldable);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    /// Retrieves form the provided reservation id the is_moldable.
    pub fn is_moldable(&self, reservation_id: ReservationId) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.is_moldable();
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    /// Checks if the reservation is of type `Workflow`.
    pub fn is_workflow(&self, reservation_id: ReservationId) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return matches!(res.get_type(), ReservationTyp::Workflow);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    /// Checks if the reservation is of type `Link`.
    pub fn is_link(&self, reservation_id: ReservationId) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return matches!(res.get_type(), ReservationTyp::Link);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    /// Checks if the reservation is of type `Node`.
    pub fn is_node(&self, reservation_id: ReservationId) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return matches!(res.get_type(), ReservationTyp::Node);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    /// Compares the reservation's current proceeding state against a target.
    pub fn is_reservation_proceeding(&self, reservation_id: ReservationId, reservation_proceeding: ReservationProceeding) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_reservation_proceeding() == reservation_proceeding;
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    /// Checks if the reservation is in a state where a `ReserveRequest` is valid.
    pub fn is_reserve_request_valid(&self, reservation_id: ReservationId) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_state().is_reserve_request_valid();
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    pub fn is_reservation_at_cycle_end(&self, reservation_id: ReservationId) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return res.get_state().is_reservation_at_cycle_end(res.get_reservation_proceeding());
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    /// Returns the `ReservationTyp` (Link, Node, or Workflow) for the given ID.
    pub fn get_type(&self, reservation_id: ReservationId) -> Option<ReservationTyp> {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            return Some(res.get_type());
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return None;
        }
    }

    /// Calculates the upward rank of a workflow for scheduling priority.
    /// This is only valid if the `ReservationId` points to a `Workflow` type.
    pub fn get_upward_rank(&self, reservation_id: ReservationId, average_link_speed: i64) -> Option<Vec<WorkflowNode>> {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.write();

            if let Some(workflow) = res.as_any().downcast_ref::<Workflow>() {
                return Some(workflow.clone().calculate_upward_rank(average_link_speed, self));
            } else {
                log::error!(
                    "Upward Rank can only be calculated for a Reservation of type Workflow. Reservation {:?} has type {:?}",
                    self.get_name_for_key(reservation_id),
                    self.get_type(reservation_id)
                );
            }
        }

        return None;
    }

    /// Returns a list of all child reservation IDs if the provided reservation_id is of type `Workflow`.
    pub fn get_workflow_res_ids(&self, reservation_id: ReservationId) -> Option<Vec<ReservationId>> {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.write();
            if let Some(workflow) = res.as_any().downcast_ref::<Workflow>() {
                return Some(workflow.get_all_reservation_ids());
            } else {
                log::error!(
                    "Getting workflow ids is only possible, if Reservation is of type Workflow. Reservation {:?} has type {:?}",
                    self.get_name_for_key(reservation_id),
                    self.get_type(reservation_id)
                );
            }
        }

        return None;
    }

    pub fn get_workflow_entry_res_ids(&self, reservation_id: ReservationId) -> Option<Vec<ReservationId>> {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            if let Some(workflow) = res.as_any().downcast_ref::<Workflow>() {
                let mut entry_res_ids: Vec<ReservationId> = vec![];

                for entry_node in workflow.entry_nodes.clone() {
                    entry_res_ids.push(workflow.nodes.get(&entry_node).unwrap().reservation_id);
                }

                return Some(entry_res_ids);
            } else {
                log::error!(
                    "Getting workflow ids is only possible, if Reservation is of type Workflow. Reservation {:?} has type {:?}",
                    self.get_name_for_key(reservation_id),
                    self.get_type(reservation_id)
                );
            }
        }

        return None;
    }

    pub fn get_workflow_exit_res_ids(&self, reservation_id: ReservationId) -> Option<Vec<ReservationId>> {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            if let Some(workflow) = res.as_any().downcast_ref::<Workflow>() {
                let mut exit_res_ids: Vec<ReservationId> = vec![];

                for exit_node in workflow.exit_nodes.clone() {
                    exit_res_ids.push(workflow.nodes.get(&exit_node).unwrap().reservation_id);
                }

                return Some(exit_res_ids);
            } else {
                log::error!(
                    "Getting workflow ids is only possible, if Reservation is of type Workflow. Reservation {:?} has type {:?}",
                    self.get_name_for_key(reservation_id),
                    self.get_type(reservation_id)
                );
            }
        }

        return None;
    }

    pub fn is_res_commit_ready(&self, reservation_id: ReservationId) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();

            // Does ProceedingState allow commit
            if !self.is_reservation_proceeding(reservation_id, ReservationProceeding::Commit) {
                return false;
            }

            // Does ReservationState allow commit
            if !matches!(self.get_state(reservation_id), ReservationState::Open | ReservationState::ReserveAnswer) {
                return false;
            }

            match res.get_type() {
                ReservationTyp::Node => {
                    for data_dependency in res.as_node().unwrap().data_dependencies.iter() {
                        if !matches!(self.get_state(*data_dependency), ReservationState::Finished) {
                            return false;
                        }
                    }
                    return true;
                }
                ReservationTyp::Link => {
                    log::debug!("LinkReservation was committed directly, because it is not possible to reserve links on the RMS site.");
                    return true;
                }
                ReservationTyp::Workflow => {
                    return true;
                }
            }
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    /// Evaluates if a specific reservation has reached or exceeded a target
    /// level of commitment in the distributed lifecycle.
    ///
    /// # Parameters
    /// * `reservation_id` - The unique identifier of the reservation to check.
    /// * `state` - The minimum required `ReservationState` to compare against.
    ///
    /// # Returns
    /// `true` if current state >= `state`.
    pub fn is_reservation_state_at_least(&self, reservation_id: ReservationId, state: ReservationState) -> bool {
        if let Some(handle) = self.get(reservation_id) {
            let res = handle.read();
            if res.get_state() >= state { true } else { false }
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id);
            return false;
        }
    }

    /// Adjusts the reserved capacity of a reservation by the provided amount.
    pub fn adjust_capacity(&self, reservation_id: ReservationId, capacity: i64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.adjust_capacity(capacity);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    /// Adjusts the task duration of a reservation by the provided amount.
    pub fn adjust_task_duration(&self, reservation_id: ReservationId, duration: i64) {
        if let Some(handle) = self.get(reservation_id) {
            let mut res = handle.write();
            res.adjust_task_duration(duration);
        } else {
            log::error!("Get reservation (id: {:?}) was not possible.", reservation_id)
        }
    }

    /// Atomically updates the state of a reservation and notifies all listeners.
    ///
    /// The mutation and listener notification happen under the same lock acquisition
    /// to prevent deadlocks when listeners re-enter the store.
    pub fn update_state(&self, id: ReservationId, new_state: ReservationState) {
        let (old_state, name, should_notify) = {
            let guard = self.inner.read();
            if let Some(res_lock) = guard.slots.get(id) {
                let mut res = res_lock.write();
                let old = res.get_state();
                let res_name = res.get_name();
                res.set_state(new_state);
                (old, Some(res_name), true)
            } else {
                log::error!("update_state failed: reservation (id: {:?}) not found.", id);
                (new_state, None, false)
            }
        };

        if should_notify {
            let listeners = {
                let guard = self.inner.read();
                guard.listeners.clone()
            };

            if let Some(res_name) = name {
                for listener in listeners {
                    listener.write().on_reservation_change(id, res_name.clone(), old_state, new_state);
                }
            }
        }
    }

    /// Provides mutable access to a workflow for scheduling purposes.
    pub fn with_workflow_mut<F, R>(&self, reservation_id: ReservationId, f: F) -> Option<R>
    where
        F: FnOnce(&mut Workflow) -> R,
    {
        if let Some(handle) = self.get(reservation_id) {
            let mut guard = handle.write();
            guard.as_workflow_mut().map(f)
        } else {
            log::error!("with_workflow_mut: reservation (id: {:?}) not found.", reservation_id);
            None
        }
    }

    /// Sorts the provided Reservation Ids by there arrival time (ascending)
    pub fn get_sorted_res_ids_with_arrival_time(&self, reservation_ids: Vec<ReservationId>) -> Vec<(ReservationId, i64)> {
        let guard = self.inner.read();

        let mut res_id_arrival_time_list = Vec::new();
        for res_id in reservation_ids {
            let res = guard.slots.get(res_id).expect("Reservation should exist in store.");
            res_id_arrival_time_list.push((res_id, res.read().get_arrival_time()));
        }
        res_id_arrival_time_list.iter().is_sorted_by(|a, b| a.1 <= b.1);
        return res_id_arrival_time_list;
    }

    /// Creates a "Shadow" copy of the store.
    ///
    /// This creates a deep copy of all reservations to allow isolated modification.
    /// This means a Scheduler can work on the Shadow Store using the same Keys
    /// as the Master Store, but changes will not affect the Master.
    /// Note: ReservationStore snapshot has no active Listeners.
    pub fn snapshot(&self) -> ReservationStore {
        let guard = self.inner.read();
        let mut new_slots = guard.slots.clone();

        for (_, arc_lock) in new_slots.iter_mut() {
            let original_res = arc_lock.read().clone();
            *arc_lock = Arc::new(RwLock::new(original_res));
        }

        let new_inner = StoreInner {
            slots: new_slots,
            name_index: guard.name_index.clone(),
            client_index: guard.client_index.clone(),
            handler_index: guard.handler_index.clone(),
            listeners: guard.listeners.clone(),
            original_to_virtual: guard.original_to_virtual.clone(),
        };

        ReservationStore { inner: Arc::new(RwLock::new(new_inner)) }
    }

    /// Dumps the current contents of the store to the error log for emergency diagnostics.
    pub fn dump_store_contents(&self, reservation_id: ReservationId) {
        let handles: Vec<(ReservationId, Arc<RwLock<Reservation>>)> = {
            let guard = self.inner.read();
            guard.slots.iter().map(|(id, handle)| (id, handle.clone())).collect()
        };

        log::error!("=== RESERVATION STORE DUMP ({} entries) ===", handles.len());
        log::error!("=== Panic by Reservation ID: {:?}, Name: {:?} ===", reservation_id, self.get_name_for_key(reservation_id));

        for (id, res_handle) in handles {
            match res_handle.try_read_for(std::time::Duration::from_millis(50)) {
                Some(res) => {
                    log::error!(
                        "  -> ID: {:?} | Name: {:?} | State: {:?} | Type: {:?} | Proceeding: {:?}",
                        id,
                        res.get_name(),
                        res.get_state(),
                        res.get_type(),
                        res.get_reservation_proceeding()
                    );
                }
                None => {
                    log::error!("  -> ID: {:?} | [Lock Busy/Deadlocked]", id);
                }
            }
        }
        log::error!("=== END OF RESERVATION STORE ===");
    }

    /// Prints the current contents of the store to the info log is used for the program presentation.
    pub fn print_store_contents(&self) {
        let handles: Vec<(ReservationId, Arc<RwLock<Reservation>>)> = {
            let guard = self.inner.read();
            guard.slots.iter().map(|(id, handle)| (id, handle.clone())).collect()
        };

        log::info!("=== RESERVATION STORE ({} entries) ===", handles.len());

        for (id, res_handle) in handles {
            match res_handle.try_read_for(std::time::Duration::from_millis(50)) {
                Some(res) => {
                    log::info!(
                        "  -> ID: {:?} | Name: {:?} | State: {:?} | Type: {:?} | Proceeding: {:?}",
                        id,
                        res.get_name(),
                        res.get_state(),
                        res.get_type(),
                        res.get_reservation_proceeding()
                    );
                }
                None => {
                    log::warn!("  -> ID: {:?} | [Lock Busy/Deadlocked]", id);
                }
            }
        }
        log::info!("=== END OF RESERVATION STORE ===");
    }
}
