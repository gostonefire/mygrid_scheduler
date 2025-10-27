use std::fmt;
use std::fmt::Formatter;
use spa_sra::errors::SpaError;
use crate::errors::SplineError;

#[derive(Debug)]
pub struct ProdError(pub String);
impl fmt::Display for ProdError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ProdError: {}", self.0)
    }
}
impl From<SpaError> for ProdError {
    fn from(e: SpaError) -> Self { ProdError(e.to_string()) }
}
impl From<&str> for ProdError {
    fn from(e: &str) -> Self { ProdError(e.to_string()) }
}
impl From<SplineError> for ProdError {
    fn from(e: SplineError) -> Self { ProdError(e.to_string()) }
}