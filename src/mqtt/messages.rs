//! MQTT message types for Bambu Lab protocol

use crate::state::PrinterState;
use serde::{Deserialize, Serialize};

/// Top-level report message (emulator -> client)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMessage {
    pub print: PrintReport,
}

impl ReportMessage {
    /// Create a report message from printer state
    pub fn from_state(state: &PrinterState) -> Self {
        Self {
            print: PrintReport::from_state(state),
        }
    }
}

/// Print status report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintReport {
    // Temperatures
    pub nozzle_temper: f32,
    pub nozzle_target_temper: f32,
    pub bed_temper: f32,
    pub bed_target_temper: f32,
    pub chamber_temper: f32,

    // Print state
    pub gcode_state: String,

    // Print progress (when printing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mc_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mc_remaining_time: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_num: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_layer_num: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtask_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gcode_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stg_cur: Option<u8>,

    // Hardware
    pub wifi_signal: String,
    pub spd_lvl: u8,
    pub spd_mag: u8,

    // Fans (reported as strings in the real protocol)
    pub heatbreak_fan_speed: String,
    pub cooling_fan_speed: String,
    pub big_fan1_speed: String,
    pub big_fan2_speed: String,

    // Lights
    pub lights_report: Vec<LightReport>,

    // AMS
    pub ams: AmsReport,

    // Upload status
    pub upload: UploadStatus,

    // Hardware Management System (errors)
    pub hms: Vec<serde_json::Value>, // Empty for now

    // SD card
    pub sdcard: bool,
}

impl PrintReport {
    /// Create a print report from printer state
    pub fn from_state(state: &PrinterState) -> Self {
        // Extract print job info if present
        let (mc_percent, mc_remaining_time, layer_num, total_layer_num, subtask_name, gcode_file, stg_cur) =
            if let Some(ref job) = state.print_job {
                (
                    Some(job.mc_percent),
                    Some(job.mc_remaining_time),
                    Some(job.layer_num),
                    Some(job.total_layer_num),
                    Some(job.subtask_name.clone()),
                    Some(job.gcode_file.clone()),
                    Some(job.current_stage.as_stage_number()),
                )
            } else {
                (None, None, None, None, None, None, None)
            };

        Self {
            nozzle_temper: state.nozzle_temp,
            nozzle_target_temper: state.nozzle_target_temp,
            bed_temper: state.bed_temp,
            bed_target_temper: state.bed_target_temp,
            chamber_temper: state.chamber_temp,

            gcode_state: state.gcode_state.as_str().to_string(),

            mc_percent,
            mc_remaining_time,
            layer_num,
            total_layer_num,
            subtask_name,
            gcode_file,
            stg_cur,

            wifi_signal: state.wifi_signal.clone(),
            spd_lvl: state.speed_level as u8,
            spd_mag: state.speed_magnitude,

            heatbreak_fan_speed: state.heatbreak_fan_speed.to_string(),
            cooling_fan_speed: state.cooling_fan_speed.to_string(),
            big_fan1_speed: state.aux_fan_speed.to_string(),
            big_fan2_speed: state.chamber_fan_speed.to_string(),

            lights_report: vec![LightReport {
                node: "chamber_light".to_string(),
                mode: if state.lights_on { "on" } else { "off" }.to_string(),
            }],

            ams: AmsReport::from_state(&state.ams),

            upload: UploadStatus {
                status: "idle".to_string(),
                progress: 0,
                message: "".to_string(),
            },

            hms: vec![],
            sdcard: state.sd_card_present,
        }
    }
}

/// Light status report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightReport {
    pub node: String,
    pub mode: String,
}

/// AMS status report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsReport {
    pub ams: Vec<AmsUnitReport>,
    pub ams_exist_bits: String,
    pub tray_exist_bits: String,
    pub tray_now: String,
    pub version: u32,
}

impl AmsReport {
    fn from_state(ams_state: &crate::state::AmsState) -> Self {
        Self {
            ams: ams_state
                .units
                .iter()
                .map(AmsUnitReport::from_unit)
                .collect(),
            ams_exist_bits: "1".to_string(),  // One AMS unit
            tray_exist_bits: "f".to_string(), // All 4 trays (0xF = 1111 binary)
            tray_now: ams_state
                .current_tray
                .map_or("0".to_string(), |t| t.to_string()),
            version: 3,
        }
    }
}

/// AMS unit report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsUnitReport {
    pub id: String,
    pub humidity: String,
    pub temp: String,
    pub tray: Vec<TrayReport>,
}

impl AmsUnitReport {
    fn from_unit(unit: &crate::state::AmsUnit) -> Self {
        Self {
            id: unit.id.to_string(),
            humidity: unit.humidity.to_string(),
            temp: unit.temp.to_string(),
            tray: unit.trays.iter().map(TrayReport::from_tray).collect(),
        }
    }
}

/// Filament tray report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayReport {
    pub id: String,
    pub tray_type: String,
    pub tray_color: String,
    pub remain: u8,
    pub nozzle_temp_min: u16,
    pub nozzle_temp_max: u16,
    pub bed_temp: u16,
}

impl TrayReport {
    fn from_tray(tray: &crate::state::FilamentTray) -> Self {
        Self {
            id: tray.id.to_string(),
            tray_type: tray.filament_type.clone(),
            tray_color: tray.color.clone(),
            remain: tray.remaining_percent,
            nozzle_temp_min: tray.nozzle_temp_min,
            nozzle_temp_max: tray.nozzle_temp_max,
            bed_temp: tray.bed_temp,
        }
    }
}

/// Upload status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStatus {
    pub status: String,
    pub progress: u8,
    pub message: String,
}

/// Top-level request message (client -> emulator)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMessage {
    #[serde(flatten)]
    pub command: Command,
}

/// Command variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Command {
    Print(PrintCommand),
    System(SystemCommand),
    Pushing(PushingCommand),
}

/// Print command wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintCommand {
    pub print: PrintCommandData,
}

/// Print command data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintCommandData {
    pub sequence_id: String,
    pub command: String,
    /// G-code file path (for project_file) or G-code line content (for gcode_line)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    /// FTP URL to the .3mf file (e.g., "ftp:///model.3mf")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Name of the print job
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtask_name: Option<String>,
    /// Whether to use AMS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_ams: Option<bool>,
    /// AMS tray mapping (e.g., [0, 1, 2, 3])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ams_mapping: Option<Vec<u8>>,
    /// Enable timelapse recording
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timelapse: Option<bool>,
    /// Enable bed leveling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bed_levelling: Option<bool>,
    /// Enable flow calibration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_cali: Option<bool>,
    /// Enable vibration calibration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibration_cali: Option<bool>,
    /// Enable layer inspection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_inspect: Option<bool>,
}

/// System command wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCommand {
    pub system: SystemCommandData,
}

/// System command data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemCommandData {
    pub sequence_id: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub led_node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub led_mode: Option<String>,
}

/// Pushing command wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushingCommand {
    pub pushing: PushingCommandData,
}

/// Pushing command data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushingCommandData {
    pub sequence_id: String,
    pub command: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PrinterState;

    #[test]
    fn test_report_message_from_state() {
        let state = PrinterState::new("TEST123".to_string());
        let report = ReportMessage::from_state(&state);

        assert_eq!(report.print.gcode_state, "IDLE");
        assert_eq!(report.print.nozzle_temper, 25.0);
        assert_eq!(report.print.bed_temper, 25.0);
        assert!(report.print.sdcard);
    }

    #[test]
    fn test_report_serialization() {
        let state = PrinterState::new("TEST123".to_string());
        let report = ReportMessage::from_state(&state);
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("print"));
        assert!(json.contains("IDLE"));
        assert!(json.contains("nozzle_temper"));
    }

    #[test]
    fn test_pushall_command_parsing() {
        let json = r#"{"pushing":{"sequence_id":"123","command":"pushall"}}"#;
        let cmd: RequestMessage = serde_json::from_str(json).unwrap();

        match cmd.command {
            Command::Pushing(pushing) => {
                assert_eq!(pushing.pushing.command, "pushall");
                assert_eq!(pushing.pushing.sequence_id, "123");
            }
            _ => panic!("Expected pushing command"),
        }
    }

    #[test]
    fn test_pause_command_parsing() {
        let json = r#"{"print":{"sequence_id":"124","command":"pause"}}"#;
        let cmd: RequestMessage = serde_json::from_str(json).unwrap();

        match cmd.command {
            Command::Print(print) => {
                assert_eq!(print.print.command, "pause");
                assert_eq!(print.print.sequence_id, "124");
            }
            _ => panic!("Expected print command"),
        }
    }

    #[test]
    fn test_ledctrl_command_parsing() {
        let json = r#"{"system":{"sequence_id":"128","command":"ledctrl","led_node":"chamber_light","led_mode":"on"}}"#;
        let cmd: RequestMessage = serde_json::from_str(json).unwrap();

        match cmd.command {
            Command::System(system) => {
                assert_eq!(system.system.command, "ledctrl");
                assert_eq!(system.system.led_node, Some("chamber_light".to_string()));
                assert_eq!(system.system.led_mode, Some("on".to_string()));
            }
            _ => panic!("Expected system command"),
        }
    }

    #[test]
    fn test_light_report_serialization() {
        let light = LightReport {
            node: "chamber_light".to_string(),
            mode: "on".to_string(),
        };
        let json = serde_json::to_string(&light).unwrap();
        assert!(json.contains("chamber_light"));
        assert!(json.contains("on"));
    }

    #[test]
    fn test_ams_report_from_state() {
        let ams_state = crate::state::AmsState::new();
        let report = AmsReport::from_state(&ams_state);

        assert_eq!(report.ams.len(), 1);
        assert_eq!(report.ams_exist_bits, "1");
        assert_eq!(report.tray_exist_bits, "f");
    }

    #[test]
    fn test_upload_status_serialization() {
        let status = UploadStatus {
            status: "idle".to_string(),
            progress: 0,
            message: "".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("idle"));
    }
}
