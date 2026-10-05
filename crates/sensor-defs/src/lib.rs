//! Sensor definitions shared by readers, estimators, and controllers.
//!
//! Plain data only: this crate knows what a sensor *measures*, not how any
//! estimator uses it.
#![allow(clippy::upper_case_acronyms)]

pub mod error;
pub mod gps;
pub mod imu;

pub use error::SensorError;
pub use gps::GPS;
pub use imu::IMU;
