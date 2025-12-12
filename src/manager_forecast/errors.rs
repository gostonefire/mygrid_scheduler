
use thiserror::Error;

#[derive(Error, Debug)]
#[error("error in forecast manager")]
pub enum ForecastError {
    #[error("error while managing dates: {0}")]
    DateError(String),
    #[error("error while fetching forecast: {0}")]
    FetchError(String),
    #[error("error while parsing forecast: {0}")]
    ParseError(String),
    #[error("error while processing forecast: {0}")]
    EmptyForecastError(String),
}
