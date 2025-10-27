use std::fmt::{Display, Formatter};

pub struct MailError(pub String);

impl Display for MailError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result { write!(f, "MailError: {}", self.0) }
}
impl From<lettre::transport::smtp::Error> for MailError {
    fn from(e: lettre::transport::smtp::Error) -> Self { MailError(e.to_string()) }
}
impl From<lettre::address::AddressError> for MailError {
    fn from(e: lettre::address::AddressError) -> Self { MailError(e.to_string()) }
}
impl From<lettre::error::Error> for MailError {
    fn from(e: lettre::error::Error) -> Self { MailError(e.to_string()) }
}