//! MQTT command handler

use anyhow::Result;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

use crate::state::PrinterState;
use super::messages::{Command, RequestMessage, ReportMessage};

/// Run the command handler (subscribes to request topic)
pub async fn run_command_handler(
    state: Arc<RwLock<PrinterState>>,
    serial_number: String,
    mqtt_port: u16,
) -> Result<()> {
    info!("Starting command handler");

    // Create MQTT client
    let mut mqttoptions = MqttOptions::new("bambu-emulator-handler", "127.0.0.1", mqtt_port);
    mqttoptions.set_keep_alive(Duration::from_secs(30));

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    // Subscribe to request topic
    let request_topic = format!("device/{}/request", serial_number);
    let report_topic = format!("device/{}/report", serial_number);

    // Wait for connection and subscribe
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                info!("Command handler connected to MQTT broker");
                if let Err(e) = client.subscribe(&request_topic, QoS::AtLeastOnce).await {
                    error!("Failed to subscribe to {}: {}", request_topic, e);
                } else {
                    info!("Subscribed to {}", request_topic);
                }
            }
            Ok(Event::Incoming(Incoming::Publish(publish))) => {
                debug!(
                    "Received message on topic {}: {} bytes",
                    publish.topic,
                    publish.payload.len()
                );

                let payload = String::from_utf8_lossy(&publish.payload);
                match serde_json::from_str::<RequestMessage>(&payload) {
                    Ok(request) => {
                        let state_clone = Arc::clone(&state);
                        let client_clone = client.clone();
                        let report_topic_clone = report_topic.clone();

                        tokio::spawn(async move {
                            handle_command(
                                request.command,
                                state_clone,
                                client_clone,
                                report_topic_clone,
                            )
                            .await;
                        });
                    }
                    Err(e) => {
                        warn!("Failed to parse request message: {}. Payload: {}", e, payload);
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                error!("MQTT handler error: {}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

/// Handle a single command (sans-IO logic)
async fn handle_command(
    command: Command,
    state: Arc<RwLock<PrinterState>>,
    client: AsyncClient,
    report_topic: String,
) {
    match command {
        Command::Pushing(pushing_cmd) => {
            if pushing_cmd.pushing.command == "pushall" {
                info!("Handling pushall command");
                // Immediately publish current state
                let state_snapshot = state.read().await.clone();
                let report = ReportMessage::from_state(&state_snapshot);

                match serde_json::to_string(&report) {
                    Ok(json) => {
                        if let Err(e) = client.publish(&report_topic, QoS::AtLeastOnce, false, json).await {
                            error!("Failed to publish pushall response: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to serialize pushall response: {}", e);
                    }
                }
            }
        }
        Command::System(system_cmd) => {
            if system_cmd.system.command == "ledctrl" {
                info!("Handling ledctrl command");
                let mut state_guard = state.write().await;

                if let Some(mode) = system_cmd.system.led_mode {
                    state_guard.set_light(&mode == "on");
                    info!("Chamber light set to: {}", mode);
                }
            }
        }
        Command::Print(print_cmd) => {
            // Phase 1: We don't handle print commands yet, just log them
            info!("Received print command: {}", print_cmd.print.command);
            info!("Print commands will be implemented in Phase 2");
        }
    }
}

/// Process a command and return updated state (pure function for testing)
pub fn process_command_pure(command: Command, mut state: PrinterState) -> PrinterState {
    match command {
        Command::System(system_cmd) => {
            if system_cmd.system.command == "ledctrl" {
                if let Some(mode) = system_cmd.system.led_mode {
                    state.set_light(&mode == "on");
                }
            }
        }
        Command::Pushing(_) => {
            // Pushall doesn't modify state, just triggers a publish
        }
        Command::Print(_) => {
            // Phase 1: Not implemented yet
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mqtt::messages::{SystemCommand, SystemCommandData};

    #[test]
    fn test_process_ledctrl_on() {
        let state = PrinterState::new("TEST123".to_string());
        assert!(!state.lights_on);

        let command = Command::System(SystemCommand {
            system: SystemCommandData {
                sequence_id: "1".to_string(),
                command: "ledctrl".to_string(),
                led_node: Some("chamber_light".to_string()),
                led_mode: Some("on".to_string()),
            },
        });

        let new_state = process_command_pure(command, state);
        assert!(new_state.lights_on);
    }

    #[test]
    fn test_process_ledctrl_off() {
        let mut state = PrinterState::new("TEST123".to_string());
        state.set_light(true);
        assert!(state.lights_on);

        let command = Command::System(SystemCommand {
            system: SystemCommandData {
                sequence_id: "2".to_string(),
                command: "ledctrl".to_string(),
                led_node: Some("chamber_light".to_string()),
                led_mode: Some("off".to_string()),
            },
        });

        let new_state = process_command_pure(command, state);
        assert!(!new_state.lights_on);
    }

    #[test]
    fn test_process_pushall_no_state_change() {
        let state = PrinterState::new("TEST123".to_string());
        let original_lights = state.lights_on;

        let command = Command::Pushing(crate::mqtt::messages::PushingCommand {
            pushing: crate::mqtt::messages::PushingCommandData {
                sequence_id: "3".to_string(),
                command: "pushall".to_string(),
            },
        });

        let new_state = process_command_pure(command, state);
        assert_eq!(new_state.lights_on, original_lights);
    }

    #[test]
    fn test_request_topic_format() {
        let serial = "ABC123";
        let topic = format!("device/{}/request", serial);
        assert_eq!(topic, "device/ABC123/request");
    }
}
