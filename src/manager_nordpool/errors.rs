use thiserror::Error;

#[derive(Error, Debug)]
pub enum NordPoolError {
    #[error("error parsing document: {0}")]   
    Document(String),
    #[error("no content for the requested time period")]   
    NoContent,
}
