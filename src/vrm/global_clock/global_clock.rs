use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GlobalClockDto {
    pub is_simulation: bool,
}

#[derive(Debug)]
pub struct GlobalClock {
    pub is_simulation: bool,
    pub reference_start_time: AtomicI64,
}

impl GlobalClock {
    pub fn new(is_simulation: bool) -> Self {
        let mut reference_start_time = AtomicI64::new(SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards").as_secs() as i64);
        if is_simulation {
            reference_start_time = AtomicI64::new(0);
        }
        Self { is_simulation: is_simulation, reference_start_time: reference_start_time }
    }

    pub fn get_system_time_s(&self) -> i64 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards").as_secs() as i64;

        if self.is_simulation {
            return self.reference_start_time.load(Ordering::Relaxed);
        }

        return now;
    }

    pub fn tick_forward(&mut self) {
        if self.is_simulation {
            self.reference_start_time = AtomicI64::new(self.reference_start_time.load(Ordering::Relaxed) + 1);
        }
    }
}
