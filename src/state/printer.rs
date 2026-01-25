//! Printer state types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

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
    pub print_job: Option<PrintJob>,

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

/// Active print job state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintJob {
    pub task_id: String,
    pub subtask_name: String,
    pub gcode_file: String,
    pub plate_number: u8,

    // Progress
    pub mc_percent: u8,         // 0-100
    pub layer_num: u32,
    pub total_layer_num: u32,
    pub mc_remaining_time: u32, // minutes

    // Timing
    #[serde(with = "chrono::serde::ts_seconds")]
    pub start_time: DateTime<Utc>,
    #[serde(with = "duration_serde")]
    pub estimated_total_time: Duration,

    // Simulation bookkeeping
    pub current_stage: PrintStage,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub stage_started_at: DateTime<Utc>,

    // Print options
    pub bed_levelling: bool,
    pub use_ams: bool,
    pub ams_mapping: Vec<u8>,
}

impl PrintJob {
    /// Create a new print job
    pub fn new(
        task_id: String,
        subtask_name: String,
        gcode_file: String,
        plate_number: u8,
        total_layers: u32,
        estimated_time_mins: u32,
        bed_levelling: bool,
        use_ams: bool,
        ams_mapping: Vec<u8>,
        target_nozzle_temp: f32,
        target_bed_temp: f32,
    ) -> Self {
        let now = Utc::now();
        Self {
            task_id,
            subtask_name,
            gcode_file,
            plate_number,
            mc_percent: 0,
            layer_num: 0,
            total_layer_num: total_layers,
            mc_remaining_time: estimated_time_mins,
            start_time: now,
            estimated_total_time: Duration::from_secs((estimated_time_mins as u64) * 60),
            current_stage: PrintStage::Heating {
                target_nozzle: target_nozzle_temp,
                target_bed: target_bed_temp,
            },
            stage_started_at: now,
            bed_levelling,
            use_ams,
            ams_mapping,
        }
    }

    /// Advance to the next print stage
    pub fn advance_stage(&mut self, next_stage: PrintStage) {
        self.current_stage = next_stage;
        self.stage_started_at = Utc::now();
    }

    /// Update progress based on layer completion
    pub fn update_progress(&mut self, layer: u32) {
        self.layer_num = layer.min(self.total_layer_num);
        if self.total_layer_num > 0 {
            self.mc_percent = ((self.layer_num as f32 / self.total_layer_num as f32) * 100.0) as u8;
        }
    }
}

/// Print simulation stages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PrintStage {
    /// Heating nozzle and bed to target temperatures
    Heating { target_nozzle: f32, target_bed: f32 },
    /// Auto bed leveling
    BedLeveling,
    /// Purging/priming the nozzle
    Purging,
    /// Active printing
    Printing,
    /// Cooling down after print
    Cooling,
    /// Print complete
    Complete,
}

impl PrintStage {
    /// Get the stage number for MQTT reporting (stg_cur field)
    pub fn as_stage_number(&self) -> u8 {
        match self {
            PrintStage::Heating { .. } => 0,
            PrintStage::BedLeveling => 1,
            PrintStage::Purging => 2,
            PrintStage::Printing => 3,
            PrintStage::Cooling => 4,
            PrintStage::Complete => 5,
        }
    }
}

/// Custom serialization for Duration
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
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
            print_job: None,

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

    /// Start a new print job
    pub fn start_print(&mut self, job: PrintJob) {
        self.gcode_state = GcodeState::Prepare;
        self.nozzle_target_temp = match &job.current_stage {
            PrintStage::Heating { target_nozzle, .. } => *target_nozzle,
            _ => 210.0,
        };
        self.bed_target_temp = match &job.current_stage {
            PrintStage::Heating { target_bed, .. } => *target_bed,
            _ => 55.0,
        };
        self.print_job = Some(job);
    }

    /// Pause the current print job
    pub fn pause_print(&mut self) -> bool {
        if self.print_job.is_some() && self.gcode_state == GcodeState::Running {
            self.gcode_state = GcodeState::Pause;
            true
        } else {
            false
        }
    }

    /// Resume a paused print job
    pub fn resume_print(&mut self) -> bool {
        if self.print_job.is_some() && self.gcode_state == GcodeState::Pause {
            self.gcode_state = GcodeState::Running;
            true
        } else {
            false
        }
    }

    /// Stop/cancel the current print job
    pub fn stop_print(&mut self) -> bool {
        if self.print_job.is_some() {
            self.print_job = None;
            self.gcode_state = GcodeState::Idle;
            self.nozzle_target_temp = 0.0;
            self.bed_target_temp = 0.0;
            true
        } else {
            false
        }
    }

    /// Complete the current print job
    pub fn complete_print(&mut self) {
        self.print_job = None;
        self.gcode_state = GcodeState::Finish;
        self.nozzle_target_temp = 0.0;
        self.bed_target_temp = 0.0;
    }

    /// Set nozzle target temperature (for gcode commands)
    pub fn set_nozzle_target(&mut self, temp: f32) {
        self.nozzle_target_temp = temp;
    }

    /// Set bed target temperature (for gcode commands)
    pub fn set_bed_target(&mut self, temp: f32) {
        self.bed_target_temp = temp;
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

    #[test]
    fn test_print_job_new() {
        let job = PrintJob::new(
            "task123".to_string(),
            "benchy.3mf".to_string(),
            "/cache/benchy.gcode".to_string(),
            1,
            100,
            60,
            true,
            true,
            vec![0, 1, 2, 3],
            210.0,
            55.0,
        );

        assert_eq!(job.task_id, "task123");
        assert_eq!(job.subtask_name, "benchy.3mf");
        assert_eq!(job.total_layer_num, 100);
        assert_eq!(job.mc_percent, 0);
        assert_eq!(job.layer_num, 0);
        assert!(job.bed_levelling);
        assert!(job.use_ams);
        assert_eq!(
            job.current_stage,
            PrintStage::Heating {
                target_nozzle: 210.0,
                target_bed: 55.0
            }
        );
    }

    #[test]
    fn test_print_job_update_progress() {
        let mut job = PrintJob::new(
            "task123".to_string(),
            "benchy.3mf".to_string(),
            "/cache/benchy.gcode".to_string(),
            1,
            100,
            60,
            true,
            true,
            vec![0],
            210.0,
            55.0,
        );

        job.update_progress(50);
        assert_eq!(job.layer_num, 50);
        assert_eq!(job.mc_percent, 50);

        job.update_progress(100);
        assert_eq!(job.layer_num, 100);
        assert_eq!(job.mc_percent, 100);

        // Can't exceed total layers
        job.update_progress(150);
        assert_eq!(job.layer_num, 100);
    }

    #[test]
    fn test_print_job_advance_stage() {
        let mut job = PrintJob::new(
            "task123".to_string(),
            "benchy.3mf".to_string(),
            "/cache/benchy.gcode".to_string(),
            1,
            100,
            60,
            true,
            true,
            vec![0],
            210.0,
            55.0,
        );

        job.advance_stage(PrintStage::BedLeveling);
        assert_eq!(job.current_stage, PrintStage::BedLeveling);

        job.advance_stage(PrintStage::Printing);
        assert_eq!(job.current_stage, PrintStage::Printing);
    }

    #[test]
    fn test_print_stage_numbers() {
        assert_eq!(
            PrintStage::Heating {
                target_nozzle: 210.0,
                target_bed: 55.0
            }
            .as_stage_number(),
            0
        );
        assert_eq!(PrintStage::BedLeveling.as_stage_number(), 1);
        assert_eq!(PrintStage::Purging.as_stage_number(), 2);
        assert_eq!(PrintStage::Printing.as_stage_number(), 3);
        assert_eq!(PrintStage::Cooling.as_stage_number(), 4);
        assert_eq!(PrintStage::Complete.as_stage_number(), 5);
    }

    #[test]
    fn test_start_print() {
        let mut state = PrinterState::new("TEST123".to_string());
        assert!(state.print_job.is_none());
        assert_eq!(state.gcode_state, GcodeState::Idle);

        let job = PrintJob::new(
            "task123".to_string(),
            "benchy.3mf".to_string(),
            "/cache/benchy.gcode".to_string(),
            1,
            100,
            60,
            true,
            true,
            vec![0],
            210.0,
            55.0,
        );

        state.start_print(job);

        assert!(state.print_job.is_some());
        assert_eq!(state.gcode_state, GcodeState::Prepare);
        assert_eq!(state.nozzle_target_temp, 210.0);
        assert_eq!(state.bed_target_temp, 55.0);
    }

    #[test]
    fn test_pause_resume_print() {
        let mut state = PrinterState::new("TEST123".to_string());
        let job = PrintJob::new(
            "task123".to_string(),
            "benchy.3mf".to_string(),
            "/cache/benchy.gcode".to_string(),
            1,
            100,
            60,
            true,
            true,
            vec![0],
            210.0,
            55.0,
        );

        state.start_print(job);
        state.gcode_state = GcodeState::Running;

        // Pause should succeed
        assert!(state.pause_print());
        assert_eq!(state.gcode_state, GcodeState::Pause);

        // Resume should succeed
        assert!(state.resume_print());
        assert_eq!(state.gcode_state, GcodeState::Running);

        // Pause when not running should fail
        state.gcode_state = GcodeState::Pause;
        assert!(!state.pause_print());
    }

    #[test]
    fn test_stop_print() {
        let mut state = PrinterState::new("TEST123".to_string());
        let job = PrintJob::new(
            "task123".to_string(),
            "benchy.3mf".to_string(),
            "/cache/benchy.gcode".to_string(),
            1,
            100,
            60,
            true,
            true,
            vec![0],
            210.0,
            55.0,
        );

        state.start_print(job);

        assert!(state.stop_print());
        assert!(state.print_job.is_none());
        assert_eq!(state.gcode_state, GcodeState::Idle);
        assert_eq!(state.nozzle_target_temp, 0.0);
        assert_eq!(state.bed_target_temp, 0.0);

        // Stop when no job should fail
        assert!(!state.stop_print());
    }

    #[test]
    fn test_complete_print() {
        let mut state = PrinterState::new("TEST123".to_string());
        let job = PrintJob::new(
            "task123".to_string(),
            "benchy.3mf".to_string(),
            "/cache/benchy.gcode".to_string(),
            1,
            100,
            60,
            true,
            true,
            vec![0],
            210.0,
            55.0,
        );

        state.start_print(job);
        state.complete_print();

        assert!(state.print_job.is_none());
        assert_eq!(state.gcode_state, GcodeState::Finish);
        assert_eq!(state.nozzle_target_temp, 0.0);
        assert_eq!(state.bed_target_temp, 0.0);
    }
}
