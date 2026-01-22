//! Configuration types for the emulator

use serde::{Deserialize, Serialize};

/// Emulator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmulatorConfig {
    // Server settings
    pub mqtt_port: u16,
    pub mqtt_tls_enabled: bool,

    // Auth
    pub access_code: Option<String>,

    // Simulation
    pub status_interval_ms: u64,

    // Identity
    pub serial_number: String,
}

impl EmulatorConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self {
            mqtt_port: 1883,
            mqtt_tls_enabled: false,
            access_code: None,
            status_interval_ms: 1000,
            serial_number: "00M00A000000001".to_string(),
        }
    }

    /// Create a configuration with a custom serial number
    pub fn with_serial(mut self, serial: String) -> Self {
        self.serial_number = serial;
        self
    }

    /// Set MQTT port
    pub fn with_mqtt_port(mut self, port: u16) -> Self {
        self.mqtt_port = port;
        self
    }

    /// Set access code
    pub fn with_access_code(mut self, code: String) -> Self {
        self.access_code = Some(code);
        self
    }

    /// Set status interval
    pub fn with_status_interval(mut self, interval_ms: u64) -> Self {
        self.status_interval_ms = interval_ms;
        self
    }

    /// Enable TLS
    pub fn with_tls(mut self, enabled: bool) -> Self {
        self.mqtt_tls_enabled = enabled;
        if enabled && self.mqtt_port == 1883 {
            self.mqtt_port = 8883;
        }
        self
    }
}

impl Default for EmulatorConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EmulatorConfig::new();
        assert_eq!(config.mqtt_port, 1883);
        assert!(!config.mqtt_tls_enabled);
        assert_eq!(config.status_interval_ms, 1000);
        assert_eq!(config.serial_number, "00M00A000000001");
        assert!(config.access_code.is_none());
    }

    #[test]
    fn test_with_serial() {
        let config = EmulatorConfig::new().with_serial("CUSTOM123".to_string());
        assert_eq!(config.serial_number, "CUSTOM123");
    }

    #[test]
    fn test_with_mqtt_port() {
        let config = EmulatorConfig::new().with_mqtt_port(8883);
        assert_eq!(config.mqtt_port, 8883);
    }

    #[test]
    fn test_with_access_code() {
        let config = EmulatorConfig::new().with_access_code("secret123".to_string());
        assert_eq!(config.access_code, Some("secret123".to_string()));
    }

    #[test]
    fn test_with_status_interval() {
        let config = EmulatorConfig::new().with_status_interval(500);
        assert_eq!(config.status_interval_ms, 500);
    }

    #[test]
    fn test_with_tls() {
        let config = EmulatorConfig::new().with_tls(true);
        assert!(config.mqtt_tls_enabled);
        assert_eq!(config.mqtt_port, 8883); // Auto-switches to TLS port
    }

    #[test]
    fn test_builder_pattern() {
        let config = EmulatorConfig::new()
            .with_serial("TEST456".to_string())
            .with_mqtt_port(1234)
            .with_access_code("pass".to_string())
            .with_status_interval(2000);

        assert_eq!(config.serial_number, "TEST456");
        assert_eq!(config.mqtt_port, 1234);
        assert_eq!(config.access_code, Some("pass".to_string()));
        assert_eq!(config.status_interval_ms, 2000);
    }

    #[test]
    fn test_config_serialization() {
        let config = EmulatorConfig::new();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("mqtt_port"));
        assert!(json.contains("serial_number"));
    }

    #[test]
    fn test_config_deserialization() {
        let json = r#"{
            "mqtt_port": 1883,
            "mqtt_tls_enabled": false,
            "access_code": null,
            "status_interval_ms": 1000,
            "serial_number": "00M00A000000001"
        }"#;
        let config: EmulatorConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mqtt_port, 1883);
        assert_eq!(config.serial_number, "00M00A000000001");
    }
}
