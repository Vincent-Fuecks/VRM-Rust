use crate::vrm::commons::id::{ComponentId, ShadowScheduleId};
use crate::vrm::schedule::load_buffer::LoadMetric;
use crate::vrm::rms::rms::RmsLoadMetric;

use super::VrmComponentManager;

impl VrmComponentManager {
    /// Calculates the average **Satisfaction Score** (0.0 to 1.0) for the current schedule within a specific time window.
    /// This method queries all directly and indirectly connected AcIs and calculates the capacity-weighted average satisfaction.
    ///
    /// # Arguments
    /// * `start` - The start of the time window.
    /// * `end` - The end of the time window.
    /// * `shadow_schedule_id` - Optional ID. If provided, calculates based on the specified shadow schedule; otherwise uses the master schedule.
    ///
    /// # Returns
    /// A `f64` value between 0.0 (worst case) and 1.0 (best case). Returns 0.0 if total capacity is 0.
    pub fn get_satisfaction(&mut self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        log::debug!(
            "ADC: {} requests satisfaction of all AcIs with the ShadowScheduleId: {:?} the time window start: {} to end: {}",
            self.adc_id,
            shadow_schedule_id.clone(),
            start,
            end
        );

        let mut satisfaction_sum = 0.0;
        let mut total_capacity = 0.0;

        for (id, container) in self.vrm_components.iter_mut() {
            let satisfaction = container.vrm_component.get_satisfaction(start, end, shadow_schedule_id.clone());

            if satisfaction < 0.0 {
                log::debug!(
                    "Satisfaction of AcI is not allowed to be negative. ADC: {}, AcIs:  {} with ShadowScheduleId: {:?}",
                    self.adc_id,
                    id,
                    shadow_schedule_id
                );
            } else {
                let cap = container.vrm_component.get_total_node_capacity() as f64;
                satisfaction_sum += satisfaction * cap;
                total_capacity += cap;
            }
        }

        return if total_capacity > 0.0 { satisfaction_sum / total_capacity } else { 0.0 };
    }

    /// Calculates the system-wide **Satisfaction Score** (0.0 to 1.0) across the full range of every schedule.
    /// This method queries all directly and indirectly connected AcIs and calculates the capacity-weighted average.
    ///
    /// # Behavioral Note
    /// **Network AcIs:** This calculation generally excludes network AIs if their satisfaction/fragmentation
    /// functions are not implemented (returning -1). These are filtered out to prevent skewing the system metric.
    ///
    /// # Arguments
    /// * `shadow_schedule_id` - Optional ID. If provided, calculates based on the specified shadow schedule.
    ///                          (If None utilize master schedule)
    ///
    /// # Returns
    /// A `f64` value between 0.0 (worst case) and 1.0 (best case).
    pub fn get_system_satisfaction(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> f64 {
        log::debug!("ADC: {} requests system satisfaction of all AcIs with the ShadowScheduleId: {:?}.", self.adc_id, shadow_schedule_id.clone());

        let mut satisfaction_sum = 0.0;
        let mut total_capacity = 0.0;

        for (id, container) in self.vrm_components.iter_mut() {
            let satisfaction = container.vrm_component.get_system_satisfaction(shadow_schedule_id.clone());
            if satisfaction < 0.0 {
                log::debug!(
                    "System satisfaction of AcI is not allowed to be negative. ADC: {}, AcIs:  {} with ShadowScheduleId: {:?}",
                    self.adc_id,
                    id,
                    shadow_schedule_id
                );
            } else {
                let cap = container.vrm_component.get_total_node_capacity() as f64;
                satisfaction_sum += satisfaction * cap;
                total_capacity += cap;
            }
        }

        return if total_capacity > 0.0 { satisfaction_sum / total_capacity } else { 0.0 };
    }

    fn calculate_averge_load_metric(
        &self,
        shadow_schedule_id: Option<ShadowScheduleId>,
        metricis: Vec<(ComponentId, Option<LoadMetric>)>,
    ) -> Option<LoadMetric> {
        let mut total_possible_reserved_capacity = 0.0;
        let mut total_average_reserved_capacity = 0.0;
        let mut earliest_start = i64::MAX;
        let mut latest_end = i64::MIN;
        let mut num_of_valid_components = 0;

        for metric in metricis {
            if let (id, Some(load_matic)) = metric {
                if load_matic.start_time < 0 {
                    log::debug!(
                        "Get Load Metric with negative start time is not allowed. ADC: {}, child VrmComponent:  {} with ShadowScheduleId: {:?}",
                        self.adc_id,
                        id,
                        shadow_schedule_id
                    );
                } else {
                    total_average_reserved_capacity += load_matic.avg_reserved_capacity;
                    total_possible_reserved_capacity += load_matic.possible_capacity;
                    num_of_valid_components += 1;

                    if earliest_start > load_matic.start_time {
                        earliest_start = load_matic.start_time;
                    }

                    if latest_end < load_matic.end_time {
                        latest_end = load_matic.end_time;
                    }
                }
            }
        }

        let mut utilization: f64 = 0.0;
        if total_possible_reserved_capacity > 0.0 {
            utilization = total_average_reserved_capacity / total_possible_reserved_capacity;
        }

        if num_of_valid_components > 0 {
            return Some(LoadMetric::new(
                earliest_start,
                latest_end,
                total_average_reserved_capacity / num_of_valid_components as f64,
                total_possible_reserved_capacity / num_of_valid_components as f64,
                utilization,
            ));
        } else {
            return None;
        }
    }

    /// Computes the **Load Metric** for a specific time range.
    /// This method aggregates the load of all directly and indirectly connected AcIs.
    /// **Note:** Only jobs submitted via this ADC are typically counted; actual load on the physical resource
    /// may be higher due to local jobs or other ADCs.
    ///
    /// # Arguments
    /// * `start` - Start of the analysis window in seconds (VRM Time).
    /// * `end` - End of the analysis window in seconds (VRM Time).
    /// * `shadow_schedule_id` - Optional ID for shadow schedule analysis (If None utilize master schedule).
    ///
    /// # Returns
    /// A `LoadMetric` struct containing utilization, start/end times, and capacity details.
    pub fn get_load_metric(&self, start: i64, end: i64, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        let mut node_metricis = Vec::new();
        let mut network_metricis = Vec::new();

        for (id, container) in self.vrm_components.iter() {
            let load_matic = container.vrm_component.get_load_metric(start, end, shadow_schedule_id.clone());
            node_metricis.push((id.clone(), load_matic.node_load_metric));
            network_metricis.push((id.clone(), load_matic.link_load_metric));
        }

        return RmsLoadMetric {
            node_load_metric: self.calculate_averge_load_metric(shadow_schedule_id.clone(), node_metricis),
            link_load_metric: self.calculate_averge_load_metric(shadow_schedule_id.clone(), network_metricis),
        };
    }

    /// Computes the **Load Metric** for the entire simulation timeline.
    /// Aggregates metrics from all valid AcIs to provide a high-level view of system utilization.
    ///
    /// # Arguments
    /// * `shadow_schedule_id` - Optional ID for shadow schedule analysis (If None utilize master schedule).
    ///
    /// # Returns
    /// A `LoadMetric` representing the average reserved capacity and utilization across the simulation.
    pub fn get_simulation_load_metric(&mut self, shadow_schedule_id: Option<ShadowScheduleId>) -> RmsLoadMetric {
        let mut node_metricis = Vec::new();
        let mut network_metricis = Vec::new();

        for (id, container) in self.vrm_components.iter_mut() {
            let load_matic = container.vrm_component.get_simulation_load_metric(shadow_schedule_id.clone());
            node_metricis.push((id.clone(), load_matic.node_load_metric));
            network_metricis.push((id.clone(), load_matic.link_load_metric));
        }

        return RmsLoadMetric {
            node_load_metric: self.calculate_averge_load_metric(shadow_schedule_id.clone(), node_metricis),
            link_load_metric: self.calculate_averge_load_metric(shadow_schedule_id.clone(), network_metricis),
        };
    }
}
