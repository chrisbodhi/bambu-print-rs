//! Temperature simulation for heating and cooling

/// Temperature rates (degrees Celsius per second at 1x speed)
pub struct TemperatureRates {
    pub nozzle_heat_rate: f32,
    pub nozzle_cool_rate: f32,
    pub bed_heat_rate: f32,
    pub bed_cool_rate: f32,
    pub chamber_heat_rate: f32,
    pub chamber_cool_rate: f32,
}

impl Default for TemperatureRates {
    fn default() -> Self {
        Self {
            nozzle_heat_rate: 3.0,   // 3°C/sec when heating
            nozzle_cool_rate: 1.0,   // 1°C/sec when cooling (with fan)
            bed_heat_rate: 1.0,      // 1°C/sec when heating
            bed_cool_rate: 0.3,      // 0.3°C/sec when cooling
            chamber_heat_rate: 0.1,  // 0.1°C/sec (passive heating from bed/nozzle)
            chamber_cool_rate: 0.05, // Very slow cooling
        }
    }
}

/// Ambient temperature (default starting point)
pub const AMBIENT_TEMP: f32 = 25.0;

/// Temperature tolerance for "at target" checks
pub const TEMP_TOLERANCE: f32 = 2.0;

/// Simulate temperature approach toward a target
/// Returns the new temperature after the given time delta
pub fn approach_temperature(
    current: f32,
    target: f32,
    heat_rate: f32,
    cool_rate: f32,
    delta_secs: f32,
) -> f32 {
    if (current - target).abs() < TEMP_TOLERANCE {
        // Close enough, snap to target
        target
    } else if current < target {
        // Heating
        (current + heat_rate * delta_secs).min(target)
    } else {
        // Cooling
        (current - cool_rate * delta_secs).max(target)
    }
}

/// Check if a temperature is at or near its target
pub fn is_at_target(current: f32, target: f32) -> bool {
    (current - target).abs() <= TEMP_TOLERANCE
}

/// Check if both nozzle and bed are at their target temperatures
pub fn temps_reached(
    nozzle_current: f32,
    nozzle_target: f32,
    bed_current: f32,
    bed_target: f32,
) -> bool {
    is_at_target(nozzle_current, nozzle_target) && is_at_target(bed_current, bed_target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approach_temperature_heating() {
        let rates = TemperatureRates::default();
        let new_temp = approach_temperature(25.0, 200.0, rates.nozzle_heat_rate, rates.nozzle_cool_rate, 1.0);
        assert_eq!(new_temp, 28.0); // 25 + 3 = 28
    }

    #[test]
    fn test_approach_temperature_cooling() {
        let rates = TemperatureRates::default();
        let new_temp = approach_temperature(200.0, 25.0, rates.nozzle_heat_rate, rates.nozzle_cool_rate, 1.0);
        assert_eq!(new_temp, 199.0); // 200 - 1 = 199
    }

    #[test]
    fn test_approach_temperature_at_target() {
        let rates = TemperatureRates::default();
        let new_temp = approach_temperature(199.0, 200.0, rates.nozzle_heat_rate, rates.nozzle_cool_rate, 1.0);
        assert_eq!(new_temp, 200.0); // Within tolerance, snaps to target
    }

    #[test]
    fn test_approach_temperature_doesnt_overshoot() {
        let rates = TemperatureRates::default();
        // Heat rate of 3°C/sec for 100 seconds would overshoot, but should clamp
        let new_temp = approach_temperature(195.0, 200.0, rates.nozzle_heat_rate, rates.nozzle_cool_rate, 100.0);
        assert_eq!(new_temp, 200.0);
    }

    #[test]
    fn test_is_at_target() {
        assert!(is_at_target(200.0, 200.0));
        assert!(is_at_target(199.0, 200.0)); // Within tolerance
        assert!(is_at_target(201.5, 200.0)); // Within tolerance
        assert!(!is_at_target(195.0, 200.0)); // Outside tolerance
    }

    #[test]
    fn test_temps_reached() {
        assert!(temps_reached(200.0, 200.0, 55.0, 55.0));
        assert!(temps_reached(199.0, 200.0, 54.0, 55.0)); // Within tolerance
        assert!(!temps_reached(100.0, 200.0, 55.0, 55.0)); // Nozzle not ready
        assert!(!temps_reached(200.0, 200.0, 30.0, 55.0)); // Bed not ready
    }

    #[test]
    fn test_bed_heating_rate() {
        let rates = TemperatureRates::default();
        let new_temp = approach_temperature(25.0, 60.0, rates.bed_heat_rate, rates.bed_cool_rate, 10.0);
        assert_eq!(new_temp, 35.0); // 25 + 1*10 = 35
    }

    #[test]
    fn test_bed_cooling_rate() {
        let rates = TemperatureRates::default();
        let new_temp = approach_temperature(60.0, 25.0, rates.bed_heat_rate, rates.bed_cool_rate, 10.0);
        assert_eq!(new_temp, 57.0); // 60 - 0.3*10 = 57
    }
}
