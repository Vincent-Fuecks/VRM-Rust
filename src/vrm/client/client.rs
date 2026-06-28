use crate::vrm::common::id::ClientId;
use crate::error::Result;
use crate::loader::parser::parse_json_file;
use crate::schema::client_dto::ClientsDto;
use crate::vrm::reservation::reservation_store::{ReservationId, ReservationStore};
use crate::vrm::workflow::workflow::Workflow;

#[derive(Debug)]
pub struct Clients {
    pub unprocessed_reservations: Vec<ReservationId>,
}

impl Clients {
    pub fn from_dto(dto: ClientsDto, reservation_store: ReservationStore) -> Result<Self> {
        let mut unprocessed = Vec::new();

        for client_dto in dto.clients {
            let client_id = ClientId::new(client_dto.id);

            for workflow_dto in client_dto.workflows {
                let workflow_res_id = Workflow::create_form_dto(workflow_dto, client_id.clone(), reservation_store.clone())?;
                unprocessed.push(workflow_res_id);
            }
        }

        Ok(Clients { unprocessed_reservations: unprocessed })
    }

    pub fn get_clients(file_path: &str, reservation_store: ReservationStore) -> Result<Clients> {
        log::info!("Starting ClientsDto construction.");

        let root_dto: ClientsDto = parse_json_file::<ClientsDto>(file_path)?;
        log::info!("JSON file parsed successfully.");

        let system_model = Clients::from_dto(root_dto, reservation_store)?;
        log::info!("Internal SystemModel was constructed successfully.");

        Ok(system_model)
    }
}
