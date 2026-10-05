//! Error struct for constructing sensor measurements
//! i.e. - when a sensor is missing or configured incorrectly

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum SensorError {
    NonFinite { field: &'static str },
    OutOfRange { field: &'static str, value: f64 },
    NonPositiveVariance { axis: usize, value: f64 },
}

impl fmt::Display for SensorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { field } => write!(f, "`{field}` is NaN or infinite"),
            Self::OutOfRange { field, value } => write!(f, "`{field}` = {value} is out of range"),
            Self::NonPositiveVariance { axis, value } => {
                write!(f, "variance on axis {axis} = {value} must be > 0")
            }
        }
    }
}

impl std::error::Error for SensorError {} // implementation for anyhow library use

//NOTE: should this change to be an nalgebra vec, or should the conversion take place in the
//estimator?
pub(crate) fn check_finite(field: &'static str, values: &[f64]) -> Result<(), SensorError> {
    for &v in values {
        if !v.is_finite() {
            return Err(SensorError::NonFinite { field });
        }
    }
    Ok(())
}
