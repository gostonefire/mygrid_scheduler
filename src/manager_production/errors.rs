use std::fmt::{Debug};
use spa_sra::errors::SpaError;
use thiserror::Error;
use crate::errors::SplineError;

#[derive(Debug, Error)]
pub enum ProdError {
    #[error("error in SPA_SRA: {0}")]
    SpaError(#[from] SpaError),
    #[error("error in spline algorithm: {0}")]
    SplineError(#[from] SplineError),
    #[error("error in thermodynamics: {0}")]
    ThermodynamicsError(String),
}
