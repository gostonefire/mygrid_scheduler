
use thiserror::Error;

#[derive(Error, Debug)]
#[error("error in forecast manager: {0}")]
pub struct ForecastError(pub String);
