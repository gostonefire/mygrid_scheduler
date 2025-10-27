pub mod errors;

use lettre::message::Mailbox;
use lettre::{Message, SmtpTransport, Transport};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use crate::config::MailParameters;
use crate::manager_mail::errors::MailError;

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
        let sender = SmtpTransport::relay(&config.smtp_endpoint)?
            .credentials(credentials)
            .build();

        let from = config.from.parse::<Mailbox>()?;
        let to = config.to.parse::<Mailbox>()?;

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
            .body(body)?;

        self.sender.send(&message)?;

        Ok(())
    }
}