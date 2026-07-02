use chrono::Local;
use fern::Dispatch;
use log::LevelFilter;
use std::fs;

// Define where to store logs
const LOG_DIR: &str = "logs";
const LOG_FILE: &str = "system.log";

/// Initializes the global logger.
///
/// This function should be called once at the very beginning of the
/// application's `main` function.
///
/// Log level is controlled by the `RUST_LOG` environment variable.
/// Example: `RUST_LOG=info cargo run`
///
/// If `RUST_LOG` is not set, it defaults to `info`.
/// Logs will be written to `logs/workflow_loader.log` and the console.
pub fn init() {
    if let Err(e) = fs::create_dir_all(LOG_DIR) {
        eprintln!("Failed to create log directory at '{}': {}", LOG_DIR, e);
    }

    let log_file_path = format!("{}/{}", LOG_DIR, LOG_FILE);

    // Get the log level from RUST_LOG, defaulting to "info" (RUST_LOG=debug or RUST_LOG=warn)
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_string());

    let log_level_filter = log_level.parse::<LevelFilter>().unwrap_or(LevelFilter::Debug);

    let base_config = Dispatch::new().level(log_level_filter).level_for("serde", LevelFilter::Warn).level_for("uuid", LevelFilter::Warn);

    let console_config = Dispatch::new()
        .format(|out, message, record| {
            // Use fern's colored formatting
            let colors = fern::colors::ColoredLevelConfig::new()
                .error(fern::colors::Color::Red)
                .warn(fern::colors::Color::Yellow)
                .info(fern::colors::Color::Green)
                .debug(fern::colors::Color::Blue)
                .trace(fern::colors::Color::BrightBlack);

            out.finish(format_args!(
                "[{} {} {}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .chain(std::io::stderr());

    let file_config = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!("[{} {} {}] {}", Local::now().format("%Y-%m-%d %H:%M:%S"), record.level(), record.target(), message))
        })
        .chain(fern::log_file(&log_file_path).unwrap_or_else(|e| {
            eprintln!("Failed to open log file '{}': {}", log_file_path, e);
            fern::log_file("/dev/stderr").expect("Failed to open stderr as fallback")
        }));

    base_config
        .chain(console_config) // Log to console
        .chain(file_config) // Log to file
        .apply()
        .unwrap_or_else(|e| {
            eprintln!("Failed to apply logger configuration: {}", e);
        });

    log::info!("Logger initialized. Logging to console and '{}'.", log_file_path);
}
