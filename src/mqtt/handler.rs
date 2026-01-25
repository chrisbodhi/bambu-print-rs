//! MQTT command handler

use anyhow::Result;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::messages::{Command, PrintCommandData, ReportMessage, RequestMessage};
use crate::state::{PrintJob, PrinterState};

/// Run the command handler (subscribes to request topic)
pub async fn run_command_handler(
    state: Arc<RwLock<PrinterState>>,
    serial_number: String,
    mqtt_port: u16,
) -> Result<()> {
    info!("Starting command handler");

    // Create MQTT client
    let mut mqttoptions = MqttOptions::new("bambu-handler", "127.0.0.1", mqtt_port);
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
                        warn!(
                            "Failed to parse request message: {}. Payload: {}",
                            e, payload
                        );
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
                        if let Err(e) = client
                            .publish(&report_topic, QoS::AtLeastOnce, false, json)
                            .await
                        {
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
            handle_print_command(&print_cmd.print, state, client, report_topic).await;
        }
    }
}

/// Handle print-related commands
async fn handle_print_command(
    cmd: &PrintCommandData,
    state: Arc<RwLock<PrinterState>>,
    client: AsyncClient,
    report_topic: String,
) {
    match cmd.command.as_str() {
        "project_file" => {
            info!(
                "Starting print job: {:?}",
                cmd.subtask_name.as_deref().unwrap_or("unknown")
            );

            // Create a new print job from the command
            let job = PrintJob::new(
                Uuid::new_v4().to_string(),
                cmd.subtask_name.clone().unwrap_or_else(|| "print".to_string()),
                cmd.param.clone().unwrap_or_else(|| "/cache/print.gcode".to_string()),
                1, // Default plate number
                100, // Default layer count (would come from 3MF parsing)
                60,  // Default 60 minute estimate
                cmd.bed_levelling.unwrap_or(true),
                cmd.use_ams.unwrap_or(false),
                cmd.ams_mapping.clone().unwrap_or_else(|| vec![0]),
                210.0, // Default nozzle temp (would come from filament type)
                55.0,  // Default bed temp (would come from filament type)
            );

            {
                let mut state_guard = state.write().await;
                state_guard.start_print(job);
            }

            // Publish updated state
            publish_state(&state, &client, &report_topic).await;
        }
        "pause" => {
            info!("Pausing print");
            let paused = {
                let mut state_guard = state.write().await;
                state_guard.pause_print()
            };

            if paused {
                info!("Print paused successfully");
                publish_state(&state, &client, &report_topic).await;
            } else {
                warn!("Failed to pause print - no active running print");
            }
        }
        "resume" => {
            info!("Resuming print");
            let resumed = {
                let mut state_guard = state.write().await;
                state_guard.resume_print()
            };

            if resumed {
                info!("Print resumed successfully");
                publish_state(&state, &client, &report_topic).await;
            } else {
                warn!("Failed to resume print - no paused print");
            }
        }
        "stop" => {
            info!("Stopping print");
            let stopped = {
                let mut state_guard = state.write().await;
                state_guard.stop_print()
            };

            if stopped {
                info!("Print stopped successfully");
                publish_state(&state, &client, &report_topic).await;
            } else {
                warn!("Failed to stop print - no active print");
            }
        }
        "gcode_line" => {
            // Handle G-code commands (primarily temperature commands)
            if let Some(ref gcode) = cmd.param {
                handle_gcode_line(gcode, &state).await;
            }
        }
        _ => {
            warn!("Unknown print command: {}", cmd.command);
        }
    }
}

/// Handle G-code line commands (temperature setting, etc.)
async fn handle_gcode_line(gcode: &str, state: &Arc<RwLock<PrinterState>>) {
    // Parse common temperature commands
    let gcode = gcode.trim().to_uppercase();

    if gcode.starts_with("M104") || gcode.starts_with("M109") {
        // Set nozzle temperature (M104 = set, M109 = set and wait)
        if let Some(temp) = parse_temperature_param(&gcode) {
            info!("Setting nozzle target temperature to {}°C", temp);
            let mut state_guard = state.write().await;
            state_guard.set_nozzle_target(temp);
        }
    } else if gcode.starts_with("M140") || gcode.starts_with("M190") {
        // Set bed temperature (M140 = set, M190 = set and wait)
        if let Some(temp) = parse_temperature_param(&gcode) {
            info!("Setting bed target temperature to {}°C", temp);
            let mut state_guard = state.write().await;
            state_guard.set_bed_target(temp);
        }
    } else {
        debug!("Ignoring G-code command: {}", gcode);
    }
}

/// Parse temperature parameter (S value) from G-code
fn parse_temperature_param(gcode: &str) -> Option<f32> {
    // Look for S parameter (e.g., "M104 S210")
    for part in gcode.split_whitespace() {
        if part.starts_with('S') {
            if let Ok(temp) = part[1..].parse::<f32>() {
                return Some(temp);
            }
        }
    }
    None
}

/// Publish the current state to MQTT
async fn publish_state(
    state: &Arc<RwLock<PrinterState>>,
    client: &AsyncClient,
    report_topic: &str,
) {
    let state_snapshot = state.read().await.clone();
    let report = ReportMessage::from_state(&state_snapshot);

    match serde_json::to_string(&report) {
        Ok(json) => {
            if let Err(e) = client
                .publish(report_topic, QoS::AtLeastOnce, false, json)
                .await
            {
                error!("Failed to publish state: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to serialize state: {}", e);
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
        Command::Print(print_cmd) => {
            match print_cmd.print.command.as_str() {
                "project_file" => {
                    let job = PrintJob::new(
                        Uuid::new_v4().to_string(),
                        print_cmd.print.subtask_name.unwrap_or_else(|| "print".to_string()),
                        print_cmd.print.param.unwrap_or_else(|| "/cache/print.gcode".to_string()),
                        1,
                        100,
                        60,
                        print_cmd.print.bed_levelling.unwrap_or(true),
                        print_cmd.print.use_ams.unwrap_or(false),
                        print_cmd.print.ams_mapping.unwrap_or_else(|| vec![0]),
                        210.0,
                        55.0,
                    );
                    state.start_print(job);
                }
                "pause" => {
                    state.pause_print();
                }
                "resume" => {
                    state.resume_print();
                }
                "stop" => {
                    state.stop_print();
                }
                "gcode_line" => {
                    if let Some(ref gcode) = print_cmd.print.param {
                        let gcode = gcode.trim().to_uppercase();
                        if gcode.starts_with("M104") || gcode.starts_with("M109") {
                            if let Some(temp) = parse_temperature_param(&gcode) {
                                state.set_nozzle_target(temp);
                            }
                        } else if gcode.starts_with("M140") || gcode.starts_with("M190") {
                            if let Some(temp) = parse_temperature_param(&gcode) {
                                state.set_bed_target(temp);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mqtt::messages::{PrintCommand, PrintCommandData, SystemCommand, SystemCommandData};
    use crate::state::GcodeState;

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

    #[test]
    fn test_process_project_file() {
        let state = PrinterState::new("TEST123".to_string());
        assert!(state.print_job.is_none());
        assert_eq!(state.gcode_state, GcodeState::Idle);

        let command = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "1".to_string(),
                command: "project_file".to_string(),
                param: Some("Metadata/plate_1.gcode".to_string()),
                url: Some("ftp:///model.3mf".to_string()),
                subtask_name: Some("benchy".to_string()),
                use_ams: Some(true),
                ams_mapping: Some(vec![0, 1]),
                timelapse: Some(true),
                bed_levelling: Some(true),
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        let new_state = process_command_pure(command, state);
        assert!(new_state.print_job.is_some());
        assert_eq!(new_state.gcode_state, GcodeState::Prepare);

        let job = new_state.print_job.as_ref().unwrap();
        assert_eq!(job.subtask_name, "benchy");
        assert!(job.use_ams);
        assert!(job.bed_levelling);
    }

    #[test]
    fn test_process_pause_command() {
        let mut state = PrinterState::new("TEST123".to_string());

        // Start a print first
        let start_cmd = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "1".to_string(),
                command: "project_file".to_string(),
                param: None,
                url: None,
                subtask_name: Some("test".to_string()),
                use_ams: None,
                ams_mapping: None,
                timelapse: None,
                bed_levelling: None,
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        state = process_command_pure(start_cmd, state);
        state.gcode_state = GcodeState::Running; // Simulate being in running state

        // Pause the print
        let pause_cmd = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "2".to_string(),
                command: "pause".to_string(),
                param: None,
                url: None,
                subtask_name: None,
                use_ams: None,
                ams_mapping: None,
                timelapse: None,
                bed_levelling: None,
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        let new_state = process_command_pure(pause_cmd, state);
        assert_eq!(new_state.gcode_state, GcodeState::Pause);
    }

    #[test]
    fn test_process_resume_command() {
        let mut state = PrinterState::new("TEST123".to_string());

        // Start and pause a print
        let start_cmd = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "1".to_string(),
                command: "project_file".to_string(),
                param: None,
                url: None,
                subtask_name: Some("test".to_string()),
                use_ams: None,
                ams_mapping: None,
                timelapse: None,
                bed_levelling: None,
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        state = process_command_pure(start_cmd, state);
        state.gcode_state = GcodeState::Pause;

        // Resume the print
        let resume_cmd = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "2".to_string(),
                command: "resume".to_string(),
                param: None,
                url: None,
                subtask_name: None,
                use_ams: None,
                ams_mapping: None,
                timelapse: None,
                bed_levelling: None,
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        let new_state = process_command_pure(resume_cmd, state);
        assert_eq!(new_state.gcode_state, GcodeState::Running);
    }

    #[test]
    fn test_process_stop_command() {
        let mut state = PrinterState::new("TEST123".to_string());

        // Start a print
        let start_cmd = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "1".to_string(),
                command: "project_file".to_string(),
                param: None,
                url: None,
                subtask_name: Some("test".to_string()),
                use_ams: None,
                ams_mapping: None,
                timelapse: None,
                bed_levelling: None,
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        state = process_command_pure(start_cmd, state);
        assert!(state.print_job.is_some());

        // Stop the print
        let stop_cmd = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "2".to_string(),
                command: "stop".to_string(),
                param: None,
                url: None,
                subtask_name: None,
                use_ams: None,
                ams_mapping: None,
                timelapse: None,
                bed_levelling: None,
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        let new_state = process_command_pure(stop_cmd, state);
        assert!(new_state.print_job.is_none());
        assert_eq!(new_state.gcode_state, GcodeState::Idle);
    }

    #[test]
    fn test_process_gcode_line_nozzle_temp() {
        let state = PrinterState::new("TEST123".to_string());
        assert_eq!(state.nozzle_target_temp, 0.0);

        let command = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "1".to_string(),
                command: "gcode_line".to_string(),
                param: Some("M104 S210\n".to_string()),
                url: None,
                subtask_name: None,
                use_ams: None,
                ams_mapping: None,
                timelapse: None,
                bed_levelling: None,
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        let new_state = process_command_pure(command, state);
        assert_eq!(new_state.nozzle_target_temp, 210.0);
    }

    #[test]
    fn test_process_gcode_line_bed_temp() {
        let state = PrinterState::new("TEST123".to_string());
        assert_eq!(state.bed_target_temp, 0.0);

        let command = Command::Print(PrintCommand {
            print: PrintCommandData {
                sequence_id: "1".to_string(),
                command: "gcode_line".to_string(),
                param: Some("M140 S60\n".to_string()),
                url: None,
                subtask_name: None,
                use_ams: None,
                ams_mapping: None,
                timelapse: None,
                bed_levelling: None,
                flow_cali: None,
                vibration_cali: None,
                layer_inspect: None,
            },
        });

        let new_state = process_command_pure(command, state);
        assert_eq!(new_state.bed_target_temp, 60.0);
    }

    #[test]
    fn test_parse_temperature_param() {
        assert_eq!(parse_temperature_param("M104 S210"), Some(210.0));
        assert_eq!(parse_temperature_param("M109 S200"), Some(200.0));
        assert_eq!(parse_temperature_param("M140 S55"), Some(55.0));
        assert_eq!(parse_temperature_param("M104"), None);
        assert_eq!(parse_temperature_param("G28"), None);
    }
}
