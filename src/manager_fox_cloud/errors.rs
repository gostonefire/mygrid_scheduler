use thiserror::Error;

#[derive(Error, Debug)]
#[error("FoxCloud error: {0}")]
pub struct FoxError(pub String);
