//! AMS (Automatic Material System) state types

use serde::{Deserialize, Serialize};

/// AMS state for the printer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsState {
    pub units: Vec<AmsUnit>,
    pub current_tray: Option<u8>,
    pub ams_status: u8,
    pub ams_rfid_status: u8,
}

impl AmsState {
    /// Create a new AMS state with one unit and 4 trays
    pub fn new() -> Self {
        Self {
            units: vec![AmsUnit::new(0)],
            current_tray: Some(0),
            ams_status: 0,
            ams_rfid_status: 1, // RFID working
        }
    }

    /// Get a specific tray by unit and tray ID
    pub fn get_tray(&self, unit_id: u8, tray_id: u8) -> Option<&FilamentTray> {
        self.units
            .iter()
            .find(|u| u.id == unit_id)
            .and_then(|u| u.trays.get(tray_id as usize))
    }

    /// Get a mutable reference to a specific tray
    pub fn get_tray_mut(&mut self, unit_id: u8, tray_id: u8) -> Option<&mut FilamentTray> {
        self.units
            .iter_mut()
            .find(|u| u.id == unit_id)
            .and_then(|u| u.trays.get_mut(tray_id as usize))
    }
}

impl Default for AmsState {
    fn default() -> Self {
        Self::new()
    }
}

/// A single AMS unit (printer has one, can have up to 4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsUnit {
    pub id: u8,
    pub humidity: u8,    // 0-5 scale
    pub temp: f32,
    pub trays: [FilamentTray; 4],
}

impl AmsUnit {
    /// Create a new AMS unit with default trays
    pub fn new(id: u8) -> Self {
        Self {
            id,
            humidity: 3, // Mid-range humidity
            temp: 24.5,
            trays: [
                FilamentTray::new(0, "PLA", "FF5733FF"),
                FilamentTray::new(1, "PLA", "3498DBFF"),
                FilamentTray::new(2, "PETG", "2ECC71FF"),
                FilamentTray::new(3, "PLA", "000000FF"),
            ],
        }
    }
}

/// A single filament tray in an AMS unit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilamentTray {
    pub id: u8,
    pub present: bool,
    pub filament_type: String,  // "PLA", "PETG", "ABS", etc.
    pub color: String,          // Hex RRGGBBAA, e.g., "FF5733FF"
    pub remaining_percent: u8,  // 0-100
    pub nozzle_temp_min: u16,
    pub nozzle_temp_max: u16,
    pub bed_temp: u16,
}

impl FilamentTray {
    /// Create a new filament tray
    pub fn new(id: u8, filament_type: &str, color: &str) -> Self {
        let (nozzle_min, nozzle_max, bed_temp) = match filament_type {
            "PLA" => (190, 230, 55),
            "PETG" => (220, 250, 70),
            "ABS" => (240, 270, 90),
            "TPU" => (210, 230, 45),
            _ => (200, 220, 60),
        };

        Self {
            id,
            present: true,
            filament_type: filament_type.to_string(),
            color: color.to_string(),
            remaining_percent: 85,
            nozzle_temp_min: nozzle_min,
            nozzle_temp_max: nozzle_max,
            bed_temp,
        }
    }

    /// Create an empty tray
    pub fn empty(id: u8) -> Self {
        Self {
            id,
            present: false,
            filament_type: String::new(),
            color: "000000FF".to_string(),
            remaining_percent: 0,
            nozzle_temp_min: 0,
            nozzle_temp_max: 0,
            bed_temp: 0,
        }
    }

    /// Consume filament (reduce remaining percentage)
    pub fn consume(&mut self, percent: u8) {
        self.remaining_percent = self.remaining_percent.saturating_sub(percent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_ams_state() {
        let ams = AmsState::new();
        assert_eq!(ams.units.len(), 1);
        assert_eq!(ams.units[0].id, 0);
        assert_eq!(ams.units[0].trays.len(), 4);
        assert_eq!(ams.current_tray, Some(0));
    }

    #[test]
    fn test_default_ams_state() {
        let ams = AmsState::default();
        assert_eq!(ams.units.len(), 1);
        assert_eq!(ams.ams_rfid_status, 1);
    }

    #[test]
    fn test_get_tray() {
        let ams = AmsState::new();
        let tray = ams.get_tray(0, 0).unwrap();
        assert_eq!(tray.id, 0);
        assert_eq!(tray.filament_type, "PLA");
    }

    #[test]
    fn test_get_tray_nonexistent() {
        let ams = AmsState::new();
        assert!(ams.get_tray(1, 0).is_none()); // Unit 1 doesn't exist
        assert!(ams.get_tray(0, 5).is_none()); // Tray 5 doesn't exist
    }

    #[test]
    fn test_get_tray_mut() {
        let mut ams = AmsState::new();
        if let Some(tray) = ams.get_tray_mut(0, 0) {
            tray.remaining_percent = 50;
        }
        assert_eq!(ams.get_tray(0, 0).unwrap().remaining_percent, 50);
    }

    #[test]
    fn test_new_ams_unit() {
        let unit = AmsUnit::new(0);
        assert_eq!(unit.id, 0);
        assert_eq!(unit.humidity, 3);
        assert_eq!(unit.temp, 24.5);
        assert_eq!(unit.trays.len(), 4);
    }

    #[test]
    fn test_filament_tray_pla() {
        let tray = FilamentTray::new(0, "PLA", "FF5733FF");
        assert_eq!(tray.id, 0);
        assert!(tray.present);
        assert_eq!(tray.filament_type, "PLA");
        assert_eq!(tray.color, "FF5733FF");
        assert_eq!(tray.nozzle_temp_min, 190);
        assert_eq!(tray.nozzle_temp_max, 230);
        assert_eq!(tray.bed_temp, 55);
        assert_eq!(tray.remaining_percent, 85);
    }

    #[test]
    fn test_filament_tray_petg() {
        let tray = FilamentTray::new(1, "PETG", "3498DBFF");
        assert_eq!(tray.filament_type, "PETG");
        assert_eq!(tray.nozzle_temp_min, 220);
        assert_eq!(tray.nozzle_temp_max, 250);
        assert_eq!(tray.bed_temp, 70);
    }

    #[test]
    fn test_filament_tray_abs() {
        let tray = FilamentTray::new(2, "ABS", "000000FF");
        assert_eq!(tray.filament_type, "ABS");
        assert_eq!(tray.nozzle_temp_min, 240);
        assert_eq!(tray.nozzle_temp_max, 270);
        assert_eq!(tray.bed_temp, 90);
    }

    #[test]
    fn test_empty_tray() {
        let tray = FilamentTray::empty(3);
        assert_eq!(tray.id, 3);
        assert!(!tray.present);
        assert_eq!(tray.remaining_percent, 0);
        assert_eq!(tray.filament_type, "");
    }

    #[test]
    fn test_consume_filament() {
        let mut tray = FilamentTray::new(0, "PLA", "FF5733FF");
        assert_eq!(tray.remaining_percent, 85);
        tray.consume(10);
        assert_eq!(tray.remaining_percent, 75);
        tray.consume(80);
        assert_eq!(tray.remaining_percent, 0); // Saturating sub
    }

    #[test]
    fn test_ams_state_serialization() {
        let ams = AmsState::new();
        let json = serde_json::to_string(&ams).unwrap();
        assert!(json.contains("units"));
        assert!(json.contains("current_tray"));
    }

    #[test]
    fn test_ams_state_round_trip() {
        let ams = AmsState::new();
        let json = serde_json::to_string(&ams).unwrap();
        let deserialized: AmsState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.units.len(), ams.units.len());
        assert_eq!(deserialized.current_tray, ams.current_tray);
    }
}
