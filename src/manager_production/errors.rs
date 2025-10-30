use std::fmt::{Debug};
use thiserror::Error;
use crate::errors::SplineError;

#[derive(Debug, Error)]
pub enum ProdError {
    #[error("error in SPA_SRA: {0}")]
    SpaError(String),
    #[error("error in spline algorithm: {0}")]
    SplineError(#[from] SplineError),
    #[error("error in thermodynamics: {0}")]
    ThermodynamicsError(String),
}
