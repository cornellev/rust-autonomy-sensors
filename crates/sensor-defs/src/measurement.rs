//! One stream for every sensor: what a transport or reorder buffer carries.
use crate::{GPS, IMU};

/// Any sensor reading. No `#[non_exhaustive]` - adding a variant
/// should break every downstream `match` until it decides what to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measurement {
    IMU(IMU),
    GPS(GPS),
}

impl Measurement {
    pub fn stamp_ns(&self) -> u64 {
        match self {
            Self::IMU(imu) => imu.stamp_ns(),
            Self::GPS(gps) => gps.stamp_ns(),
        }
    }
}

// Automatically defines Into as well here
impl From<IMU> for Measurement {
    fn from(imu: IMU) -> Self {
        Self::IMU(imu)
    }
}

// Automatically defines Into as well here
impl From<GPS> for Measurement {
    fn from(gps: GPS) -> Self {
        Self::GPS(gps)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::SensorError;

    #[test]
    fn stamp_comes_from_inner_reading() -> Result<(), SensorError> {
        let imu: Measurement = IMU::new(7, [0.0; 3], [0.0; 3])?.into();
        let gps: Measurement = GPS::new(9, 42.444, -76.502, 123.0, [4.0; 3])?.into();
        assert_eq!(imu.stamp_ns(), 7);
        assert_eq!(gps.stamp_ns(), 9);
        Ok(())
    }

    #[test]
    fn from_wraps_in_the_matching_variant() -> Result<(), SensorError> {
        let imu = IMU::new(7, [0.0; 3], [0.0; 3])?;
        assert_eq!(Measurement::from(imu), Measurement::IMU(imu));
        Ok(())
    }
}
