//! IMU sample: body frame angular rate and specific force
use crate::error::{SensorError, check_finite};

#[derive(Debug, Clone, Copy, PartialEq)]
// only way to get values is by creating an IMU object that is valid
pub struct IMU {
    stamp_ns: u64,
    gyro_rad_s: [f64; 3],
    accel_m_s2: [f64; 3],
}

impl IMU {
    pub fn new(
        stamp_ns: u64,
        gyro_rad_s: [f64; 3],
        accel_m_s2: [f64; 3],
    ) -> Result<Self, SensorError> {
        check_finite("gyro_rad_s", &gyro_rad_s)?;
        check_finite("accel_m_s2", &accel_m_s2)?;
        Ok(Self {
            stamp_ns,
            gyro_rad_s,
            accel_m_s2,
        })
    }

    pub fn stamp_ns(&self) -> u64 {
        self.stamp_ns
    }

    pub fn gyro_rad_s(&self) -> [f64; 3] {
        self.gyro_rad_s
    }

    pub fn accel_m_s2(&self) -> [f64; 3] {
        self.accel_m_s2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_sample_builds() -> Result<(), SensorError> {
        let imu = IMU::new(10, [0.0, 0.0, 0.1], [0.0, 0.0, 9.81])?;
        assert_eq!(imu.gyro_rad_s()[2], 0.1);
        Ok(())
    }

    #[test]
    fn nan_gyro_is_rejected() {
        let err = IMU::new(10, [f64::NAN, 0.0, 0.0], [0.0; 3]).unwrap_err();
        assert_eq!(
            err,
            SensorError::NonFinite {
                field: "gyro_rad_s"
            }
        );
    }

    #[test]
    fn infinite_accel_is_rejected() {
        let err = IMU::new(10, [0.0; 3], [0.0, f64::INFINITY, 0.0]).unwrap_err();
        assert_eq!(
            err,
            SensorError::NonFinite {
                field: "accel_m_s2"
            }
        );
    }
}
