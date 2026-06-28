use std::{cmp::Ordering, collections::HashMap};

use super::vrm_component_manager::VrmComponentManager;
use crate::domain::vrm_system_model::{grid_resource_management_system::vrm_component_container::VrmComponentContainer, utils::id::ComponentId};
use crate::vrm::reservation::reservation_store::ReservationId;

/**
* A mapping between reservations and the AIs which handle them. In this
 * map the names of the reservations do not have to be unique
 * (see {@link Reservation#getJobName()}.
 *
 * There is the similar class {@link AIReservationDatabase} which can handle only
 * entries with unique names and is therefore saver and faster.
 *
 * @see AIReservationDatabase
 */
pub struct OrderResVrmComponentDatabase {
    pub store: HashMap<ReservationId, ComponentId>,
    res_comparator: Box<dyn Fn(ReservationId, ReservationId) -> Ordering>,
    ai_comparator: Box<dyn Fn(&VrmComponentContainer, &VrmComponentContainer) -> Ordering>,
}

impl OrderResVrmComponentDatabase {
    pub fn new<F1, F2>(res_sort: F1, ai_sort: F2) -> Self
    where
        F1: Fn(ReservationId, ReservationId) -> Ordering + 'static,
        F2: Fn(&VrmComponentContainer, &VrmComponentContainer) -> Ordering + 'static,
    {
        Self { store: HashMap::new(), res_comparator: Box::new(res_sort), ai_comparator: Box::new(ai_sort) }
    }

    /// Adds a reservation and its corresponding AI container.
    pub fn put(&mut self, res: ReservationId, component_id: ComponentId) {
        self.store.insert(res, component_id);
    }

    fn compare_reservations(&self, manager: &VrmComponentManager, res1: ReservationId, res2: ReservationId) -> Ordering {
        let mut order = (self.res_comparator)(res1, res2);

        if order == Ordering::Equal {
            let ai1 = self.store.get(&res1);
            let ai2 = self.store.get(&res2);

            match (ai1, ai2) {
                (Some(a), Some(b)) => {
                    let container0 = manager.vrm_components.get(a).unwrap();

                    let container1 = manager.vrm_components.get(b).unwrap();

                    order = (self.ai_comparator)(container0, container1);
                }
                _ => {
                    panic!("FATAL: Reservations cannot be compared, as they are not elements of this container. {:?}, {:?}", res1, res2);
                }
            }
        }
        order
    }

    pub fn sorted_key_set(&self, manager: &VrmComponentManager) -> Vec<ReservationId> {
        let mut keys: Vec<ReservationId> = self.store.keys().cloned().collect();

        keys.sort_by(|a, b| self.compare_reservations(manager, *a, *b));

        keys
    }
}
