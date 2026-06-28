use crate::error::Result;
use crate::loader::parser::parse_json_file;
use crate::vrm::reservation::reservation_store::ReservationStore;

use self::schema::client_dto::ClientsDto;
use self::vrm::client::client::Clients;
use self::vrm::commons::logging::logger;

pub mod error;
pub mod loader;
pub mod schema;
pub mod vrm;

pub fn generate_system_model(file_path: &str, reservation_store: ReservationStore) -> Result<Clients> {
    logger::init();
    log::info!("Logger initialized. Starting SystemModel construction.");

    let root_dto: ClientsDto = parse_json_file::<ClientsDto>(file_path)?;
    log::info!("JSON file parsed successfully.");

    let system_model = Clients::from_dto(root_dto, reservation_store)?;
    log::info!("Internal SystemModel constructed successfully.");

    Ok(system_model)
}
