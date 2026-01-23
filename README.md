# bambu-print-rs

A Rust server that emulates a Bambu Lab X1 Carbon 3D printer with 4-slot AMS, exposing MQTT and FTP interfaces compatible with the `bambulabs_api` Python library. Designed for testing AI orchestration layers managing multiple printers.

## Features

- **Embedded MQTT Broker**: No external dependencies - runs a complete MQTT broker using rumqttd
- **Real Protocol Compatibility**: Matches the actual Bambu Lab MQTT message format
- **Configurable**: Customize serial number, ports, and update intervals
- **Stateful Simulation**: Maintains printer state including temperatures, AMS, and print status
- **Comprehensive Testing**: 49+ unit tests with sans-IO architecture for reliability

## Current Status: Phase 1 (MVP)

✅ **Implemented:**
- MQTT broker with bidirectional communication
- Status publishing (1 second intervals)
- Basic command handling:
  - `pushall`: Request immediate status update
  - `ledctrl`: Toggle chamber light
- Full state representation (temperatures, AMS, hardware status)
- CLI with customizable configuration

🚧 **Coming in Phase 2:**
- Print simulation engine (heating, printing, cooling stages)
- FTP server for file uploads
- Full print workflow (start, pause, resume, stop)

## Prerequisites

- Rust 1.74+ (tested with 1.92)
- Cargo (comes with Rust)

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/yourusername/bambu-print-rs.git
cd bambu-print-rs

# Build the project
cargo build --release

# The binary will be at target/release/bambu-print-rs
```

### Quick Start (Development)

```bash
# Run directly with cargo
cargo run
```

## Usage

### Basic Usage

Start the emulator with default settings:

```bash
cargo run
```

This starts:
- MQTT broker on port 1883
- Serial number: `00M00A000000001`
- Status updates every 1 second

### Command Line Options

```bash
# View all options
cargo run -- --help

# Custom serial number
cargo run -- --serial "ABC123456789"

# Custom MQTT port
cargo run -- --mqtt-port 8883

# Enable verbose logging
cargo run -- --verbose

# Combine multiple options
cargo run -- --serial "PRINTER001" --mqtt-port 1234 --status-interval 500 --verbose
```

### Available Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--serial` | `-s` | `00M00A000000001` | Printer serial number |
| `--mqtt-port` | `-p` | `1883` | MQTT broker port |
| `--mqtt-tls` | | `false` | Enable TLS (switches to port 8883) |
| `--access-code` | `-a` | None | Access code for authentication |
| `--status-interval` | | `1000` | Status update interval (ms) |
| `--verbose` | `-v` | `false` | Enable debug logging |

### Testing with MQTT Client

You can test the emulator using any MQTT client. Here's an example using `mosquitto_sub`:

```bash
# In terminal 1: Start the emulator
cargo run -- --verbose

# In terminal 2: Subscribe to status updates
mosquitto_sub -h localhost -p 1883 -t "device/00M00A000000001/report" -v

# In terminal 3: Send a pushall command
mosquitto_pub -h localhost -p 1883 -t "device/00M00A000000001/request" \
  -m '{"pushing":{"sequence_id":"1","command":"pushall"}}'

# Toggle the chamber light
mosquitto_pub -h localhost -p 1883 -t "device/00M00A000000001/request" \
  -m '{"system":{"sequence_id":"2","command":"ledctrl","led_node":"chamber_light","led_mode":"on"}}'
```

### Testing with Python bambulabs_api

```python
import bambulabs_api as bl
import time

# Connect to the emulator
printer = bl.Printer("127.0.0.1", "access_code", "00M00A000000001")
printer.connect()

# Wait for initial status
time.sleep(2)

# Get current state
status = printer.get_state()
print(f"Printer state: {status}")

# Check temperatures
temps = printer.get_temperature()
print(f"Nozzle: {temps['nozzle']}°C, Bed: {temps['bed']}°C")

# Disconnect
printer.disconnect()
```

## MQTT Protocol

### Status Updates (Emulator → Client)

Published to `device/{serial}/report` every ~1 second:

```json
{
  "print": {
    "nozzle_temper": 25.0,
    "nozzle_target_temper": 0.0,
    "bed_temper": 25.0,
    "bed_target_temper": 0.0,
    "chamber_temper": 25.0,
    "gcode_state": "IDLE",
    "wifi_signal": "-48dBm",
    "spd_lvl": 2,
    "spd_mag": 100,
    "lights_report": [{"node": "chamber_light", "mode": "off"}],
    "ams": {
      "ams": [{
        "id": "0",
        "humidity": "3",
        "temp": "24.5",
        "tray": [
          {"id": "0", "tray_type": "PLA", "tray_color": "FF5733FF", "remain": 85},
          {"id": "1", "tray_type": "PLA", "tray_color": "3498DBFF", "remain": 85},
          {"id": "2", "tray_type": "PETG", "tray_color": "2ECC71FF", "remain": 85},
          {"id": "3", "tray_type": "PLA", "tray_color": "000000FF", "remain": 85}
        ]
      }],
      "ams_exist_bits": "1",
      "tray_exist_bits": "f",
      "tray_now": "0"
    }
  }
}
```

### Commands (Client → Emulator)

Send to `device/{serial}/request`:

**Request Status Update:**
```json
{"pushing":{"sequence_id":"1","command":"pushall"}}
```

**Toggle Light On:**
```json
{"system":{"sequence_id":"2","command":"ledctrl","led_node":"chamber_light","led_mode":"on"}}
```

**Toggle Light Off:**
```json
{"system":{"sequence_id":"3","command":"ledctrl","led_node":"chamber_light","led_mode":"off"}}
```

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_gcode_state_serialization

# Run tests with verbose logging
RUST_LOG=debug cargo test
```

### Building for Release

```bash
cargo build --release
```

The optimized binary will be at `target/release/bambu-print-rs`.

### Project Structure

```
src/
├── lib.rs                  # Library root for embedding
├── main.rs                 # CLI entry point
├── config.rs               # Configuration types
├── state/
│   ├── mod.rs
│   ├── printer.rs          # PrinterState, GcodeState, etc.
│   └── ams.rs              # AMS and filament types
└── mqtt/
    ├── mod.rs
    ├── broker.rs           # rumqttd broker wrapper
    ├── messages.rs         # MQTT message types
    ├── publisher.rs        # Status publishing loop
    └── handler.rs          # Command processing
```

## Troubleshooting

### Port Already in Use

If you see an error about port 1883 being in use:

```bash
# Use a different port
cargo run -- --mqtt-port 11883
```

### Connection Refused

Make sure the emulator is running before connecting clients:

```bash
# Start with verbose logging to see what's happening
cargo run -- --verbose
```

### No Status Updates

Check that you're subscribing to the correct topic with your serial number:

```bash
# If using custom serial "ABC123"
mosquitto_sub -h localhost -p 1883 -t "device/ABC123/report"
```

## Contributing

Contributions are welcome! Please see [CLAUDE.md](CLAUDE.md) for development guidelines and architecture details.

### Running the Full Test Suite

```bash
# Unit tests
cargo test

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy -- -D warnings
```

## Roadmap

- [x] **Phase 1**: Core MQTT functionality (status publishing, basic commands)
- [ ] **Phase 2**: Print simulation engine (heating, printing, progress tracking)
- [ ] **Phase 3**: FTP server integration (file upload support)
- [ ] **Phase 4**: State persistence and configuration files

See [plans/001-init.md](plans/001-init.md) for detailed implementation plan.

## License

[License TBD]

## Acknowledgments

- Built with [rumqttd](https://github.com/bytebeamio/rumqtt) - Embedded MQTT broker
- Designed for compatibility with [bambulabs_api](https://github.com/greghesp/bambulabs_api) Python library
