use std::cmp::Ordering;

use crate::vrm::vrm_component::comparator::{
    load_compare::LoadCompare, position_compare::PositionCompare, size_compare::SizeCompare,
};
use crate::vrm::vrm_component::vrm_component_container::VrmComponentContainer;

/// An enum to describe the available ways to sort the registered VrmComponents.
///
/// For each order a Comparator is available and can be generated
/// with [AIOrder::get_comparator].
#[derive(Debug, Clone, Copy)]
pub enum VrmComponentOrder {
    /// VrmComponent order: always start with the first VrmComponent and then proceed in the order of registration.
    OrderStartFirst,

    /// AI order: start with the next AI in every step and then proceed in the order of registration.
    OrderNext(usize),

    /// VrmComponent order: order VrmComponent by known load, start with the VrmComponent with low load
    OrderLoad(i64, i64),

    /// VrmComponent order: order VrmComponent by known load, start with the VrmComponent with high load
    OrderReverseLoad(i64, i64),

    /// VrmComponent order: order VrmComponent by resource size, start with the VrmComponent with highest capacity
    OrderResourceSize,

    /// VrmComponent order: order VrmComponent by resource size, start with the VrmComponent with lowest capacity
    OrderResourceSizeReverse,
}

impl VrmComponentOrder {
    /// Generates a comparator for this order of VrmComponents.
    pub fn get_comparator(&self) -> Box<dyn Fn(&VrmComponentContainer, &VrmComponentContainer) -> Ordering> {
        match *self {
            VrmComponentOrder::OrderStartFirst => {
                let position = PositionCompare::new(0);
                Box::new(move |container1, container2| position.compare(container1, container2))
            }

            VrmComponentOrder::OrderNext(pos) => {
                let position = PositionCompare::new(pos);
                Box::new(move |container1, container2| position.compare(container1, container2))
            }

            VrmComponentOrder::OrderLoad(start, end) => {
                let load = LoadCompare::new(start, end);
                Box::new(move |container1, container2| load.compare(container1, container2))
            }

            VrmComponentOrder::OrderReverseLoad(start, end) => {
                let load = LoadCompare::new(start, end);
                Box::new(move |container1, container2| load.compare(container1, container2).reverse())
            }

            VrmComponentOrder::OrderResourceSize => {
                let size = SizeCompare::new();
                Box::new(move |container1, container2| size.compare(container1, container2))
            }

            VrmComponentOrder::OrderResourceSizeReverse => {
                let size = SizeCompare::new();
                Box::new(move |container1, container2| size.compare(container1, container2).reverse())
            }
        }
    }
}
