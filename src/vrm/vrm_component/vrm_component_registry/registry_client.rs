use std::collections::HashMap;
use std::sync::{Arc, RwLock, mpsc};
use std::thread;

use crate::vrm::vrm_component::vrm_component_registry::vrm_component_proxy::VrmComponentProxy;
use crate::vrm::vrm_component::vrm_component_registry::vrm_message::VrmMessage;
use crate::vrm::vrm_component::vrm_component_trait::VrmComponent;
use crate::vrm::commons::id::ComponentId;

/// The RegistryClient maps ComponentId -> Sender
#[derive(Clone, Debug)]
pub struct RegistryClient {
    directory: Arc<RwLock<HashMap<ComponentId, mpsc::Sender<VrmMessage>>>>,
}

impl RegistryClient {
    pub fn new() -> Self {
        Self { directory: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub fn spawn_component(&self, component: Box<dyn VrmComponent + Send + 'static>) -> VrmComponentProxy {
        let id = component.get_id();
        let (tx, rx) = mpsc::channel::<VrmMessage>();

        // 1. Register in directory
        {
            let mut map = self.directory.write().unwrap();
            map.insert(id.clone(), tx.clone());
        }

        // 2. Spawn the "Actor" thread
        let component_id_clone = id.clone();
        thread::Builder::new()
            .name(format!("Actor-{}", id))
            .spawn(move || {
                log::info!("Component Actor {} started.", component_id_clone);
                Self::run_actor_loop(component, rx);
            })
            .expect("Failed to spawn component thread");

        VrmComponentProxy { id, tx }
    }

    fn run_actor_loop(mut component: Box<dyn VrmComponent + Send + 'static>, rx: mpsc::Receiver<VrmMessage>) {
        while let Ok(msg) = rx.recv() {
            match msg {
                VrmMessage::GetId(reply) => {
                    let _ = reply.send(component.get_id());
                }
                VrmMessage::GetTotalCapacity(reply) => {
                    let _ = reply.send(component.get_total_capacity());
                }
                VrmMessage::GetTotalLinkCapacity(reply) => {
                    let _ = reply.send(component.get_total_link_capacity());
                }
                VrmMessage::GetTotalNodeCapacity(reply) => {
                    let _ = reply.send(component.get_total_node_capacity());
                }
                VrmMessage::GetLinkResourceCount(reply) => {
                    let _ = reply.send(component.get_link_resource_count());
                }
                VrmMessage::CanHandel { reservation, reply_to } => {
                    let _ = reply_to.send(component.can_handel(reservation));
                }
                VrmMessage::Probe { reservation_id, shadow_schedule_id, reply_to } => {
                    let _ = reply_to.send(component.probe(reservation_id, shadow_schedule_id));
                }
                VrmMessage::ProbeBest { reservation_id, shadow_schedule_id, probe_reservation_comparator, reply_to } => {
                    let _ = reply_to.send(component.probe_best(reservation_id, shadow_schedule_id, probe_reservation_comparator));
                }
                VrmMessage::Reserve { reservation_id, shadow_schedule_id, reply_to } => {
                    let _ = reply_to.send(component.reserve(reservation_id, shadow_schedule_id));
                }
                VrmMessage::Commit { reservation_id, reply_to } => {
                    let _ = reply_to.send(component.commit(reservation_id));
                }
                VrmMessage::DeleteTask { reservation_id, shadow_schedule_id, reply_to } => {
                    let _ = reply_to.send(component.delete(reservation_id, shadow_schedule_id));
                }
                VrmMessage::GetSatisfaction { start, end, shadow_schedule_id, reply_to } => {
                    let _ = reply_to.send(component.get_satisfaction(start, end, shadow_schedule_id));
                }
                VrmMessage::GetSystemSatisfaction { shadow_schedule_id, reply_to } => {
                    let _ = reply_to.send(component.get_system_satisfaction(shadow_schedule_id));
                }
                VrmMessage::CreateShadowSchedule { id, reply_to } => {
                    let _ = reply_to.send(component.create_shadow_schedule(id));
                }
                VrmMessage::DeleteShadowSchedule { id, reply_to } => {
                    let _ = reply_to.send(component.delete_shadow_schedule(id));
                }
                VrmMessage::CommitShadowSchedule { id, reply_to } => {
                    let _ = reply_to.send(component.commit_shadow_schedule(id));
                }
                VrmMessage::GetLoadMetricUpToDate { start, end, shadow_schedule_id, reply_to } => {
                    let _ = reply_to.send(component.get_load_metric_up_to_date(start, end, shadow_schedule_id));
                }
                VrmMessage::GetLoadMetric { start, end, shadow_schedule_id, reply_to } => {
                    let _ = reply_to.send(component.get_load_metric(start, end, shadow_schedule_id));
                }
                VrmMessage::GetSimulationLoadMetric { shadow_schedule_id, reply_to } => {
                    let _ = reply_to.send(component.get_simulation_load_metric(shadow_schedule_id));
                }
                VrmMessage::Shutdown => break,
            }
        }
    }
}