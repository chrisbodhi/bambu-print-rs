# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Rust server that emulates a Bambu Lab X1 Carbon 3D printer with 4-slot AMS, exposing MQTT and FTP interfaces compatible with the `bambulabs_api` Python library. Designed for testing AI orchestration layers managing multiple printers.

## Build Commands

```bash
# Build the project
cargo build

# Build in release mode
cargo build --release

# Run the emulator
cargo run

# Run with custom options
cargo run -- --mqtt-port 1883 --ftp-port 21 --time-multiplier 100

# Run tests
cargo test

# Run specific test
cargo test <test_name>

# Run with verbose logging
cargo run -- --verbose
```

## Architecture

### Core Components

The emulator consists of four main subsystems that work together:

1. **MQTT Broker** (rumqttd): Embedded broker handling bidirectional communication
   - Publishes status updates to `device/{serial}/report` every ~1 second
   - Listens for commands on `device/{serial}/request`
   - All messages are JSON formatted

2. **FTP Server** (unftp): Handles .3mf file uploads
   - Files uploaded to `/cache/` directory
   - Custom storage backend triggers state updates on file upload
   - Required for full print workflow

3. **Print Simulation Engine**: Multi-stage print simulation
   - Stages: Heating → Bed Leveling → Purging → Printing → Cooling → Complete
   - Configurable time multiplier (default 100x real speed)
   - Simulates temperature ramping, layer progress, filament consumption

4. **State Manager**: Centralized state with JSON persistence
   - PrinterState: temperatures, print job, hardware status
   - AmsState: 4-slot filament management
   - Auto-saves to `state.json` at intervals

### Data Flow

```
Client (bambulabs_api)
    ↓ FTP upload .3mf
FTP Server → State Manager (file ready)
    ↓ MQTT command (project_file)
MQTT Handler → Print Engine (start job)
    ↓ simulation tick
Print Engine → State Manager → MQTT Broker
    ↓ publish status
Client receives updates
```

## Module Organization

```
src/
├── main.rs              # CLI entry point with clap
├── lib.rs               # Library root for embedding
├── config.rs            # EmulatorConfig types
├── state/
│   ├── printer.rs       # PrinterState, PrintJob, GcodeState
│   ├── ams.rs           # AMS and filament tray types
│   └── persistence.rs   # JSON serialization/deserialization
├── mqtt/
│   ├── broker.rs        # rumqttd wrapper and lifecycle
│   ├── messages.rs      # MQTT JSON message schemas
│   └── handler.rs       # Command processing (print, pause, resume, stop)
├── ftp/
│   └── backend.rs       # unftp StorageBackend implementation
├── simulation/
│   ├── engine.rs        # Main tick loop and state machine
│   ├── temperature.rs   # Thermal modeling (heating/cooling rates)
│   └── print_stages.rs  # PrintStage enum and transitions
└── util/
    └── time.rs          # Time multiplier utilities
```

## MQTT Protocol

### Status Updates (Emulator → Client)

Published to `device/{serial}/report` with comprehensive printer state:
- Temperatures (nozzle, bed, chamber) - current and target
- Print progress (layer, percentage, remaining time)
- AMS state (4 trays with filament type, color, remaining %)
- Hardware status (lights, door, fans, wifi)

### Commands (Client → Emulator)

Sent to `device/{serial}/request`:
- `project_file`: Start print job
- `pause`: Pause active print
- `resume`: Resume paused print
- `stop`: Cancel print
- `gcode_line`: Execute G-code (primarily M104/M109/M140/M190 for temps)
- `ledctrl`: Toggle chamber light
- `pushall`: Request immediate status update

All commands include `sequence_id` for tracking.

## Print Simulation Details

### Stage Timings (at 1x speed)

- Heating: 30-180s (nozzle + bed in parallel)
- Bed Leveling: 60s (if enabled)
- Purging: 15s
- Printing: Variable based on layer count
- Cooling: 30s

### Temperature Modeling

Simple linear approach:
- Nozzle: heats ~3°C/sec, cools ~1°C/sec
- Bed: heats ~1°C/sec, cools ~0.3°C/sec
- Chamber: rises ~0.1°C/sec during print

### Time Multiplier

Default 100x speedup applies to all stages. A 2-hour real print completes in ~72 seconds.

## Testing

### Integration Testing Strategy

Use `bambulabs_api` Python library as the test client:

```python
import bambulabs_api as bl
printer = bl.Printer("127.0.0.1", "access_code", "serial_number")
printer.connect()
printer.upload_and_print("model.3mf", plate=1)
# Monitor status updates
printer.disconnect()
```

### Test Fixtures

Store JSON state files in `tests/fixtures/` for various scenarios:
- `idle_state.json` - Fresh printer
- `mid_print_state.json` - 50% progress
- `low_filament_state.json` - Nearly empty slot
- `paused_state.json` - Paused job

## Development Notes

### Adding New Commands

1. Define message struct in `mqtt/messages.rs` with serde
2. Add handler logic in `mqtt/handler.rs`
3. Update state in `state/printer.rs`
4. Ensure status updates reflect changes

### State Persistence

- Auto-saves at configured intervals
- Manually saved on graceful shutdown (SIGTERM/SIGINT)
- Restored from `state.json` on startup if present
- Human-readable JSON for debugging

### Logging

Uses `tracing` framework with structured logging:
- Default: INFO level
- Enable DEBUG/TRACE with `--verbose` or `RUST_LOG` env var
- Logs include MQTT messages, state transitions, FTP operations

## Implementation Phases

Current status tracked in `plans/001-init.md`. Ensure this stays up to date:

1. **Phase 1**: Core MQTT (status publishing, basic commands)
2. **Phase 2**: Print simulation engine
3. **Phase 3**: FTP integration
4. **Phase 4**: Persistence and configuration

## Configuration

CLI options override defaults:
- `--mqtt-port`: MQTT broker port (default: 1883)
- `--ftp-port`: FTP server port (default: 21)
- `--access-code`: Authentication code (optional)
- `--time-multiplier`: Simulation speed (default: 100)
- `--state-file`: Persistence file path (default: state.json)
- `--serial`: Printer serial number

Future: TOML config file support via `--config`.
