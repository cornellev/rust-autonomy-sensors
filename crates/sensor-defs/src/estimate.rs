//! Contract between sensor definitions and any estimator.
//!
//! Estimators opt in per sensor: `impl Update<GPS> for MyFilter`.
//! Calling `update` with a sensor that has no impl is a compile error.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reject {
    NotRunning,
    OutOfOrder { stamp_ns: u64, last_ns: u64 },
    SingularInnovation,
}

impl fmt::Display for Reject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => write!(f, "estimator is not running yet"),
            Self::OutOfOrder { stamp_ns, last_ns } => {
                write!(
                    f,
                    "stamp {stamp_ns} ns is older than filter time {last_ns} ns"
                )
            }
            Self::SingularInnovation => write!(f, "innovation covariance is singular"),
        }
    }
}

impl std::error::Error for Reject {}

/// Fuse a measurement `S` into the state: z = h(x) + v.
pub trait Update<S> {
    fn update(&mut self, meas: &S) -> Result<(), Reject>;
}

//NOTE: not sure if I want this here, as propagation model should be separate, no?

/// Drive the process model forward with an input `U`: x' = f(x,u).
pub trait Propagate<U> {
    fn propagate(&mut self, input: &U) -> Result<(), Reject>;
}
