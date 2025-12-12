use thiserror::Error;

#[derive(Error, Debug)]
pub enum NordPoolError {
    #[error("error parsing document: {0}")]   
    DocumentError(#[from] serde_json::Error),
    #[error("ureq error: {0}")]  
    NetworkError(#[from] ureq::Error),
    #[error("no content for the requested time period")]   
    NoContentError,
    #[error("content length error")] 
    ContentLengthError,
}
