use crate::domain::vrm_system_model::client::client::Clients;
use crate::domain::vrm_system_model::reservation::reservation_store::ReservationStore;
use crate::error::Result;
use crate::loader::parser::parse_json_file;

use self::schema::client_dto::ClientsDto;

pub mod domain;
pub mod error;
pub mod loader;
pub mod logger;
pub mod schema;

pub fn generate_system_model(file_path: &str, reservation_store: ReservationStore) -> Result<Clients> {
    logger::init();
    log::info!("Logger initialized. Starting SystemModel construction.");

    let root_dto: ClientsDto = parse_json_file::<ClientsDto>(file_path)?;
    log::info!("JSON file parsed successfully.");

    let system_model = Clients::from_dto(root_dto, reservation_store)?;
    log::info!("Internal SystemModel constructed successfully.");

    Ok(system_model)
}
