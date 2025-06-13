use serde::{ser::Serializer, Serialize};
use std::fmt;

#[derive(Debug)] // Añadido Debug
pub enum Error {
    Zbus(zbus::Error),
    ZbusVariant(zbus::zvariant::Error),
    // Otros errores específicos de tu aplicación pueden ir aquí
    CommandError(String),
    NotFound(String),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Zbus(e) => Some(e),
            Error::ZbusVariant(e) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Zbus(e) => write!(f, "D-Bus error: {}", e),
            Error::ZbusVariant(e) => write!(f, "D-Bus variant error: {}", e),
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
        // Usar la implementación de Display para serializar el error como una cadena
        serializer.serialize_str(&self.to_string())
    }
}

// Implementación de From para errores de zbus
impl From<zbus::Error> for Error {
    fn from(err: zbus::Error) -> Self {
        Error::Zbus(err)
    }
}

impl From<zbus::zvariant::Error> for Error {
    fn from(err: zbus::zvariant::Error) -> Self {
        Error::ZbusVariant(err)
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(_err: std::convert::Infallible) -> Self {
        // This case should ideally not be reached if Infallible means "cannot fail".
        // However, to satisfy the trait bound for `?`, we need to provide a variant.
        // Using CommandError or a new dedicated variant might be appropriate.
        Error::CommandError("Infallible error encountered".to_string())
    }
}

// ... (tu Result personalizado si lo tienes, o puedes usar std::result::Result<T, crate::Error>)
pub type Result<T> = std::result::Result<T, Error>;
