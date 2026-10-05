//! A scalar altitude filter written a way a downstream crate/estimator
//! would use sensor-defs: it only sees the public API
use sensor_defs::{GPS, IMU, Propagate, Reject, SensorError, Update};

/// State: altitude x [m] with variance p [m^2].
/// Process model is a random walk, so the IMU only advances
/// time: x' = x, p' = p + q dt.
#[derive(Debug, Clone, PartialEq)]
struct AltFilter {
    x_m: f64,
    p_m2: f64,
    q_m2_s: f64,
    t_ns: u64,
}

impl AltFilter {
    fn check_order(&self, stamp_ns: u64) -> Result<(), Reject> {
        if stamp_ns < self.t_ns {
            return Err(Reject::OutOfOrder {
                stamp_ns,
                last_ns: self.t_ns,
            });
        }
        Ok(())
    }
}

impl Update<GPS> for AltFilter {
    fn update(&mut self, gps: &GPS) -> Result<(), Reject> {
        self.check_order(gps.stamp_ns())?;
        let h = 1.0;
        let r = gps.pos_var_m2()[2];
        let s = h * self.p_m2 * h + r;
        if s <= 0.0 {
            return Err(Reject::SingularInnovation);
        }
        let k = self.p_m2 * h / s;
        self.x_m += k * (gps.alt_m() - h * self.x_m);
        self.p_m2 = (1.0 - k * h) * self.p_m2 * (1.0 - k * h) + k * r * k; // Joseph form
        self.t_ns = gps.stamp_ns();
        Ok(())
    }
}

impl Propagate<IMU> for AltFilter {
    fn propagate(&mut self, imu: &IMU) -> Result<(), Reject> {
        // Update time, and scale covariance by dt.
        self.check_order(imu.stamp_ns())?;
        let dt = (imu.stamp_ns() - self.t_ns) as f64 * 1e-9;
        let p_new = self.p_m2 + self.q_m2_s * dt;
        self.p_m2 = p_new;
        self.t_ns = imu.stamp_ns();
        Ok(())
    }
}

fn fuse_all<E: Update<GPS>>(est: &mut E, fixes: &[GPS]) -> Result<(), Reject> {
    for fix in fixes {
        est.update(fix)?;
    }
    Ok(())
}

const S: u64 = 1_000_000_000;

fn filter() -> AltFilter {
    AltFilter {
        x_m: 100.0,
        p_m2: 25.0,
        q_m2_s: 0.5,
        t_ns: 0,
    }
}

fn fix(stamp_ns: u64, alt_m: f64, var_up_m2: f64) -> Result<GPS, SensorError> {
    GPS::new(stamp_ns, 42.444, -76.502, alt_m, [4.0, 4.0, var_up_m2])
}

#[test]
fn update_pulls_toward_measurement_and_shrinks_var() -> Result<(), SensorError> {
    let mut f = filter();
    f.update(&fix(S, 110.0, 25.0)?).unwrap();
    assert_eq!(f.x_m, 105.0); // equal variances: halfway
    assert_eq!(f.p_m2, 12.5);
    Ok(())
}

#[test]
fn variance_stays_positive_and_doesnt_grow() -> Result<(), SensorError> {
    for p in [1e-6, 1.0, 1e6] {
        for r in [1e-6, 1.0, 1e6] {
            let mut f = AltFilter {
                p_m2: p,
                ..filter()
            };
            f.update(&fix(S, 50.0, r)?).unwrap();
            assert!(f.p_m2 > 0.0 && f.p_m2 <= p, "p={p} r={r} -> {}", f.p_m2);
        }
    }
    Ok(())
}

#[test]
fn out_of_order_fix_is_rejected() -> Result<(), SensorError> {
    let mut f = AltFilter {
        t_ns: 5 * S,
        ..filter()
    };
    let before = f.clone();
    let err = f.update(&fix(2 * S, 110.0, 25.0)?).unwrap_err();
    assert_eq!(
        err,
        Reject::OutOfOrder {
            stamp_ns: 2 * S,
            last_ns: 5 * S
        }
    );
    assert_eq!(f, before);
    Ok(())
}

#[test]
fn generic_fuse_stops_at_reject() -> Result<(), SensorError> {
    let mut f = filter();
    let fixes = [fix(2 * S, 100.0, 1.0)?, fix(S, 100.0, 1.0)?];
    let err = fuse_all(&mut f, &fixes).unwrap_err();
    assert!(matches!(err, Reject::OutOfOrder { .. }));
    assert_eq!(f.t_ns, 2 * S); // first fix applied, second refused
    Ok(())
}

#[test]
fn propagate_grows_by_q_dt() -> Result<(), SensorError> {
    let mut f = filter();
    f.propagate(&IMU::new(2 * S, [0.0; 3], [0.0, 0.0, 9.81])?)
        .unwrap();
    assert_eq!(f.p_m2, 25.0 + 0.5 * 2.0);
    assert_eq!(f.x_m, 100.0);
    Ok(())
}

#[test]
fn out_of_order_imu_is_rejected() -> Result<(), SensorError> {
    let mut f = AltFilter {
        t_ns: 5 * S,
        ..filter()
    };
    let before = f.clone();
    let err = f.propagate(&IMU::new(S, [0.0; 3], [0.0; 3])?).unwrap_err();
    assert_eq!(
        err,
        Reject::OutOfOrder {
            stamp_ns: S,
            last_ns: 5 * S
        }
    );
    assert_eq!(f, before);
    Ok(())
}

#[test]
fn imu_and_gps_interleave() -> Result<(), Box<dyn std::error::Error>> {
    let mut f = filter();
    let ms = S / 1000;
    for k in 1..=100 {
        // 100 Hz IMU for 1s
        //NOTE: what error handling are we missing here? is this because of the test signature?
        f.propagate(&IMU::new(k * 10 * ms, [0.0; 3], [0.0; 3])?)?;
        if k % 20 == 0 {
            // 5 Hz GPS, any rate works as long as stamps are not backwards
            f.update(&fix(k * 10 * ms, 100.0, 4.0)?)?;
        }
    }
    assert_eq!(f.t_ns, S);
    assert!(f.p_m2 < 25.0);
    Ok(())
}

#[test]
fn late_gps_fix_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut f = filter();
    let ms = S / 1000;
    f.propagate(&IMU::new(1080 * ms, [0.0; 3], [0.0; 3])?)?;
    let err = f.update(&fix(S, 100.0, 4.0)?).unwrap_err();
    assert!(matches!(err, Reject::OutOfOrder { .. }));
    Ok(())
}
