use std::fmt;
use std::fmt::Formatter;
use ureq::Error;


pub enum NordPoolError {
    NordPool(String),
    Document(String),
    NoContent,
}

impl fmt::Display for NordPoolError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            NordPoolError::NordPool(e) => write!(f, "NordPoolError::NordPool: {}", e),
            NordPoolError::Document(e) => write!(f, "NordPoolError::Document: {}", e),
            NordPoolError::NoContent           => write!(f, "NordPoolError::NoContent"),
        }
    }
}
impl From<&str> for NordPoolError {
    fn from(e: &str) -> Self {
        NordPoolError::NordPool(e.to_string())
    }
}
impl From<Error> for NordPoolError {
    fn from(e: Error) -> Self {
        NordPoolError::NordPool(e.to_string())
    }
}
impl From<serde_json::Error> for NordPoolError {
    fn from(e: serde_json::Error) -> Self {
        NordPoolError::Document(e.to_string())
    }
}
