use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex, RwLock},
};

use crate::vrm::commons::id::ComponentId;

use super::{reservation::ReservationState, reservation_store::ReservationId};

/// The result returned to the ADC after waiting.
#[derive(Clone, Debug)]
pub struct ReservationResult {
    pub state: ReservationState,
    pub aci_id: Option<ComponentId>,
}

/// Internal state for the sync gate.
#[derive(Clone, Debug)]
struct GateState {
    state: ReservationState,
    aci_id: Option<ComponentId>,
}

/// A simple synchronization helper to allow one thread to wait for a
/// specific state change on a reservation.
#[derive(Clone, Debug)]
pub struct ReservationSyncGate {
    pair: Arc<(Mutex<GateState>, Condvar)>,
}

impl ReservationSyncGate {
    pub fn new(initial_state: ReservationState) -> Self {
        let initial_gate_state = GateState { state: initial_state, aci_id: None };
        Self { pair: Arc::new((Mutex::new(initial_gate_state), Condvar::new())) }
    }

    /// Called by the AcI to signal that the state has changed.
    pub fn notify(&self, new_state: ReservationState, aci_id: ComponentId) {
        let (lock, cvar) = &*self.pair;
        let mut gate_state = lock.lock().unwrap();
        gate_state.state = new_state;
        gate_state.aci_id = Some(aci_id);
        cvar.notify_all();
    }

    pub fn wait_with_timeout(&self, timeout: std::time::Duration) -> ReservationResult {
        let (lock, cvar) = &*self.pair;
        let mut gate_state = lock.lock().unwrap();

        // Wait as long as we are in the "transition" state
        while gate_state.state == ReservationState::ReserveProbeReservation {
            let result = cvar.wait_timeout(gate_state, timeout).unwrap();
            if result.1.timed_out() {
                return ReservationResult { state: ReservationState::Rejected, aci_id: None };
            }
            gate_state = result.0;
        }

        ReservationResult { state: gate_state.state, aci_id: gate_state.aci_id.clone() }
    }
}

#[derive(Clone, Debug)]
pub struct SyncRegistry {
    gates: Arc<RwLock<HashMap<ReservationId, Arc<ReservationSyncGate>>>>,
}

impl SyncRegistry {
    pub fn new() -> Self {
        Self { gates: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub fn create_gate(&self, id: ReservationId) -> Arc<ReservationSyncGate> {
        let gate = Arc::new(ReservationSyncGate::new(ReservationState::ReserveProbeReservation));
        self.gates.write().unwrap().insert(id, gate.clone());
        gate
    }

    pub fn get_gate(&self, id: ReservationId) -> Option<Arc<ReservationSyncGate>> {
        self.gates.read().unwrap().get(&id).cloned()
    }

    pub fn remove_gate(&self, id: ReservationId) {
        self.gates.write().unwrap().remove(&id);
    }
}
