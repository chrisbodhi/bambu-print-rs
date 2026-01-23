//! MQTT status publisher

use anyhow::Result;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info};

use super::messages::ReportMessage;
use crate::state::PrinterState;

/// Run the status publishing loop
pub async fn run_status_publisher(
    state: Arc<RwLock<PrinterState>>,
    serial_number: String,
    interval_ms: u64,
    mqtt_port: u16,
) -> Result<()> {
    info!("Starting status publisher");

    // Create MQTT client
    let mut mqttoptions = MqttOptions::new("bambu-pub", "127.0.0.1", mqtt_port);
    mqttoptions.set_keep_alive(Duration::from_secs(30));

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    // Spawn a task to handle MQTT events
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    info!("Publisher connected to MQTT broker");
                }
                Ok(_) => {}
                Err(e) => {
                    error!("MQTT publisher error: {}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });

    // Wait for connection
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Publishing loop
    let topic = format!("device/{}/report", serial_number);
    let mut tick = interval(Duration::from_millis(interval_ms));

    loop {
        tick.tick().await;

        let state_snapshot = state.read().await.clone();
        let report = ReportMessage::from_state(&state_snapshot);

        match serde_json::to_string(&report) {
            Ok(json) => {
                debug!("Publishing status to {}: {} bytes", topic, json.len());
                if let Err(e) = client.publish(&topic, QoS::AtLeastOnce, false, json).await {
                    error!("Failed to publish status: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to serialize report: {}", e);
            }
        }
    }
}

/// Publish a single status report (useful for testing)
pub async fn publish_status(
    client: &AsyncClient,
    serial_number: &str,
    state: &PrinterState,
) -> Result<()> {
    let report = ReportMessage::from_state(state);
    let json = serde_json::to_string(&report)?;
    let topic = format!("device/{}/report", serial_number);

    client
        .publish(&topic, QoS::AtLeastOnce, false, json)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_topic_format() {
        let serial = "TEST123";
        let topic = format!("device/{}/report", serial);
        assert_eq!(topic, "device/TEST123/report");
    }

    #[test]
    fn test_serialize_report() {
        let state = PrinterState::new("TEST456".to_string());
        let report = ReportMessage::from_state(&state);
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("print"));
        assert!(json.contains("IDLE"));
    }

    #[test]
    fn test_mqtt_options_creation() {
        let mqttoptions = MqttOptions::new("test-publisher", "127.0.0.1", 1883);
        assert_eq!(
            mqttoptions.broker_address(),
            ("127.0.0.1".to_string(), 1883)
        );
    }
}
