use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_container::VrmComponentContainer;
use crate::vrm::rms::rms::RmsLoadMetric;

use std::cmp::Ordering;

/// Compares VrmComponentContainer by the load known to this ADC. It only takes load into
/// account submitted by this ADC unit, so the ordering may differ from the real load ordering.
pub struct LoadCompare {
    start: i64,
    end: i64,
}

impl LoadCompare {
    /// Create new comparator with a given time frame.
    ///
    /// * `start_value`: specifies the beginning of the time frame to use for the comparison.
    /// * `end_value`: specifies the end of the time frame, which is utilized for the comparison.
    pub fn new(start_value: i64, end_value: i64) -> Self {
        Self { start: start_value, end: end_value }
    }

    /// Optimization: Performance Bottleneck: get_load_metric is 2 * N * log(N) times called
    /// Returns `Ordering::Less`, if aci1 has a lower load than aci2
    ///         `Ordering::Greater`, if aci1 has a higher load than aci2
    ///
    /// Note: if load of aci1 and aci2 are equal, is the registration_index of both acis compared.
    ///       In case both acis are the same `Ordering::Equal` is returned.
    pub fn compare(&self, aci1: &VrmComponentContainer, aci2: &VrmComponentContainer) -> Ordering {
        if aci1.registration_index == aci2.registration_index {
            return Ordering::Equal;
        }

        let m1 = aci1.vrm_component.get_load_metric(self.start, self.end, None);
        let m2 = aci2.vrm_component.get_load_metric(self.start, self.end, None);

        // Node + Link, or just Node, or just Link.
        let get_aggregated_utilizaiton = |metric: &RmsLoadMetric| -> f64 {
            match (&metric.node_load_metric, &metric.link_load_metric) {
                (Some(n), Some(l)) => n.utilization + l.utilization,
                (Some(n), None) => n.utilization,
                (None, Some(l)) => l.utilization,
                (None, None) => panic!("No valid RmsMetric was found."),
            }
        };

        let val1 = get_aggregated_utilizaiton(&m1);
        let val2 = get_aggregated_utilizaiton(&m2);

        match val1.partial_cmp(&val2) {
            Some(Ordering::Equal) | None => aci1.registration_index.cmp(&aci2.registration_index),
            Some(ord) => ord,
        }
    }
}
