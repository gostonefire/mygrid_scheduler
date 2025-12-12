use thiserror::Error;

#[derive(Error, Debug)]
#[error("FoxCloud error: {0}")]
pub enum FoxError {
    #[error("error getting SoC from Fox Cloud: {0}")]
    GetSocError(String),
    #[error("error posting request to Fox Cloud: {0}")]
    PostRequestError(String),
}
