use std::fmt;

/// On-disk format version, independent of the application/crate release version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

pub const VERSION: Version = Version { major: 1, minor: 0 };

impl Version {
    pub(crate) fn to_le_bytes(self) -> [u8; 4] {
        let major = self.major.to_le_bytes();
        let minor = self.minor.to_le_bytes();
        [major[0], major[1], minor[0], minor[1]]
    }

    pub(crate) fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self {
            major: u16::from_le_bytes([bytes[0], bytes[1]]),
            minor: u16::from_le_bytes([bytes[2], bytes[3]]),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}
