//! Names for every message type that crosses Zenoh between nodes.
//!
//! A node contract's `type = "..."` must be one of these; the
//! orchestrator will reject anything that is not this. Adding a message
//! type means adding a variant here, so this acts as a "master list".

/// A message type that may appear on a Zenoh topic.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MsgType {
    IMU,
    GPS,
    /// ZED left image, BGRA. Wire format: zed-reader README
    ZedImage,
    /// ZED depth, f32 meters. Wire format: zed-reader README
    ZedDepth,
    StateEstimate,
}

impl MsgType {
    /// Every variant, for listing and for tests.
    pub const ALL: [MsgType; 5] = [
        Self::IMU,
        Self::GPS,
        Self::ZedImage,
        Self::ZedDepth,
        Self::StateEstimate,
    ];

    /// the name a contract uses for this type.
    pub fn name(&self) -> &'static str {
        match self {
            Self::IMU => "IMU",
            Self::GPS => "GPS",
            Self::ZedImage => "ZedImage",
            Self::ZedDepth => "ZedDepth",
            Self::StateEstimate => "StateEstimate",
        }
    }

    /// The type a contract name refers to, if any. Exact match: `"imu"` is
    /// not `"IMU"`
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "IMU" => Some(Self::IMU),
            "GPS" => Some(Self::GPS),
            "ZedImage" => Some(Self::ZedImage),
            "ZedDepth" => Some(Self::ZedDepth),
            "StateEstimate" => Some(Self::StateEstimate),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_round_trips() {
        for msg_type in MsgType::ALL {
            assert_eq!(MsgType::from_name(msg_type.name()), Some(msg_type));
        }
    }

    #[test]
    fn names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for msg_type in MsgType::ALL {
            assert!(seen.insert(msg_type.name()), "{} twice", msg_type.name());
        }
    }

    #[test]
    fn unknown_and_miscased_names_rejected() {
        assert_eq!(MsgType::from_name("cev_msgs/IMU"), None);
        assert_eq!(MsgType::from_name("imu"), None);
        assert_eq!(MsgType::from_name(""), None);
    }
}
