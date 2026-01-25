//! Bambu Lab Printer Emulator - CLI entry point

use anyhow::Result;
use bambu_print_rs::{Emulator, EmulatorConfig};
use clap::Parser;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Bambu Lab 3D Printer Emulator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Printer serial number
    #[arg(short, long, default_value = "00M00A000000001")]
    serial: String,

    /// MQTT port
    #[arg(short = 'p', long, default_value = "1883")]
    mqtt_port: u16,

    /// Enable TLS on MQTT (uses port 8883)
    #[arg(long)]
    mqtt_tls: bool,

    /// Access code for authentication (omit for no auth)
    #[arg(short, long)]
    access_code: Option<String>,

    /// Status update interval in milliseconds
    #[arg(long, default_value = "1000")]
    status_interval: u64,

    /// Simulation time multiplier (e.g., 100 = 100x speed)
    #[arg(short, long, default_value = "100")]
    time_multiplier: f32,

    /// Increase logging verbosity
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("bambu_print_rs={},rumqttd=info", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Bambu Lab Printer Emulator v{}", env!("CARGO_PKG_VERSION"));
    info!("Starting emulator with serial: {}", args.serial);

    // Build configuration
    let mut config = EmulatorConfig::new()
        .with_serial(args.serial)
        .with_mqtt_port(args.mqtt_port)
        .with_status_interval(args.status_interval)
        .with_time_multiplier(args.time_multiplier);

    if args.mqtt_tls {
        config = config.with_tls(true);
    }

    if let Some(code) = args.access_code {
        config = config.with_access_code(code);
    }

    // Create and run emulator
    let emulator = Emulator::new(config);
    emulator.run().await?;

    Ok(())
}
