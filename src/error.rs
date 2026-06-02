use serde::Serializer;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("D-Bus error: {0}")]
    Zbus(#[from] zbus::Error),
    #[error("D-Bus variant error: {0}")]
    Zvariant(#[from] zbus::zvariant::Error),
    #[error("Command error: {0}")]
    CommandError(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(_err: std::convert::Infallible) -> Self {
        Error::CommandError("Infallible error encountered".to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
