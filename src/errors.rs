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

#[derive(Debug, Error)]
#[error("TimeValuesError: {0}")]
pub enum TimeValuesError {
    #[error("Time value does not match date")]
    TimeValue,
}
