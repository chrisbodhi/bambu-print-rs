//! Printer state types

use serde::{Deserialize, Serialize};

/// Main printer state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterState {
    // Identity
    pub serial_number: String,
    pub model: PrinterModel,
    pub firmware_version: String,

    // Temperatures (Celsius)
    pub nozzle_temp: f32,
    pub nozzle_target_temp: f32,
    pub bed_temp: f32,
    pub bed_target_temp: f32,
    pub chamber_temp: f32,

    // Print state
    pub gcode_state: GcodeState,

    // Hardware
    pub ams: super::AmsState,
    pub lights_on: bool,
    pub door_open: bool,
    pub sd_card_present: bool,

    // Network
    pub wifi_signal: String,

    // Fans (0-15 scale for most fans)
    pub heatbreak_fan_speed: u8,
    pub cooling_fan_speed: u8,
    pub aux_fan_speed: u8,
    pub chamber_fan_speed: u8,

    // Speed
    pub speed_level: SpeedLevel,
    pub speed_magnitude: u8, // 50-166 (percentage)
}

impl PrinterState {
    /// Create a new printer state with default values
    pub fn new(serial_number: String) -> Self {
        Self {
            serial_number,
            model: PrinterModel::X1Carbon,
            firmware_version: "01.08.00.00".to_string(),

            nozzle_temp: 25.0,
            nozzle_target_temp: 0.0,
            bed_temp: 25.0,
            bed_target_temp: 0.0,
            chamber_temp: 25.0,

            gcode_state: GcodeState::Idle,

            ams: super::AmsState::new(),
            lights_on: false,
            door_open: false,
            sd_card_present: true,

            wifi_signal: "-48dBm".to_string(),

            heatbreak_fan_speed: 0,
            cooling_fan_speed: 0,
            aux_fan_speed: 0,
            chamber_fan_speed: 0,

            speed_level: SpeedLevel::Standard,
            speed_magnitude: 100,
        }
    }

    /// Toggle chamber light
    pub fn toggle_light(&mut self) {
        self.lights_on = !self.lights_on;
    }

    /// Set chamber light state
    pub fn set_light(&mut self, on: bool) {
        self.lights_on = on;
    }
}

/// Printer model
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrinterModel {
    X1Carbon,
    X1,
    P1S,
    P1P,
    A1,
    A1Mini,
}

impl PrinterModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrinterModel::X1Carbon => "X1 Carbon",
            PrinterModel::X1 => "X1",
            PrinterModel::P1S => "P1S",
            PrinterModel::P1P => "P1P",
            PrinterModel::A1 => "A1",
            PrinterModel::A1Mini => "A1 Mini",
        }
    }
}

/// G-code execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GcodeState {
    #[serde(rename = "IDLE")]
    Idle,
    #[serde(rename = "PREPARE")]
    Prepare,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "PAUSE")]
    Pause,
    #[serde(rename = "FINISH")]
    Finish,
    #[serde(rename = "FAILED")]
    Failed,
}

impl GcodeState {
    pub fn as_str(&self) -> &'static str {
        match self {
            GcodeState::Idle => "IDLE",
            GcodeState::Prepare => "PREPARE",
            GcodeState::Running => "RUNNING",
            GcodeState::Pause => "PAUSE",
            GcodeState::Finish => "FINISH",
            GcodeState::Failed => "FAILED",
        }
    }
}

/// Speed level presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SpeedLevel {
    Silent = 1,
    Standard = 2,
    Sport = 3,
    Ludicrous = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_printer_state() {
        let state = PrinterState::new("TEST123".to_string());
        assert_eq!(state.serial_number, "TEST123");
        assert_eq!(state.model, PrinterModel::X1Carbon);
        assert_eq!(state.gcode_state, GcodeState::Idle);
        assert_eq!(state.nozzle_temp, 25.0);
        assert_eq!(state.bed_temp, 25.0);
        assert!(!state.lights_on);
    }

    #[test]
    fn test_toggle_light() {
        let mut state = PrinterState::new("TEST123".to_string());
        assert!(!state.lights_on);
        state.toggle_light();
        assert!(state.lights_on);
        state.toggle_light();
        assert!(!state.lights_on);
    }

    #[test]
    fn test_set_light() {
        let mut state = PrinterState::new("TEST123".to_string());
        state.set_light(true);
        assert!(state.lights_on);
        state.set_light(false);
        assert!(!state.lights_on);
    }

    #[test]
    fn test_gcode_state_serialization() {
        let idle = GcodeState::Idle;
        let json = serde_json::to_string(&idle).unwrap();
        assert_eq!(json, r#""IDLE""#);

        let running = GcodeState::Running;
        let json = serde_json::to_string(&running).unwrap();
        assert_eq!(json, r#""RUNNING""#);
    }

    #[test]
    fn test_gcode_state_deserialization() {
        let idle: GcodeState = serde_json::from_str(r#""IDLE""#).unwrap();
        assert_eq!(idle, GcodeState::Idle);

        let running: GcodeState = serde_json::from_str(r#""RUNNING""#).unwrap();
        assert_eq!(running, GcodeState::Running);
    }

    #[test]
    fn test_printer_model_as_str() {
        assert_eq!(PrinterModel::X1Carbon.as_str(), "X1 Carbon");
        assert_eq!(PrinterModel::P1S.as_str(), "P1S");
        assert_eq!(PrinterModel::A1Mini.as_str(), "A1 Mini");
    }

    #[test]
    fn test_speed_level_values() {
        assert_eq!(SpeedLevel::Silent as u8, 1);
        assert_eq!(SpeedLevel::Standard as u8, 2);
        assert_eq!(SpeedLevel::Sport as u8, 3);
        assert_eq!(SpeedLevel::Ludicrous as u8, 4);
    }

    #[test]
    fn test_printer_state_serialization() {
        let state = PrinterState::new("ABC123".to_string());
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("ABC123"));
        assert!(json.contains("IDLE"));
    }

    #[test]
    fn test_printer_state_round_trip() {
        let state = PrinterState::new("XYZ789".to_string());
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: PrinterState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.serial_number, state.serial_number);
        assert_eq!(deserialized.gcode_state, state.gcode_state);
    }
}
