use std::fmt::{Debug};
use spa_sra::errors::SpaError;
use thiserror::Error;
use crate::models::ForecastValuesError;


/// Error depicting errors that occur while estimating power production
///
#[derive(Debug, Error)]
#[error("error while estimating production")]
pub enum ProductionError {
    #[error("wrong input data length between tariffs, consumption and production")]
    WeatherDataError(#[from] ForecastValuesError),
    #[error("error while calculating solar positions")]
    SolarPositionsError(#[from] SpaError),
    #[error("error in thermodynamics: {0}")]
    ThermodynamicsError(String),
}
