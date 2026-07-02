use crate::{
    vrm::reservation::reservation_store::ReservationId,
    vrm::schedule::{
        schedule_trait::Schedule,
        slotted_schedule::{slotted_schedule_context::SlottedScheduleContext, strategy::strategy_trait::SlottedScheduleStrategy},
    },
};

impl<S: SlottedScheduleStrategy + Clone + 'static> SlottedScheduleContext<S> {
    /// Computes the **Fragmentation Index** of the schedule over a specific time range using
    /// the **Quadratic Mean** method.
    ///
    /// Fragmentation measures the continuity and size distribution of free resource blocks across the schedule.
    /// A value closer to **0.0** indicates less fragmentation and better availability for large, long-duration tasks.
    /// The formula used approximates the **average block size length squared divided by the sum of block sizes**.
    ///
    /// # Returns
    /// A `f64` representing the calculated fragmentation index, where **0.0** is best (least fragmented)
    /// and **1.0** is worst (most fragmented).
    pub fn get_fragmentation_quadratic_mean(&self, start_slot_index: i64, end_slot_index: i64) -> f64 {
        let mut quad_sum_per_free_block: Vec<f64> = vec![0.0; (S::get_capacity(self) + 1) as usize];
        let mut sum_per_free_block: Vec<f64> = vec![0.0; (S::get_capacity(self) + 1) as usize];
        let mut current_free_block_len: Vec<i64> = vec![0; (S::get_capacity(self) + 1) as usize];

        // Add all free slots which end in the investigated time range.
        self.add_block_which_end_in_range(
            start_slot_index,
            end_slot_index,
            &mut quad_sum_per_free_block,
            &mut sum_per_free_block,
            &mut current_free_block_len,
        );

        // Add free blocks which are cut by the end of the investigated time range
        self.add_block_which_are_cut_by_range_end(&mut quad_sum_per_free_block, &mut sum_per_free_block, &mut current_free_block_len);

        self.calculate_avg_fragmentation(&quad_sum_per_free_block, &sum_per_free_block)
    }

    fn add_block_which_end_in_range(
        &self,
        start_slot_index: i64,
        end_slot_index: i64,
        quad_sum_per_free_block: &mut Vec<f64>,
        sum_per_free_block: &mut Vec<f64>,
        current_free_block_len: &mut Vec<i64>,
    ) {
        for slot_index in start_slot_index..=end_slot_index {
            let free_capacity = S::get_capacity(self) - self.get_slot_load(slot_index);

            for capacity in 1..=free_capacity {
                current_free_block_len[capacity as usize] += 1;
            }

            for capacity in free_capacity + 1..=S::get_capacity(self) {
                if current_free_block_len[capacity as usize] > 0 {
                    quad_sum_per_free_block[capacity as usize] +=
                        f64::powf(current_free_block_len[capacity as usize] as f64, S::get_fragmentation_power());

                    sum_per_free_block[capacity as usize] += current_free_block_len[capacity as usize] as f64;
                    current_free_block_len[capacity as usize] = 0;
                }
            }
        }
    }

    fn add_block_which_are_cut_by_range_end(
        &self,
        quad_sum_per_free_block: &mut Vec<f64>,
        sum_per_free_block: &mut Vec<f64>,
        current_free_block_len: &mut Vec<i64>,
    ) {
        for capacity in 1..=S::get_capacity(self) {
            if current_free_block_len[capacity as usize] > 0 {
                quad_sum_per_free_block[capacity as usize] +=
                    f64::powf(current_free_block_len[capacity as usize] as f64, S::get_fragmentation_power());
                sum_per_free_block[capacity as usize] += current_free_block_len[capacity as usize] as f64;
                current_free_block_len[capacity as usize] = 0;
            }
        }
    }

    fn calculate_avg_fragmentation(&self, quad_sum_per_free_block: &Vec<f64>, sum_per_free_block: &Vec<f64>) -> f64 {
        let mut block_fragmentation: Vec<f64> = Vec::new();

        for capacity in 1..=S::get_capacity(self) {
            if sum_per_free_block[capacity as usize] > 0.0 {
                let frag: f64 = quad_sum_per_free_block[capacity as usize] / sum_per_free_block[capacity as usize].powf(S::get_fragmentation_power());

                block_fragmentation.push(frag);
            }
        }

        // No free block
        if block_fragmentation.is_empty() {
            return 0.0;
        }

        1.0 - block_fragmentation.iter().sum::<f64>() / (block_fragmentation.len() as f64)
    }

    /// Calculates the **Resubmission Fragmentation Index (RFI)** for a specific time window.
    ///
    /// The RFI is a specialized metric that estimates the quality of scheduling based on how well existing active
    /// reservations could be **re-scheduled (resubmitted)** if they were released and immediately re-requested.
    /// This is a **simulated, pessimistic fragmentation test**.
    ///
    /// A high RFI (closer to **1.0**) indicates severe fragmentation, suggesting that even if capacity exists
    /// (`free_capacity_in_range > 0`), it is scattered into blocks too small to accommodate the existing reservations
    /// when they are randomly re-tested. A low RFI (closer to **0.0**)
    /// indicates good ability to schedule the reservations.
    ///
    /// # Warning
    ///
    /// This is an **expensive, simulation-based metric** that involves cloning the schedule
    /// and iterative reservation attempts.
    ///
    /// # Returns
    ///
    /// A `f64` representing the RFI, typically between **0.0** (good) and **1.0** (bad).
    /// Returns **0.0** if the range is completely empty.
    pub fn get_fragmentation_resubmit(&self, start_slot_index: i64, end_slot_index: i64) -> f64 {
        log::warn!("In SlottedSchedule id: {}, fragmentation resubmit is requested.", self.id);

        let mut free_capacity_in_range: i64 = 0;
        let mut range_in_use: bool = false;

        for slot_index in start_slot_index..=end_slot_index {
            let next_slot_load: i64 = self.get_slot_load(slot_index);
            free_capacity_in_range += S::get_capacity(self) - next_slot_load;

            if next_slot_load > 0 {
                range_in_use = true;
            }
        }

        if !range_in_use {
            return 0.0;
        }

        let mut remaining_capacity: i64 = free_capacity_in_range * self.slot_width;
        let mut rejected_capacity: i64 = 0;

        let mut test_schedule = self.clone();

        while remaining_capacity > 0 {
            if self.active_reservations.is_empty() {
                log::error!("Simulation of single resubmission failed, because active reservations are empty while remaining_capacity > 0.");
                return 0.0;
            }

            // This loop ensures we select a reservation that AT LEAST PARTLY overlaps the range.
            let random_reservation_id: ReservationId = loop {
                let id = self.active_reservations.get_random_id().expect("No random ReservationId was found in test SlottedSchedule.");

                let is_non_overlapping = self.reservation_store.get_assigned_start(id) > self.get_slot_end_time(end_slot_index)
                    || self.reservation_store.get_assigned_end(id) < self.get_slot_start_time(start_slot_index);

                if !is_non_overlapping {
                    break id;
                }
            };

            match test_schedule.reserve(random_reservation_id) {
                // Could not book again
                Some(id) => {
                    remaining_capacity -= self.reservation_store.get_reserved_capacity(id);
                    rejected_capacity +=
                        self.reservation_store.get_reserved_capacity(id) * self.reservation_store.get_task_duration(id);
                }
                // Success
                None => {
                    remaining_capacity -= self.reservation_store.get_reserved_capacity(random_reservation_id);
                }
            }
        }
        (rejected_capacity as f64) / ((free_capacity_in_range * self.slot_width) as f64)
    }
}
