//! MQTT broker management using rumqttd

use anyhow::Result;
use rumqttd::{Broker, Config};
use std::collections::HashMap;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Start the embedded MQTT broker
pub async fn start_broker(port: u16) -> Result<JoinHandle<()>> {
    info!("Starting MQTT broker on port {}", port);

    let config = create_broker_config(port);
    let mut broker = Broker::new(config);

    let handle = tokio::spawn(async move {
        match broker.start() {
            Ok(_) => info!("MQTT broker started successfully"),
            Err(e) => error!("MQTT broker error: {}", e),
        }
    });

    // Give the broker a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(handle)
}

/// Create broker configuration
fn create_broker_config(port: u16) -> Config {
    let mut config = Config::default();

    // Configure router to allow multiple connections
    config.router.max_connections = 10; // Allow publisher, handler, and external clients
    config.router.max_segment_size = 256 * 1024; // 256KB segments
    config.router.max_segment_count = 10; // Keep 10 segments in memory

    // Configure server settings
    let listen_addr = format!("0.0.0.0:{}", port).parse().unwrap();
    let server = rumqttd::ServerSettings {
        name: "main".to_string(),
        listen: listen_addr,
        tls: None,
        next_connection_delay_ms: 1,
        connections: rumqttd::ConnectionSettings {
            connection_timeout_ms: 60000,
            max_payload_size: 256 * 1024, // 256KB
            max_inflight_count: 100,
            auth: None,
            external_auth: None,
            dynamic_filters: false,
        },
    };

    let mut servers = HashMap::new();
    servers.insert("main".to_string(), server);
    config.v4 = Some(servers);

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn test_create_broker_config() {
        let config = create_broker_config(1883);
        assert!(config.v4.is_some());

        let v4_config = config.v4.unwrap();
        let server = v4_config.get("main").unwrap();
        let expected: SocketAddr = "0.0.0.0:1883".parse().unwrap();
        assert_eq!(server.listen, expected);
    }

    #[test]
    fn test_create_broker_config_custom_port() {
        let config = create_broker_config(8883);
        let v4_config = config.v4.unwrap();
        let server = v4_config.get("main").unwrap();
        let expected: SocketAddr = "0.0.0.0:8883".parse().unwrap();
        assert_eq!(server.listen, expected);
    }

    #[test]
    fn test_connection_settings() {
        let config = create_broker_config(1883);
        let v4_config = config.v4.unwrap();
        let server = v4_config.get("main").unwrap();
        assert_eq!(server.connections.max_payload_size, 256 * 1024);
        assert_eq!(server.connections.max_inflight_count, 100);
    }

    #[test]
    fn test_router_max_connections() {
        let config = create_broker_config(1883);
        assert_eq!(config.router.max_connections, 10);
    }

    #[test]
    fn test_router_segment_settings() {
        let config = create_broker_config(1883);
        assert_eq!(config.router.max_segment_size, 256 * 1024);
        assert_eq!(config.router.max_segment_count, 10);
    }
}
