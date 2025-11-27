use thiserror::Error;

/// Error depicting errors that occur while creating and managing schedules
///
#[derive(Debug, Error)]
#[error("error while building schedule: {0}")]
pub struct SchedulingError(pub String);

/// Error depicting errors that occur during Monotonic Cubic Spline interpolation
/// 
#[derive(Debug, Error)]
pub enum SplineError {
    #[error("x is too short")]
    IllegalLength,
    #[error("control points not monotonically increasing")]
    ControlPoint,
}

/// Error depicting errors that occur while managing forecasts
/// 
#[derive(Debug, Error)]
#[error("ForecastValuesError: {0}")]
pub enum ForecastValuesError {
    #[error("forecast values are empty")]
    EmptyForecastValues,
    #[error("forecast values length does not equal 24")]
    WrongForecastLength,
}