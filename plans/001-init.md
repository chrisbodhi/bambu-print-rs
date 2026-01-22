# Bambu Lab Printer Emulator - Implementation Plan

## Overview

A Rust server that emulates a Bambu Lab X1 Carbon with 4-slot AMS, exposing MQTT and FTP interfaces compatible with the `bambulabs_api` Python library. Designed for testing an AI orchestration layer that manages multiple printers.

## Requirements Summary

| Aspect| Decision |
|---|---|
| Protocols | MQTT + FTP |
| Client Target | bambulabs_api Python library (local/LAN mode, no cloud) |
| Print Simulation | Configurable speed with realistic stages |
| AMS | Single 4-slot unit, simple implementation |
| State | Persist to file |
| Multi-printer | Multiple instances (later) |
| Auth | Optional, match what Python library expects |
| Failure Injection | Skip for now |

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   Bambu Emulator                        │
│                                                         │
│  ┌─────────────┐  ┌─────────────┐  ┌────────────────┐   │
│  │ MQTT Broker │  │ FTP Server  │  │ State Manager  │   │
│  │  (rumqttd)  │  │ (unftp)     │  │                │   │
│  └──────┬──────┘  └──────┬──────┘  └───────┬────────┘   │
│         │                │                 │            │
│         └────────────────┴─────────────────┘            │
│                          │                              │
│                 ┌────────▼────────┐                     │
│                 │  Print Engine   │                     │
│                 │  (Simulation)   │                     │
│                 └────────┬────────┘                     │
│                          │                              │
│                 ┌────────▼────────┐                     │
│                 │  Persistence    │                     │
│                 │  (JSON file)    │                     │
│                 └─────────────────┘                     │
└─────────────────────────────────────────────────────────┘
```

## Technology Choices

### MQTT Broker: `rumqttd`

Choice: rumqttd - Pure Rust MQTT broker

Why:

- Embeddable as a library (not just standalone binary)
- Supports MQTT 3.1.1 (what Bambu printers use)
- TLS support via rustls
- Active maintenance, good docs
- Can intercept/inject messages programmatically

Tradeoffs:

- Less battle-tested than Mosquitto
- May need to dig into internals for custom auth

Alternatives considered:

- mosquitto (C, would require FFI or running as separate process)
- emqx (Erlang, way overkill)
- Writing our own (unnecessary complexity)

### FTP Server: unftp

Choice: unftp - Async FTP server library

Why:

- Pure Rust, embeddable
- Pluggable storage backends (we'll use in-memory or filesystem)
- Supports passive mode (required for most clients)
- TLS support available

Tradeoffs:

- FTP is a cursed protocol; passive mode port ranges may complicate containerization
- May need custom storage backend to trigger print jobs on upload

Alternatives considered:

- firetrap (less maintained)
- Shelling out to vsftpd (adds deployment complexity)

### Async Runtime: tokio

Why: Both rumqttd and unftp are tokio-based. No real alternative here.

### Serialization: serde + serde_json

Why: The MQTT payloads are all JSON. Standard choice.

### State Persistence: JSON file with tokio::fs

Why:

- Human-readable for debugging
- Simple to implement
- Can version control test fixtures

Tradeoffs:

- Not suitable for high-frequency writes (we'll debounce/batch)
- No concurrent access safety (fine for single instance)

Alternatives considered:

- SQLite via rusqlite (overkill for single-printer state)
- sled embedded DB (adds dependency, harder to inspect)

### CLI Framework: clap

Why: Standard choice, derive macros make it easy.

### Logging: tracing + tracing-subscriber

Why: Structured logging, async-aware, good ecosystem.

## Data Model

### Printer State

```rust
#[derive(Serialize, Deserialize)]
pub struct PrinterState {
    // Identity
    pub serial_number: String,
    pub model: PrinterModel,  // X1Carbon, P1S, A1, etc.
    pub firmware_version: String,
    
    // Temperatures (Celsius)
    pub nozzle_temp: f32,
    pub nozzle_target_temp: f32,
    pub bed_temp: f32,
    pub bed_target_temp: f32,
    pub chamber_temp: f32,
    
    // Print state
    pub gcode_state: GcodeState,  // IDLE, PREPARE, RUNNING, PAUSE, FINISH, FAILED
    pub print_job: Option<PrintJob>,
    
    // Hardware
    pub ams: AmsState,
    pub lights_on: bool,
    pub door_open: bool,
    pub sd_card_present: bool,
    
    // Network
    pub wifi_signal: String,  // e.g., "-53dBm"
    
    // Fans (0-100 or 0-15 depending on field)
    pub heatbreak_fan_speed: u8,
    pub cooling_fan_speed: u8,
    pub aux_fan_speed: u8,
    pub chamber_fan_speed: u8,
    
    // Speed
    pub speed_level: SpeedLevel,  // Silent, Standard, Sport, Ludicrous
    pub speed_magnitude: u8,      // 50-166 (percentage)
}

#[derive(Serialize, Deserialize)]
pub struct PrintJob {
    pub task_id: String,
    pub subtask_name: String,
    pub gcode_file: String,
    pub plate_number: u8,
    
    // Progress
    pub mc_percent: u8,           // 0-100
    pub layer_num: u32,
    pub total_layer_num: u32,
    pub mc_remaining_time: u32,   // minutes
    
    // Timing
    pub start_time: DateTime<Utc>,
    pub estimated_total_time: Duration,
    
    // Simulation bookkeeping
    pub current_stage: PrintStage,
    pub stage_started_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
pub enum PrintStage {
    Heating { target_nozzle: f32, target_bed: f32 },
    BedLeveling,
    Purging,
    Printing { layer: u32, total: u32 },
    Cooling,
    Complete,
}

#[derive(Serialize, Deserialize)]
pub struct AmsState {
    pub units: Vec<AmsUnit>,  // Typically just 1 for now
    pub current_tray: Option<u8>,
    pub ams_status: u8,
    pub ams_rfid_status: u8,
}

#[derive(Serialize, Deserialize)]
pub struct AmsUnit {
    pub id: u8,
    pub humidity: u8,        // 0-5 scale
    pub temp: f32,
    pub trays: [FilamentTray; 4],
}

#[derive(Serialize, Deserialize)]
pub struct FilamentTray {
    pub id: u8,
    pub present: bool,
    pub filament_type: String,      // "PLA", "PETG", "ABS", etc.
    pub color: String,              // Hex RRGGBBAA, e.g., "FF5733FF"
    pub remaining_percent: u8,      // 0-100 (derived from weight)
    pub nozzle_temp_min: u16,
    pub nozzle_temp_max: u16,
    pub bed_temp: u16,
}

#[derive(Serialize, Deserialize)]
pub enum GcodeState {
    Idle,
    Prepare,
    Running,
    Pause,
    Finish,
    Failed,
}

#[derive(Serialize, Deserialize)]
pub enum SpeedLevel {
    Silent = 1,
    Standard = 2,
    Sport = 3,
    Ludicrous = 4,
}
```

### Configuration

```rust
#[derive(Serialize, Deserialize)]
pub struct EmulatorConfig {
    // Server settings
    pub mqtt_port: u16,           // Default: 8883 (TLS) or 1883 (plain)
    pub mqtt_tls_enabled: bool,
    pub ftp_port: u16,            // Default: 21
    pub ftp_passive_ports: Range<u16>,  // Default: 50000-50100
    
    // Auth
    pub access_code: Option<String>,  // None = no auth required
    
    // Simulation
    pub time_multiplier: f32,     // Default: 100.0 (100x speed)
    pub status_interval_ms: u64,  // Default: 1000 (real printers ~1/sec)
    
    // Persistence
    pub state_file: PathBuf,
    pub auto_save_interval_secs: u64,
    
    // Initial state
    pub initial_state: Option<PrinterState>,
}
```

## MQTT Topic Structure

Based on `bambulabs_api` expectations:

### Topics

| Topic | Direction | Purpose |
|---|---|---|
| `device/{serial}/report` | Emulator → Client | Status updates (published every ~1 sec) |
| `device/{serial}/request` | Client → Emulator | Commands from client |

### Report Message Format

The emulator publishes a JSON blob matching what real printers send:

```json
{
  "print": {
    "nozzle_temper": 205.5,
    "nozzle_target_temper": 210,
    "bed_temper": 55.0,
    "bed_target_temper": 55,
    "chamber_temper": 35,
    "gcode_state": "RUNNING",
    "mc_percent": 45,
    "mc_remaining_time": 23,
    "layer_num": 42,
    "total_layer_num": 150,
    "subtask_name": "benchy.3mf",
    "gcode_file": "/cache/benchy.gcode",
    "wifi_signal": "-48dBm",
    "spd_lvl": 2,
    "spd_mag": 100,
    "heatbreak_fan_speed": "7",
    "cooling_fan_speed": "15",
    "big_fan1_speed": "0",
    "big_fan2_speed": "0",
    "stg_cur": 2,
    "lights_report": [{"node": "chamber_light", "mode": "on"}],
    "ams": {
      "ams": [{
        "id": "0",
        "humidity": "3",
        "temp": "24.5",
        "tray": [
          {"id": "0", "tray_type": "PLA", "tray_color": "FF5733FF", "remain": 85, ...},
          {"id": "1", "tray_type": "PLA", "tray_color": "3498DBFF", "remain": 72, ...},
          {"id": "2", "tray_type": "PETG", "tray_color": "2ECC71FF", "remain": 100, ...},
          {"id": "3", "tray_type": "PLA", "tray_color": "000000FF", "remain": 45, ...}
        ]
      }],
      "ams_exist_bits": "1",
      "tray_exist_bits": "f",
      "tray_now": "0",
      "version": 3
    },
    "online": {"ahb": false, "rfid": true, "version": 689131425},
    "upload": {"status": "idle", "progress": 0, "message": ""},
    "hms": [],
    "sdcard": true
  }
}
```

### Request Message Format

Commands from client:

```json
// Start print
{
  "print": {
    "sequence_id": "123",
    "command": "project_file",
    "param": "Metadata/plate_1.gcode",
    "url": "ftp:///model.3mf",
    "subtask_name": "benchy",
    "use_ams": true,
    "ams_mapping": [0, 1, 2, 3],
    "timelapse": true,
    "bed_levelling": true,
    "flow_cali": true,
    "vibration_cali": true,
    "layer_inspect": true
  }
}

// Pause
{"print": {"sequence_id": "124", "command": "pause"}}

// Resume
{"print": {"sequence_id": "125", "command": "resume"}}

// Stop
{"print": {"sequence_id": "126", "command": "stop"}}

// Set temperature
{"print": {"sequence_id": "127", "command": "gcode_line", "param": "M104 S210\n"}}

// Toggle light
{"system": {"sequence_id": "128", "command": "ledctrl", "led_node": "chamber_light", "led_mode": "on"}}

// Push all (request full status)
{"pushing": {"sequence_id": "129", "command": "pushall"}}
```

### Print Simulation Engine

Stage Durations (at 1x speed)

| Stage | Real Duration | Notes |
|---|---|---|
| Heating (nozzle) | 30-90 sec | Depends on target temp |
| Heating (bed) | 60-180 sec | Parallel with nozzle |
| Bed leveling | 60 sec | If enabled |
| Purging | 15 sec | Nozzle prime |
| Printing | Variable | Based on layer count & estimated time |
| Cooling | 30 sec | Before marking complete |

### Simulation Loop

```rust
async fn simulation_tick(&mut self, elapsed: Duration) {
    let simulated_elapsed = elapsed * self.config.time_multiplier;
    
    match &mut self.state.print_job {
        None => return,
        Some(job) => {
            match &job.current_stage {
                PrintStage::Heating { target_nozzle, target_bed } => {
                    // Ramp temperatures toward targets
                    self.state.nozzle_temp = approach(
                        self.state.nozzle_temp,
                        *target_nozzle,
                        NOZZLE_HEAT_RATE * simulated_elapsed.as_secs_f32()
                    );
                    self.state.bed_temp = approach(
                        self.state.bed_temp,
                        *target_bed,
                        BED_HEAT_RATE * simulated_elapsed.as_secs_f32()
                    );
                    
                    // Transition when both at temp
                    if temps_reached() {
                        job.current_stage = PrintStage::BedLeveling;
                    }
                }
                PrintStage::Printing { layer, total } => {
                    // Advance layers based on time
                    let time_per_layer = job.estimated_total_time / *total;
                    let layers_to_advance = (simulated_elapsed / time_per_layer) as u32;
                    
                    job.layer_num = (job.layer_num + layers_to_advance).min(*total);
                    job.mc_percent = ((job.layer_num as f32 / *total as f32) * 100.0) as u8;
                    
                    // Consume filament
                    self.consume_filament(layers_to_advance);
                    
                    if job.layer_num >= *total {
                        job.current_stage = PrintStage::Cooling;
                    }
                }
                // ... other stages
            }
        }
    }
}
```

### Temperature Modeling

Simple linear approach model:

- Nozzle heats at ~3°C/sec, cools at ~1°C/sec (with fan)
- Bed heats at ~1°C/sec, cools at ~0.3°C/sec
- Chamber rises slowly during printing (~0.1°C/sec), ambient ~25°C


## FTP Server Integration

Purpose

The bambulabs_api library uploads .3mf files via FTP before sending the print command. We need to:

1. Accept file uploads to a virtual filesystem
2. Store files in memory or a temp directory
3. Trigger state updates when files are uploaded

### Implementation

```rust
struct EmulatorStorageBackend {
    files: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    state_manager: Arc<StateManager>,
}

#[async_trait]
impl StorageBackend<EmulatorUser> for EmulatorStorageBackend {
    async fn put(&self, user: &EmulatorUser, path: &str, input: impl AsyncRead) -> Result<u64> {
        let data = read_to_vec(input).await?;
        let size = data.len() as u64;
        
        self.files.write().await.insert(path.to_string(), data);
        
        // Notify state manager that a file is ready
        self.state_manager.file_uploaded(path).await;
        
        Ok(size)
    }
    
    // ... get, list, etc.
}
```

### File Paths

Real printers expose:
- `/` - Root with cache, model, timelapse directories
- `/cache/` - Uploaded print files land here
- `/model/` - Sliced models
- `/timelapse/` - Video files

We'll emulate `/cache/` only initially.

---

## Module Structure

```
bambu-print-rs/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library root (for embedding)
│   ├── config.rs            # Configuration types
│   ├── state/
│   │   ├── mod.rs
│   │   ├── printer.rs       # PrinterState, PrintJob, etc.
│   │   ├── ams.rs           # AMS types
│   │   └── persistence.rs   # JSON file load/save
│   ├── mqtt/
│   │   ├── mod.rs
│   │   ├── broker.rs        # rumqttd wrapper
│   │   ├── messages.rs      # JSON message types
│   │   └── handler.rs       # Command processing
│   ├── ftp/
│   │   ├── mod.rs
│   │   └── backend.rs       # unftp storage backend
│   ├── simulation/
│   │   ├── mod.rs
│   │   ├── engine.rs        # Main simulation loop
│   │   ├── temperature.rs   # Thermal modeling
│   │   └── print_stages.rs  # Stage transitions
│   └── util/
│       ├── mod.rs
│       └── time.rs          # Scaled time utilities
├── tests/
│   ├── integration/
│   │   ├── mqtt_test.rs
│   │   ├── ftp_test.rs
│   │   └── print_flow_test.rs
│   └── fixtures/
│       └── sample_state.json
└── examples/
    └── basic_usage.rs
```

---

## CLI Interface

```sh
bambu-print-rs 0.1.0
Emulates a Bambu Lab 3D printer for testing

USAGE:
    bambu-print-rs [OPTIONS]

OPTIONS:
    -c, --config <FILE>           Config file path [default: emulator.toml]
    -s, --serial <SERIAL>         Printer serial number [default: "00M00A000000001"]
    -p, --mqtt-port <PORT>        MQTT port [default: 1883]
        --mqtt-tls                Enable TLS on MQTT (uses port 8883)
        --ftp-port <PORT>         FTP port [default: 21]
    -a, --access-code <CODE>      Access code for auth (omit for no auth)
    -t, --time-multiplier <N>     Simulation speed [default: 100]
        --state-file <FILE>       State persistence file [default: state.json]
    -v, --verbose                 Increase logging verbosity
    -h, --help                    Print help
    -V, --version                 Print version
```

## Testing Strategy

### Unit Tests

- State serialization/deserialization
- Temperature modeling math
- Print stage transitions
- Message parsing

### Integration Tests

Use bambulabs_api Python library as the client:

```python
# test_emulator_integration.py
import bambulabs_api as bl
import time

def test_connect_and_get_status():
    printer = bl.Printer("127.0.0.1", "12345678", "00M00A000000001")
    printer.connect()
    time.sleep(1)
    
    status = printer.get_state()
    assert status == bl.PrinterState.IDLE
    
    printer.disconnect()

def test_print_flow():
    printer = bl.Printer("127.0.0.1", "12345678", "00M00A000000001")
    printer.connect()
    
    # Upload and print
    printer.upload_and_print("test_model.3mf", plate=1)
    
    # Wait for completion (emulator runs at 100x)
    time.sleep(30)
    
    status = printer.get_state()
    assert status == bl.PrinterState.IDLE
    
    printer.disconnect()
```

### Test Fixtures

Provide JSON state files for various scenarios:

- idle_state.json - Fresh printer, no job
- mid_print_state.json - 50% through a print
- low_filament_state.json - Slot 0 nearly empty
- paused_state.json - Print paused by user


## Implementation Phases

Phase 1: Core MQTT (MVP)

- [ ] Basic project scaffolding
- [ ] rumqttd embedded broker running
- [ ] State types defined
- [ ] Status publishing loop (every 1 sec)
- [ ] Handle pushall command
- [ ] CLI with basic options

Deliverable: Can connect with bambulabs_api, see IDLE status

Phase 2: Print Simulation

- [ ] Command handler for print/pause/resume/stop
- [ ] Print stage state machine
- [ ] Temperature simulation
- [ ] Layer progress simulation
- [ ] Filament consumption tracking

Deliverable: Can start a print and watch it progress to completion

Phase 3: FTP Integration

- [ ] unftp embedded server
- [ ] In-memory file storage
- [ ] Wire up file upload to trigger print availability
- [ ] Full upload_and_print flow works

Deliverable: Full print flow via bambulabs_api works

Phase 4: Persistence & Polish

- [ ] JSON state persistence
- [ ] Graceful shutdown with state save
- [ ] State restore on startup
- [ ] Better logging
- [ ] Configuration file support

Deliverable: Production-ready single-instance emulator

---

Open Questions / Future Work

1. 3MF Parsing: Should we actually parse uploaded .3mf files to extract layer count, estimated time, etc.? Or just use dummy values? (Recommend: dummy values for now, add parsing if needed)
2. G-code Commands: How faithfully should we handle arbitrary G-code via gcode_line? Real printers execute these. (Recommend: parse temp commands M104/M109/M140/M190, ignore others)
3. HMS Error Codes: When we add failure injection, we'll need to research the actual HMS error code format. (Defer until failure injection phase)
4. Camera Placeholder: Should we return a static image or 404 for camera endpoints? (Recommend: ignore for now)
5. Multi-printer: The architecture should make it easy to run multiple PrinterState instances, but the MQTT topic routing needs thought. (Defer to later phase)

---

## Dependencies Summary

```toml
[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# MQTT
rumqttd = "0.19"
rumqttc = "0.24"  # For internal client if needed

# FTP
unftp = "0.20"
unftp-sbe-fs = "0.2"  # Or custom backend

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# CLI
clap = { version = "4", features = ["derive"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Time
chrono = { version = "0.4", features = ["serde"] }

# TLS (optional)
rustls = "0.21"
tokio-rustls = "0.24"

# Utilities
thiserror = "1"
anyhow = "1"

[dev-dependencies]
# For Python integration tests
pyo3 = "0.20"
```
