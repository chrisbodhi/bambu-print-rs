//! Print simulation engine
//!
//! This module handles simulating the 3D printing process, including:
//! - Temperature control (heating/cooling)
//! - Print stage transitions
//! - Layer progress
//! - Filament consumption

pub mod engine;
pub mod temperature;

pub use engine::{run_simulation_engine, SimulationConfig};
pub use temperature::{TemperatureRates, AMBIENT_TEMP, TEMP_TOLERANCE};
