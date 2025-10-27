use std::fmt;
use std::fmt::Formatter;
use chrono::ParseError;
use ureq::Error;


pub enum FoxError {
    FoxCloud(String),
    Document(String),
    Other(String),
}

impl fmt::Display for FoxError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            FoxError::FoxCloud(e) => write!(f, "FoxError::FoxCloud: {}", e),
            FoxError::Document(e) => write!(f, "FoxError::Document: {}", e),
            FoxError::Other(e) => write!(f, "FoxError::Schedule: {}", e),
        }
    }
}
impl From<String> for FoxError {
    fn from(e: String) -> Self {
        FoxError::Other(e)
    }
}
impl From<&str> for FoxError {
    fn from(e: &str) -> Self {
        FoxError::Other(e.to_string())
    }
}
impl From<Error> for FoxError {
    fn from(e: Error) -> FoxError {
        FoxError::FoxCloud(e.to_string())
    }
}
impl From<serde_json::Error> for FoxError {
    fn from(e: serde_json::Error) -> FoxError {
        FoxError::Document(e.to_string())
    }
}
impl From<ParseError> for FoxError {
    fn from(e: ParseError) -> FoxError { FoxError::Document(e.to_string()) }
}
