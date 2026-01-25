//! Print simulation engine
//!
//! Handles the main simulation loop that advances print state based on time.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;
use tracing::{debug, info};

use crate::state::{GcodeState, PrintStage, PrinterState};

use super::temperature::{
    approach_temperature, temps_reached, TemperatureRates, AMBIENT_TEMP, TEMP_TOLERANCE,
};

/// Stage durations at 1x speed (in seconds)
pub struct StageDurations {
    pub bed_leveling_secs: f32,
    pub purging_secs: f32,
    pub cooling_secs: f32,
}

impl Default for StageDurations {
    fn default() -> Self {
        Self {
            bed_leveling_secs: 60.0,
            purging_secs: 15.0,
            cooling_secs: 30.0,
        }
    }
}

/// Simulation engine configuration
pub struct SimulationConfig {
    pub time_multiplier: f32,
    pub tick_interval: Duration,
    pub temp_rates: TemperatureRates,
    pub stage_durations: StageDurations,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            time_multiplier: 100.0, // 100x speed by default
            tick_interval: Duration::from_millis(100), // Tick every 100ms
            temp_rates: TemperatureRates::default(),
            stage_durations: StageDurations::default(),
        }
    }
}

impl SimulationConfig {
    pub fn new(time_multiplier: f32) -> Self {
        Self {
            time_multiplier,
            ..Default::default()
        }
    }
}

/// Run the simulation engine
pub async fn run_simulation_engine(
    state: Arc<RwLock<PrinterState>>,
    config: SimulationConfig,
) -> anyhow::Result<()> {
    info!(
        "Starting simulation engine with {}x time multiplier",
        config.time_multiplier
    );

    let mut last_tick = Instant::now();

    loop {
        tokio::time::sleep(config.tick_interval).await;

        let now = Instant::now();
        let real_elapsed = now.duration_since(last_tick);
        last_tick = now;

        // Apply time multiplier
        let simulated_elapsed_secs = real_elapsed.as_secs_f32() * config.time_multiplier;

        // Run simulation tick
        {
            let mut state_guard = state.write().await;
            simulation_tick(&mut state_guard, simulated_elapsed_secs, &config);
        }
    }
}

/// Process a single simulation tick
fn simulation_tick(state: &mut PrinterState, delta_secs: f32, config: &SimulationConfig) {
    // Always update temperatures toward targets
    update_temperatures(state, delta_secs, &config.temp_rates);

    // Update fan speeds based on state
    update_fans(state);

    // If there's an active print job, advance its simulation
    if let Some(ref mut job) = state.print_job {
        match &job.current_stage {
            PrintStage::Heating {
                target_nozzle,
                target_bed,
            } => {
                // Check if temperatures have been reached
                if temps_reached(
                    state.nozzle_temp,
                    *target_nozzle,
                    state.bed_temp,
                    *target_bed,
                ) {
                    info!("Heating complete, temperatures reached");
                    // Advance to next stage based on print options
                    if job.bed_levelling {
                        job.advance_stage(PrintStage::BedLeveling);
                    } else {
                        job.advance_stage(PrintStage::Purging);
                    }
                    state.gcode_state = GcodeState::Running;
                }
            }
            PrintStage::BedLeveling => {
                let elapsed = chrono::Utc::now()
                    .signed_duration_since(job.stage_started_at)
                    .num_milliseconds() as f32
                    / 1000.0
                    * config.time_multiplier;

                if elapsed >= config.stage_durations.bed_leveling_secs {
                    info!("Bed leveling complete");
                    job.advance_stage(PrintStage::Purging);
                }
            }
            PrintStage::Purging => {
                let elapsed = chrono::Utc::now()
                    .signed_duration_since(job.stage_started_at)
                    .num_milliseconds() as f32
                    / 1000.0
                    * config.time_multiplier;

                if elapsed >= config.stage_durations.purging_secs {
                    info!("Purging complete, starting print");
                    job.advance_stage(PrintStage::Printing);
                }
            }
            PrintStage::Printing => {
                // Only advance if not paused
                if state.gcode_state == GcodeState::Running {
                    // Calculate layer progress based on time
                    if job.total_layer_num > 0 {
                        let total_print_secs = job.estimated_total_time.as_secs_f32();
                        let secs_per_layer = total_print_secs / job.total_layer_num as f32;

                        if secs_per_layer > 0.0 {
                            let layers_to_advance = (delta_secs / secs_per_layer) as u32;
                            if layers_to_advance > 0 {
                                let new_layer =
                                    (job.layer_num + layers_to_advance).min(job.total_layer_num);

                                if new_layer > job.layer_num {
                                    debug!(
                                        "Advancing from layer {} to {} (of {})",
                                        job.layer_num, new_layer, job.total_layer_num
                                    );
                                }

                                job.update_progress(new_layer);

                                // Update remaining time
                                let remaining_layers = job.total_layer_num - job.layer_num;
                                job.mc_remaining_time =
                                    ((remaining_layers as f32 * secs_per_layer) / 60.0) as u32;

                                // Consume filament (roughly 0.05% per layer for simplicity)
                                if job.use_ams && !job.ams_mapping.is_empty() {
                                    let tray_id = job.ams_mapping[0];
                                    if let Some(tray) = state.ams.get_tray_mut(0, tray_id) {
                                        let consume_percent = ((layers_to_advance as f32) * 0.05) as u8;
                                        tray.consume(consume_percent.max(1));
                                    }
                                }
                            }
                        }
                    }

                    // Check if print is complete
                    if job.layer_num >= job.total_layer_num {
                        info!("Printing complete, starting cooling");
                        job.advance_stage(PrintStage::Cooling);
                        // Start cooling - set target temps to 0
                        state.nozzle_target_temp = 0.0;
                        state.bed_target_temp = 0.0;
                    }
                }
            }
            PrintStage::Cooling => {
                let elapsed = chrono::Utc::now()
                    .signed_duration_since(job.stage_started_at)
                    .num_milliseconds() as f32
                    / 1000.0
                    * config.time_multiplier;

                // Also check if temperatures are low enough
                let temps_cooled = state.nozzle_temp < 50.0 && state.bed_temp < 40.0;

                if elapsed >= config.stage_durations.cooling_secs || temps_cooled {
                    info!("Cooling complete, print finished");
                    job.advance_stage(PrintStage::Complete);
                }
            }
            PrintStage::Complete => {
                // Print is done, clean up
                info!("Print job complete: {}", job.subtask_name);
                state.complete_print();
            }
        }
    } else {
        // No active job - cool down if needed
        if state.nozzle_target_temp == 0.0 && state.bed_target_temp == 0.0 {
            // Return to idle if we're finished and cooled down
            if state.gcode_state == GcodeState::Finish {
                if state.nozzle_temp <= AMBIENT_TEMP + TEMP_TOLERANCE
                    && state.bed_temp <= AMBIENT_TEMP + TEMP_TOLERANCE
                {
                    state.gcode_state = GcodeState::Idle;
                }
            }
        }
    }
}

/// Update temperatures based on targets and rates
fn update_temperatures(state: &mut PrinterState, delta_secs: f32, rates: &TemperatureRates) {
    // Determine cooling/heating targets
    let nozzle_target = if state.nozzle_target_temp > 0.0 {
        state.nozzle_target_temp
    } else {
        AMBIENT_TEMP
    };

    let bed_target = if state.bed_target_temp > 0.0 {
        state.bed_target_temp
    } else {
        AMBIENT_TEMP
    };

    // Chamber slowly rises when nozzle/bed are hot, otherwise cools
    let chamber_target = if state.nozzle_temp > 100.0 || state.bed_temp > 50.0 {
        // Passive heating from hot components
        45.0_f32.min(state.nozzle_temp * 0.15 + state.bed_temp * 0.2)
    } else {
        AMBIENT_TEMP
    };

    // Update temperatures
    state.nozzle_temp = approach_temperature(
        state.nozzle_temp,
        nozzle_target,
        rates.nozzle_heat_rate,
        rates.nozzle_cool_rate,
        delta_secs,
    );

    state.bed_temp = approach_temperature(
        state.bed_temp,
        bed_target,
        rates.bed_heat_rate,
        rates.bed_cool_rate,
        delta_secs,
    );

    state.chamber_temp = approach_temperature(
        state.chamber_temp,
        chamber_target,
        rates.chamber_heat_rate,
        rates.chamber_cool_rate,
        delta_secs,
    );
}

/// Update fan speeds based on printer state
fn update_fans(state: &mut PrinterState) {
    // Heatbreak fan runs when nozzle is hot
    state.heatbreak_fan_speed = if state.nozzle_temp > 50.0 { 7 } else { 0 };

    // Cooling fan runs during printing
    state.cooling_fan_speed = if state.gcode_state == GcodeState::Running {
        15 // Full speed during print
    } else {
        0
    };

    // Aux fan (big_fan1) for cooling
    state.aux_fan_speed = if state.nozzle_temp > 200.0 || state.bed_temp > 80.0 {
        10
    } else {
        0
    };

    // Chamber fan for enclosure
    state.chamber_fan_speed = if state.chamber_temp > 40.0 { 5 } else { 0 };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PrintJob;

    #[test]
    fn test_simulation_config_default() {
        let config = SimulationConfig::default();
        assert_eq!(config.time_multiplier, 100.0);
        assert_eq!(config.tick_interval, Duration::from_millis(100));
    }

    #[test]
    fn test_simulation_config_custom() {
        let config = SimulationConfig::new(50.0);
        assert_eq!(config.time_multiplier, 50.0);
    }

    #[test]
    fn test_update_temperatures_heating() {
        let mut state = PrinterState::new("TEST".to_string());
        state.nozzle_target_temp = 200.0;
        state.bed_target_temp = 60.0;

        let rates = TemperatureRates::default();
        update_temperatures(&mut state, 1.0, &rates);

        assert!(state.nozzle_temp > 25.0); // Should have increased
        assert!(state.bed_temp > 25.0); // Should have increased
    }

    #[test]
    fn test_update_temperatures_cooling() {
        let mut state = PrinterState::new("TEST".to_string());
        state.nozzle_temp = 200.0;
        state.bed_temp = 60.0;
        state.nozzle_target_temp = 0.0;
        state.bed_target_temp = 0.0;

        let rates = TemperatureRates::default();
        update_temperatures(&mut state, 1.0, &rates);

        assert!(state.nozzle_temp < 200.0); // Should have decreased
        assert!(state.bed_temp < 60.0); // Should have decreased
    }

    #[test]
    fn test_update_fans_idle() {
        let mut state = PrinterState::new("TEST".to_string());
        update_fans(&mut state);

        assert_eq!(state.heatbreak_fan_speed, 0);
        assert_eq!(state.cooling_fan_speed, 0);
    }

    #[test]
    fn test_update_fans_printing() {
        let mut state = PrinterState::new("TEST".to_string());
        state.nozzle_temp = 200.0;
        state.gcode_state = GcodeState::Running;
        update_fans(&mut state);

        assert!(state.heatbreak_fan_speed > 0);
        assert!(state.cooling_fan_speed > 0);
    }

    #[test]
    fn test_simulation_tick_idle() {
        let mut state = PrinterState::new("TEST".to_string());
        let config = SimulationConfig::default();

        simulation_tick(&mut state, 1.0, &config);

        // Should remain idle with no changes
        assert_eq!(state.gcode_state, GcodeState::Idle);
        assert!(state.print_job.is_none());
    }

    #[test]
    fn test_simulation_tick_heating_progress() {
        let mut state = PrinterState::new("TEST".to_string());
        let config = SimulationConfig::default();

        let job = PrintJob::new(
            "task1".to_string(),
            "test.3mf".to_string(),
            "/cache/test.gcode".to_string(),
            1,
            100,
            60,
            false, // No bed leveling for faster test
            false,
            vec![],
            200.0,
            55.0,
        );

        state.start_print(job);

        // Simulate enough time to heat up (with high time multiplier)
        for _ in 0..100 {
            simulation_tick(&mut state, 10.0, &config);
        }

        // Should have transitioned past heating
        if let Some(ref job) = state.print_job {
            assert_ne!(
                job.current_stage,
                PrintStage::Heating {
                    target_nozzle: 200.0,
                    target_bed: 55.0
                }
            );
        }
    }

    #[test]
    fn test_filament_consumption_during_printing() {
        let mut state = PrinterState::new("TEST".to_string());
        let config = SimulationConfig::new(1000.0); // Very fast for testing

        let initial_filament = state.ams.get_tray(0, 0).unwrap().remaining_percent;

        let mut job = PrintJob::new(
            "task1".to_string(),
            "test.3mf".to_string(),
            "/cache/test.gcode".to_string(),
            1,
            100,
            60,
            false,
            true,
            vec![0],
            200.0,
            55.0,
        );

        // Set job to printing stage
        job.current_stage = PrintStage::Printing;
        state.start_print(job);
        state.gcode_state = GcodeState::Running;

        // Run many simulation ticks to advance layers and consume filament
        for _ in 0..100 {
            simulation_tick(&mut state, 100.0, &config);
        }

        let new_filament = state.ams.get_tray(0, 0).unwrap().remaining_percent;
        assert!(new_filament < initial_filament, "Filament should have been consumed during printing");
    }
}
