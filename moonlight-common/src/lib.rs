use std::{
    ffi::NulError,
    fmt::{Debug, Display},
    num::ParseIntError,
    str::FromStr,
};

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MoonlightError {
    #[error("couldn't aquire an instance")]
    InstanceAquire,
    #[error("a connection is already active")]
    ConnectionAlreadyExists,
    #[error("the host doesn't support this feature")]
    NotSupportedOnHost,
    #[error("an error happened whilst sending an event")]
    EventSendError(i32),
    #[error("this call requires a GFE version which uses ENet")]
    ENetRequired,
    #[error("a string contained a nul byte which is not allowed in c strings")]
    StringNulError(#[from] NulError),
    #[error("couldn't establish a connection")]
    ConnectionFailed,
    #[error("the client is not paired")]
    NotPaired,
}

#[cfg(feature = "network")]
pub mod network;

#[cfg(feature = "stream")]
pub mod stream;

#[cfg(feature = "high")]
pub mod high;

#[cfg(feature = "pair")]
pub mod pair;

pub mod mac;

#[derive(Debug, Error, Clone)]
#[error("failed to parse the state of the server")]
pub struct ParseServerStateError;

#[derive(Debug, Copy, Clone)]
pub enum ServerState {
    Busy,
    Free,
}

impl FromStr for ServerState {
    type Err = ParseServerStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s.ends_with("FREE") => Ok(ServerState::Free),
            s if s.ends_with("BUSY") => Ok(ServerState::Busy),
            _ => Err(ParseServerStateError),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PairStatus {
    NotPaired,
    Paired,
}

#[derive(Debug, Error)]
#[error("failed to parse server version")]
pub enum ParseServerVersionError {
    #[error("{0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("invalid version pattern")]
    InvalidPattern,
}

#[derive(Debug, Clone, Copy)]
pub struct ServerVersion {
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
    pub mini_patch: i32,
}

impl ServerVersion {
    pub fn new(major: i32, minor: i32, patch: i32, mini_patch: i32) -> ServerVersion {
        Self {
            major,
            minor,
            patch,
            mini_patch,
        }
    }
}

impl Display for ServerVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major, self.minor, self.patch, self.mini_patch
        )
    }
}

impl FromStr for ServerVersion {
    type Err = ParseServerVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.splitn(4, ".");

        let major = split
            .next()
            .ok_or(ParseServerVersionError::InvalidPattern)?
            .parse()?;
        let minor = split
            .next()
            .ok_or(ParseServerVersionError::InvalidPattern)?
            .parse()?;
        let patch = split
            .next()
            .ok_or(ParseServerVersionError::InvalidPattern)?
            .parse()?;
        let mini_patch = split
            .next()
            .ok_or(ParseServerVersionError::InvalidPattern)?
            .parse()?;

        Ok(Self {
            major,
            minor,
            patch,
            mini_patch,
        })
    }
}

/// A pin which contains four values in the range 0..10
#[derive(Clone, Copy)]
pub struct PairPin {
    numbers: [u8; 4],
}

impl PairPin {
    #[cfg(feature = "pair")]
    pub fn generate() -> Result<Self, openssl::error::ErrorStack> {
        let rand_num = || {
            let mut num = [0u8];
            openssl::rand::rand_bytes(&mut num)?;
            Ok(num[0] % 10)
        };

        Ok(
            Self::from_array([rand_num()?, rand_num()?, rand_num()?, rand_num()?])
                .expect("generated invalid pair pin"),
        )
    }

    pub fn from_array(numbers: [u8; 4]) -> Option<Self> {
        let range = 0..10;

        if range.contains(&numbers[0])
            && range.contains(&numbers[1])
            && range.contains(&numbers[2])
            && range.contains(&numbers[3])
        {
            return Some(Self { numbers });
        }

        None
    }

    pub fn n(&self, index: usize) -> Option<u8> {
        self.numbers.get(index).copied()
    }
    pub fn n1(&self) -> u8 {
        self.numbers[0]
    }
    pub fn n2(&self) -> u8 {
        self.numbers[1]
    }
    pub fn n3(&self) -> u8 {
        self.numbers[2]
    }
    pub fn n4(&self) -> u8 {
        self.numbers[3]
    }

    pub fn array(&self) -> [u8; 4] {
        self.numbers
    }
}

impl Display for PairPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}{}{}", self.n1(), self.n2(), self.n3(), self.n4())
    }
}
impl Debug for PairPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PairPin(")?;
        Display::fmt(&self, f)?;
        write!(f, ")")?;

        Ok(())
    }
}

pub const SALT_LENGTH: usize = 16;
pub const CHALLENGE_LENGTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgorithm {
    Sha1,
    Sha256,
}

impl HashAlgorithm {
    pub const MAX_HASH_LEN: usize = 32;

    pub fn hash_len(&self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
        }
    }
}

pub fn hash_algorithm_for_server(server_version: ServerVersion) -> HashAlgorithm {
    if server_version.major >= 7 {
        HashAlgorithm::Sha256
    } else {
        HashAlgorithm::Sha1
    }
}
