//! GPS fix: raw geodetic position from the receiver
use crate::error::{SensorError, check_finite};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GPS {
    stamp_ns: u64,
    lat_deg: f64,
    lon_deg: f64,
    alt_m: f64,
    pos_var_m2: [f64; 3], // [ENU]
}

impl GPS {
    pub fn new(
        stamp_ns: u64,
        lat_deg: f64,
        lon_deg: f64,
        alt_m: f64,
        pos_var_m2: [f64; 3],
    ) -> Result<Self, SensorError> {
        // Finite checks on everything first
        check_finite("lat_deg", &[lat_deg])?;
        check_finite("lon_deg", &[lon_deg])?;
        check_finite("alt_m", &[alt_m])?;
        check_finite("pos_var_m2", &pos_var_m2)?;

        // Range checks on latitude and longitude
        if !(-90.0..=90.0).contains(&lat_deg) {
            return Err(SensorError::OutOfRange {
                field: "lat_deg",
                value: lat_deg,
            });
        }

        if !(-180.0..=180.0).contains(&lon_deg) {
            return Err(SensorError::OutOfRange {
                field: "lon_deg",
                value: lon_deg,
            });
        }

        // Zero variance check per axis of pos_var_m2
        for (axis, &value) in pos_var_m2.iter().enumerate() {
            if value <= 0.0 {
                return Err(SensorError::NonPositiveVariance { axis, value });
            }
        }
        Ok(Self {
            stamp_ns,
            lat_deg,
            lon_deg,
            alt_m,
            pos_var_m2,
        })
    }

    pub fn stamp_ns(&self) -> u64 {
        self.stamp_ns
    }
    pub fn lat_deg(&self) -> f64 {
        self.lat_deg
    }
    pub fn lon_deg(&self) -> f64 {
        self.lon_deg
    }
    pub fn alt_m(&self) -> f64 {
        self.alt_m
    }
    pub fn pos_var_m2(&self) -> [f64; 3] {
        self.pos_var_m2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITHACA: (f64, f64, f64) = (42.4440, -76.5019, 123.0);

    #[test]
    fn valid_fix_builds() -> Result<(), SensorError> {
        let (lat, lon, alt) = ITHACA;
        let gps = GPS::new(10, lat, lon, alt, [4.0, 4.0, 9.0])?;
        assert_eq!(gps.lat_deg(), lat);
        Ok(())
    }

    #[test]
    fn latitude_past_pole_is_rejected() {
        let (_, lon, alt) = ITHACA;
        let err = GPS::new(10, 91.0, lon, alt, [4.0; 3]).unwrap_err();
        assert_eq!(
            err,
            SensorError::OutOfRange {
                field: "lat_deg",
                value: 91.0
            }
        );
    }

    #[test]
    fn non_wrapped_longitude_is_rejected() {
        let (lat, _, alt) = ITHACA;
        let err = GPS::new(10, lat, 185.0, alt, [4.0; 3]).unwrap_err();
        assert_eq!(
            err,
            SensorError::OutOfRange {
                field: "lon_deg",
                value: 185.0
            }
        );
    }

    #[test]
    fn nan_altitude_is_rejected() {
        let (lat, lon, _) = ITHACA;
        let err = GPS::new(10, lat, lon, f64::NAN, [4.0; 3]).unwrap_err();
        assert_eq!(err, SensorError::NonFinite { field: "alt_m" });
    }

    #[test]
    fn zero_variance_is_rejected() {
        let (lat, lon, alt) = ITHACA;
        let err = GPS::new(10, lat, lon, alt, [4.0, 0.0, 9.0]).unwrap_err();
        assert_eq!(
            err,
            SensorError::NonPositiveVariance {
                axis: 1,
                value: 0.0
            }
        );
    }

    #[test]
    fn readable_error_message() {
        let err = SensorError::OutOfRange {
            field: "lat_deg",
            value: 91.0,
        };
        assert_eq!(err.to_string(), "`lat_deg` = 91 is out of range");
    }
}
