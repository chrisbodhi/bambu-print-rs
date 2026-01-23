//! Bambu Lab Printer Emulator
//!
//! A Rust server that emulates a Bambu Lab X1 Carbon 3D printer with 4-slot AMS,
//! exposing MQTT and FTP interfaces compatible with the `bambulabs_api` Python library.

pub mod config;
pub mod mqtt;
pub mod state;

pub use config::EmulatorConfig;
pub use state::{GcodeState, PrinterState};

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Main emulator instance
pub struct Emulator {
    config: EmulatorConfig,
    state: Arc<RwLock<PrinterState>>,
}

impl Emulator {
    /// Create a new emulator with the given configuration
    pub fn new(config: EmulatorConfig) -> Self {
        let state = Arc::new(RwLock::new(PrinterState::new(config.serial_number.clone())));

        Self { config, state }
    }

    /// Start the emulator (MQTT broker and status publishing loop)
    pub async fn run(self) -> Result<()> {
        info!("Starting Bambu Lab emulator");
        info!("Serial number: {}", self.config.serial_number);
        info!("MQTT port: {}", self.config.mqtt_port);

        // Start MQTT broker
        let broker_handle = mqtt::broker::start_broker(self.config.mqtt_port).await?;

        // Start status publishing loop
        let state_clone = Arc::clone(&self.state);
        let serial = self.config.serial_number.clone();
        let status_interval = self.config.status_interval_ms;
        let mqtt_port = self.config.mqtt_port;

        let publish_handle = tokio::spawn(async move {
            mqtt::publisher::run_status_publisher(state_clone, serial, status_interval, mqtt_port)
                .await
        });

        // Start MQTT command handler
        let state_clone = Arc::clone(&self.state);
        let serial = self.config.serial_number.clone();
        let mqtt_port = self.config.mqtt_port;

        let handler_handle = tokio::spawn(async move {
            mqtt::handler::run_command_handler(state_clone, serial, mqtt_port).await
        });

        // Wait for tasks
        tokio::select! {
            _ = broker_handle => info!("MQTT broker stopped"),
            _ = publish_handle => info!("Status publisher stopped"),
            _ = handler_handle => info!("Command handler stopped"),
        }

        Ok(())
    }
}
