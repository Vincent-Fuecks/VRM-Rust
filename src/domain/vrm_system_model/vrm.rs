use std::collections::HashMap;
use std::sync::Arc;

use crate::api::vrm_system_model_dto::vrm_dto::VrmDto;
use crate::api::workflow_dto::client_dto::{ClientDto, ClientsDto};

use crate::domain::vrm_system_model::vrm_component::{
    aci::AcI,
    adc::ADC,
    component_communication::{codec, protocol, session},
    utils::vrm_component_base::{VrmComponentBase, VrmComponentTyp},
    utils::vrm_component_message::VrmComponentMessage,
    vrm_component_trait::VrmComponent,
};

use crate::domain::simulator;
use crate::domain::simulator::simulator::{Simulator, SystemSimulator};
use crate::domain::vrm_system_model::client::client::Clients;
use crate::domain::vrm_system_model::vrm;
use crate::domain::vrm_system_model::{
    client::client::Client,
    utils::id::{AciId, AdcId, ClientId, WorkflowId},
    vrm_system_model::Vrm,
    workflow::workflow::Workflow,
};
use crate::vrm::commons::id::ComponentId;
use crate::vrm::reservation::reservation::Reservation;
use crate::vrm::reservation::reservation_store::{self, ReservationId, ReservationStore};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
pub struct System {
    pub simulator: Arc<dyn SystemSimulator>,
    pub reservation_store: ReservationStore,

    // Contains the Ids of all Clients
    pub client_list: Vec<ClientId>,

    /** list of reservation, which are not yet submitted */
    pub unprocessed_reservations: Vec<ReservationId>,

    /** list of reservations are executed in the moment (commited, but not finished) */
    pub open_reservations: Vec<ReservationId>,

    pub master_id: ComponentId,

    // Contains the representation of the VRM model
    pub vrm: Vrm,
}
impl System {
    pub fn new(
        simulator: Arc<dyn SystemSimulator>,
        reservation_store: ReservationStore,
        client_list: Vec<ClientId>,
        master_id: ComponentId,
        vrm: Vrm,
    ) -> Self {
        System { simulator, reservation_store, client_list, unprocessed_reservations: Vec::new(), open_reservations: Vec::new(), vrm, master_id }
    }

    /// First verifies, that all of the client reservations where added to the system and than adds all reservations
    /// to system scheduling queue
    pub fn add_client(&mut self, client_dto: &ClientDto) -> bool {
        let client_id = ClientId::new(client_dto.id.clone());

        for workflow_dto in &client_dto.workflows {
            let workflow = match Workflow::create_form_dto(workflow_dto.clone(), client_id.clone(), self.reservation_store.clone()) {
                Ok(wf) => wf,
                Err(e) => {
                    log::error!("WorkflowDtoConversionError: Failed to create workflow form DTO of client {}. With error {}", client_id, e);
                    return false;
                }
            };

            let res_ids_of_workflow = workflow.get_all_reservation_ids();

            if self.reservation_store.contains_reservations(res_ids_of_workflow) {
                let workflow_res_id = self.reservation_store.add(Reservation::Workflow(workflow));
                self.unprocessed_reservations.push(workflow_res_id);
            } else {
                log::error!("ErrorReservationOfWorkflowIsNotInStore: An reservation Client {} is not present in the reservation Store.", client_id);
                return false;
            }
        }
        return true;
    }

    // pub fn aggregate_vrm_components(&self, vrm: Vrm) -> HashMap<ComponentId, Box<dyn VrmComponent>> {
    //     let mut vrm_component_map = HashMap::new();

    //     let mut vrm_component_map: HashMap<ComponentId, Box<dyn VrmComponent>> = HashMap::new();

    //     for (adc_id, adc) in vrm.adcs {
    //         let component_id: ComponentId = adc_id.cast();
    //         vrm_component_map.insert(component_id, Box::new(adc));
    //     }

    //     for (aci_id, aci) in vrm.acis {
    //         let component_id: ComponentId = aci_id.cast();
    //         vrm_component_map.insert(component_id, Box::new(aci));
    //     }

    //     return vrm_component_map;
    // }

    pub fn init_vrm(&mut self, clients_dto: ClientsDto, vrm: Vrm) -> bool {
        for client_dto in clients_dto.clients.iter() {
            let client_id = ClientId::new(client_dto.id.clone());
            self.client_list.push(client_id.clone());

            if !self.add_client(client_dto) {
                log::info!(
                    "Added all reservations of the client: {} to the vrm scheduler queue. The reservations will be distributed to the VrmComponents in the next step.",
                    client_id
                );
            }
        }

        self.master_id = vrm.adc_master.clone().cast();

        // Setup ADCs
        for (adc_id, adc) in vrm.adcs {}

        todo!()
    }

    pub fn run_vrm(&mut self) {
        todo!()
    }
}

impl System {
    fn from_clients_dto(
        dto: ClientsDto,
        simulator: Arc<dyn SystemSimulator>,
        reservation_store: ReservationStore,
    ) -> Result<HashMap<ClientId, Client>> {
        let clients = HashMap::new();

        for client_dto in dto.clients {
            let mut workflows = HashMap::new();
            let client_id = ClientId::new(client_dto.id);

            for workflow_dto in client_dto.workflows {
                let workflow_id = WorkflowId::new(workflow_dto.id.clone());
                let workflow = Workflow::create_form_dto(workflow_dto, client_id.clone(), reservation_store.clone())?;
                workflows.insert(workflow_id, workflow);
            }

            let adc_id;
            if let Some(id) = client_dto.adc_id {
                adc_id = Some(AdcId::new(id));
            } else {
                adc_id = None;
            }

            let open_reservations = todo!();
            let unprocessed_reservations = todo!();

            let client = Client { id: client_id, simulator, adc_id, open_reservations, unprocessed_reservations };

            clients.insert(client_id, client);
        }

        Ok(clients)
    }

    fn from_vrm_dto(dto: VrmDto, simulator: Arc<dyn SystemSimulator>) -> Result<(HashMap<AdcId, ADC>, HashMap<AciId, AcI>)> {
        let mut adcs: HashMap<AdcId, ADC> = HashMap::new();
        let mut acis: HashMap<AciId, AcI> = HashMap::new();

        for adc_dto in dto.adc {
            let adc_id = AdcId::new(adc_dto.id.clone());
            let adc = ADC::new(adc_dto.id);
            adcs.insert(adc_id, adc);
        }

        for aci_dto in dto.aci {
            let aci_id = AciId::new(aci_dto.id.clone());
            let aci = AcI::new(aci_dto.id);
            acis.insert(aci_id, aci);
        }

        Ok((adcs, acis))
    }

    // pub async fn run_all_clients(&mut self, reservation_store: ReservationStore, simulator: Arc<dyn SystemSimulator>) {
    //     let clients: Vec<(ClientId, Client)> = self.clients.drain().collect();
    //     let mut handles = vec![];
    //     let mut adc: dyn ExtendedReservationProcessor = self.vrm.adcs.get_mut(&self.vrm.adc_master).expect("Did not find master ADC.");

    //     for (id, client) in clients {
    //         println!("Starting client: {}", id);

    //         let handle = tokio::spawn(async move {
    //             client.run(reservation_store, &mut adc).await;
    //         });

    //         handles.push(handle);
    //     }

    //     for handle in handles {
    //         let _ = handle.await;
    //     }

    //     println!("All clients have completed their reservations.");
    // }
}
