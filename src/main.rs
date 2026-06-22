use crate::domain::simulator::simulator::GlobalClock;
use crate::domain::vrm_system_model::reservation::vrm_state_listener::VrmStateListener;
use crate::domain::vrm_system_model::utils::statistics::AnalyticsSystem;
use crate::domain::vrm_system_model::vrm_manager::VrmManager;

use crate::domain::vrm_system_model::client::client::Clients;

use crate::domain::vrm_system_model::grid_resource_management_system::vrm_component_registry::registry_client::RegistryClient;
use crate::domain::vrm_system_model::reservation::reservation_store::ReservationStore;

#[cfg(debug_assertions)]
use crate::api::vrm_system_model_dto::vrm_dto::VrmDto;
use crate::error::Result;
use crate::loader::parser::parse_json_file;
use clap::Parser;
use parking_lot::{RwLock, deadlock};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub mod api;
pub mod domain;
pub mod error;
pub mod loader;
pub mod logger;

pub fn get_vrm_dto(file_path: &str) -> Result<VrmDto> {
    log::info!("Starting VrmDto construction.");

    let root_dto: VrmDto = parse_json_file::<VrmDto>(file_path)?;
    log::info!("JSON file parsed successfully.");
    Ok(root_dto)
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the workflow input file (.json)
    #[arg(short = 'f', long, default_value = "src/data/demo/workflow_with_direct_mapping.json")]
    input_file: String,

    /// Path to the output results/statistics file (.csv)
    #[arg(short = 'o', long, default_value = "results.csv")]
    output_file: String,

    /// Path to the VRM node simulator config
    #[arg(short = 'c', long, default_value = "src/data/vrm_rms_simulator.json")]
    config_file: String,

    /// Disables Logging
    #[arg(short = 'l', long)]
    disable_logging: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Init Logging
    if args.disable_logging {
        log::set_max_level(log::LevelFilter::Off);
    } else {
        logger::init();
        // log::set_max_level(log::LevelFilter::Info);
        AnalyticsSystem::init(args.output_file);
    }

    start_deadlock_detector();

    let file_path_workflows = &args.input_file;
    let file_path_vrm = &args.config_file;

    let reservation_store = ReservationStore::new();
    reservation_store.add_listener(Arc::new(RwLock::new(VrmStateListener::new_empty())));

    let vrm_dto = get_vrm_dto(file_path_vrm).expect("Failed to load VRM DTO");
    let is_simulation = vrm_dto.simulator.is_simulation;
    let unprocessed_reservations =
        Clients::get_clients(file_path_workflows, reservation_store.clone()).expect("Failed to load clients").unprocessed_reservations;

    let registry = RegistryClient::new();
    let simulator = Arc::new(GlobalClock::new(is_simulation));

    let mut vrm_manager = VrmManager::init_vrm_system(vrm_dto, unprocessed_reservations, simulator, registry, reservation_store.clone())
        .await
        .expect("Failed to initialize VRM system");

    vrm_manager.run_vrm().await;
}

pub fn start_deadlock_detector() {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(10));
            let deadlocks = deadlock::check_deadlock();
            if !deadlocks.is_empty() {
                for (i, threads) in deadlocks.iter().enumerate() {
                    eprintln!("Deadlock #{} detected with {} threads:", i, threads.len());
                    for t in threads {
                        eprintln!("Thread ID: {:?}", t.thread_id());
                    }
                }
            }
        }
    });
}
