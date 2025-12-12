use lettre::message::Mailbox;
use lettre::{Message, SmtpTransport, Transport};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use anyhow::Result;
use thiserror::Error;
use crate::config::MailParameters;

pub struct Mail {
    sender: SmtpTransport,
    from: Mailbox,
    to: Mailbox,
}

impl Mail {
    /// Returns a new instance of the Mail struct
    ///
    /// # Arguments
    ///
    /// * 'config' - mail configuration parameters
    pub fn new(config: &MailParameters) -> Result<Self, MailError> {
        let credentials = Credentials::new(config.smtp_user.to_owned(), config.smtp_password.to_owned());
        let sender = SmtpTransport::relay(&config.smtp_endpoint)
            .map_err(|e| MailError::SMTPTransportError(e.to_string()))?
            .credentials(credentials)
            .build();

        let from = config.from.parse::<Mailbox>()
            .map_err(|e| MailError::ParseError(format!("from address: {}", e.to_string())))?;
        let to = config.to.parse::<Mailbox>()
            .map_err(|e| MailError::ParseError(format!("to address: {}", e.to_string())))?;

        Ok(
            Self {
                sender,
                from,
                to,
            }
        )
    }

    /// Sends a mail with the given subject and body
    ///
    /// # Arguments
    ///
    /// * 'subject' - the subject of the mail
    /// * 'body' - the body of the mail
    pub fn send_mail(&self, subject: String, body: String) -> Result<(), MailError> {

        let message = Message::builder()
            .from(self.from.clone())
            .to(self.to.clone())
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)
            .map_err(|e| MailError::MessageError(e.to_string()))?;

        self.sender.send(&message)
            .map_err(|e| MailError::TransportError(e.to_string()))?;

        Ok(())
    }
}

/// Error depicting errors that occur while sending emails
///
#[derive(Debug, Error)]
#[error("error while sending email")]
pub enum MailError {
    #[error("smtp transport error: {0}")]
    SMTPTransportError(String),
    #[error("transport error: {0}")]
    TransportError(String),
    #[error("error parsing email address: {0}")]
    ParseError(String),
    #[error("error compiling message: {0}")]
    MessageError(String),
}