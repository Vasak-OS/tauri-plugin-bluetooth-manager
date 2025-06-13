use serde::{ser::Serializer, Serialize};
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Zbus(zbus::Error),
    Zvariant(zbus::zvariant::Error),
    CommandError(String),
    NotFound(String),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Zbus(e) => Some(e),
            Error::Zvariant(e) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Zbus(e) => write!(f, "D-Bus error: {}", e),
            Error::Zvariant(e) => write!(f, "D-Bus variant error: {}", e),
            Error::CommandError(s) => write!(f, "Command error: {}", s),
            Error::NotFound(s) => write!(f, "Not found: {}", s),
        }
    }
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<zbus::Error> for Error {
    fn from(err: zbus::Error) -> Self {
        Error::Zbus(err)
    }
}

impl From<zbus::zvariant::Error> for Error {
    fn from(err: zbus::zvariant::Error) -> Self {
        Error::Zvariant(err)
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(_err: std::convert::Infallible) -> Self {
        Error::CommandError("Infallible error encountered".to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
