use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;

/// The target string to filter for analytics events.
pub const ANALYTICS_TARGET: &str = "analytics";

pub struct AnalyticsSystem;

impl AnalyticsSystem {
    pub fn init(log_file_path: String) {
        let mut file = File::create(log_file_path.clone()).expect("Failed to create log file");

        let header_line = StatParameter::headers().join(";") + "\n";
        file.write_all(header_line.as_bytes()).expect("Failed to write headers");
        let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file);

        _ = tracing_subscriber::registry().with(tracing_subscriber::fmt::layer()).with(AnalyticsLayer::new(non_blocking_writer)).try_init();

        tracing::info!("Simulation started. Analytics writing to {}", log_file_path);
    }
}

/// Each event consists of a set of key-value-pairs with the measured data or some meta data of the event.
/// This enum specifies all allowed key values and thus the column in the output file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum StatParameter {
    /// Time in seconds since simulation start.
    Time,

    /// Description why this entry was made
    LogDescription,

    // Component
    /// Type of the component is "CLIENT", "ADC", "AI" or "SIMULATOR"
    ComponentType,

    /// Name of component
    ComponentName,

    /// Overall or estimated capacity of the component
    NodeComponentCapacity,

    /// Load of component
    NodeComponentUtilization,

    /// Overall or estimated capacity of the component
    NetworkComponentCapacity,

    /// Load of component
    NetworkComponentUtilization,

    /// Fragmentation of component
    ComponentFragmentation,

    // Reservation
    /// Name of the reservation
    ReservationName,

    /// Size of Reservation (capacity)
    ReservationCapacity,

    /// Overall size of Reservation (capacity * duration)
    ReservationWorkload,

    /// ReservationState of Reservation
    ReservationState,

    /// ReservationProceeding of Reservation
    ReservationProceeding,

    /// Number of Tasks in Reservation. Is 1 if Reservation was not a Workflow
    NumberOfTasks,

    // Operation
    /// Command send or answered
    Command,

    /// Time to process command in ms
    ProcessingTime,

    /// System fragmentation before reservation  
    FragmentationBefore,

    /// System fragmentation after reservation
    FragmentationAfter,

    /// Number of CoAllocation Dependencies (if the reservation is a Workflow)
    NumberOfCoAllocationDependencies,

    /// Number of DataDependencies (if the reservation is a Workflow)
    NumberOfDataDependencies,
}

impl StatParameter {
    /// Returns the CSV headers in the exact order required.
    pub fn headers() -> &'static [&'static str] {
        &[
            "Time",
            "LogDescription",
            "ComponentType",
            "ComponentName",
            "ComponentCapacity",
            "ComponentUtilization",
            "ComponentFragmentation",
            "ReservationName",
            "ReservationCapacity",
            "ReservationWorkload",
            "ReservationState",
            "ReservationProceeding",
            "NumberOfTasks",
            "Command",
            "ProcessingTime",
            "FragmentationBefore",
            "FragmentationAfter",
            "NumberOfCoAllocationDependencies",
            "NumberOfDataDependencies",
        ]
    }

    fn from_str(s: &str) -> Option<StatParameter> {
        match s {
            "Time" => Some(Self::Time),
            "LogDescription" => Some(Self::LogDescription),
            "ComponentType" => Some(Self::ComponentType),
            "ComponentName" => Some(Self::ComponentName),
            "NodeComponentCapacity" => Some(Self::NodeComponentCapacity),
            "NodeComponentUtilization" => Some(Self::NodeComponentUtilization),
            "NetworkComponentCapacity" => Some(Self::NetworkComponentCapacity),
            "NetworkComponentUtilization" => Some(Self::NetworkComponentUtilization),
            "ComponentFragmentation" => Some(Self::ComponentFragmentation),
            "ReservationName" => Some(Self::ReservationName),
            "ReservationCapacity" => Some(Self::ReservationCapacity),
            "ReservationWorkload" => Some(Self::ReservationWorkload),
            "ReservationState" => Some(Self::ReservationState),
            "ReservationProceeding" => Some(Self::ReservationProceeding),
            "NumberOfTasks" => Some(Self::NumberOfTasks),
            "Command" => Some(Self::Command),
            "ProcessingTime" => Some(Self::ProcessingTime),
            "FragmentationBefore" => Some(Self::FragmentationBefore),
            "FragmentationAfter" => Some(Self::FragmentationAfter),
            "NumberOfCoAllocationDependencies" => Some(Self::NumberOfCoAllocationDependencies),
            "NumberOfDataDependencies" => Some(Self::NumberOfDataDependencies),
            _ => None,
        }
    }
}

/// Extracts values from a Tracing Event and maps them to StatParameters.
struct AnalyticsVisitor {
    values: HashMap<StatParameter, String>,
}

impl AnalyticsVisitor {
    fn new() -> Self {
        Self { values: HashMap::new() }
    }
}

impl Visit for AnalyticsVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if let Some(param) = StatParameter::from_str(field.name()) {
            self.values.insert(param, format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if let Some(param) = StatParameter::from_str(field.name()) {
            self.values.insert(param, value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if let Some(param) = StatParameter::from_str(field.name()) {
            self.values.insert(param, value.to_string());
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if let Some(param) = StatParameter::from_str(field.name()) {
            self.values.insert(param, value.to_string());
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if let Some(param) = StatParameter::from_str(field.name()) {
            self.values.insert(param, value.to_string());
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        if let Some(param) = StatParameter::from_str(field.name()) {
            self.values.insert(param, value.to_string());
        }
    }
}

/// A custom Tracing Layer that intercepts analytics events and writes them to a CSV writer.
pub struct AnalyticsLayer<W: Write + 'static> {
    writer: Mutex<W>,
}

impl<W: Write + 'static> AnalyticsLayer<W> {
    pub fn new(writer: W) -> Self {
        Self { writer: Mutex::new(writer) }
    }

    fn write_csv_row(&self, visitor: &AnalyticsVisitor) {
        let headers = StatParameter::headers();
        let mut row = String::with_capacity(256);

        for (i, &header_name) in headers.iter().enumerate() {
            if let Some(param) = StatParameter::from_str(header_name) {
                let default_val = "NA".to_string(); // Default for missing columns
                let val = visitor.values.get(&param).unwrap_or(&default_val);

                row.push_str(val);
            } else {
                row.push_str("ERROR");
            }

            if i < headers.len() - 1 {
                row.push(';');
            }
        }
        row.push('\n');

        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(row.as_bytes());
        }
    }
}

impl<S, W> Layer<S> for AnalyticsLayer<W>
where
    S: Subscriber,
    W: Write + 'static,
{
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        metadata.target() == ANALYTICS_TARGET
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = AnalyticsVisitor::new();
        event.record(&mut visitor);

        if !visitor.values.contains_key(&StatParameter::Time) {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            visitor.values.insert(StatParameter::Time, now.to_string());
        }

        self.write_csv_row(&visitor);
    }
}
