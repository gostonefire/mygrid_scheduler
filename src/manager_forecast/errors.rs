use std::fmt;
use ureq::Error;

pub enum ForecastError {
    Network(String),
    Document(String),
}

impl fmt::Display for ForecastError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ForecastError::Network(e) => write!(f, "ForecastError::Network: {}", e),
            ForecastError::Document(e) => write!(f, "ForecastError::Document: {}", e),
        }
    }
}
impl From<Error> for ForecastError {
    fn from(e: Error) -> Self {
        ForecastError::Network(e.to_string())
    }
}
impl From<serde_json::Error> for ForecastError {
    fn from(e: serde_json::Error) -> Self {
        ForecastError::Document(e.to_string())
    }
}
