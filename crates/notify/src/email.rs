//! SMTP email notifier via `lettre` with TLS support.
//!
//! Delivers notifications as emails through an SMTP server.
//! Supports STARTTLS and implicit TLS connections.

use crate::traits::{Notification, Notifier, NotifyError};
use lettre::{
    message::Mailbox, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};

/// Sends notifications as emails via SMTP.
#[derive(Debug)]
pub struct EmailNotifier {
    /// Async SMTP transport for sending emails.
    transport: AsyncSmtpTransport<Tokio1Executor>,
    /// Sender mailbox.
    from: Mailbox,
    /// Recipient mailboxes.
    to: Vec<Mailbox>,
}

impl EmailNotifier {
    /// Build an `EmailNotifier` from SMTP configuration.
    ///
    /// - `smtp_host`: SMTP server hostname.
    /// - `smtp_port`: Optional port (defaults to 587, or 465 for implicit TLS).
    /// - `tls`: Whether to use TLS. `None` or `Some(true)` enables STARTTLS;
    ///   port 465 always uses implicit TLS regardless of this flag.
    /// - `from`: Sender email address (e.g. `"alerts@example.com"` or `"Alerts <alerts@example.com>"`).
    /// - `to`: Recipient email addresses.
    ///
    /// SMTP credentials are resolved from the `SMTP_USERNAME` and `SMTP_PASSWORD`
    /// environment variables. If both are set, they are passed to the transport;
    /// otherwise the connection is unauthenticated.
    pub fn from_config(
        smtp_host: &str,
        smtp_port: Option<u16>,
        tls: Option<bool>,
        from: &str,
        to: &[String],
    ) -> Result<Self, NotifyError> {
        let from_mailbox: Mailbox = from
            .parse()
            .map_err(|e: lettre::address::AddressError| NotifyError::Config(e.to_string()))?;

        let to_mailboxes: Vec<Mailbox> = to
            .iter()
            .map(|addr| {
                addr.parse()
                    .map_err(|e: lettre::address::AddressError| NotifyError::Config(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if to_mailboxes.is_empty() {
            return Err(NotifyError::Config(
                "at least one recipient is required".to_string(),
            ));
        }

        let port = smtp_port.unwrap_or(587);
        let use_tls = tls.unwrap_or(true);

        // Port 465 uses implicit TLS; everything else uses STARTTLS when TLS is enabled.
        let mut builder = if port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_host)
                .map_err(|e| NotifyError::Config(e.to_string()))?
                .port(port)
        } else if use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_host)
                .map_err(|e| NotifyError::Config(e.to_string()))?
                .port(port)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host).port(port)
        };

        // Attach credentials from environment if available.
        if let (Ok(username), Ok(password)) =
            (std::env::var("SMTP_USERNAME"), std::env::var("SMTP_PASSWORD"))
        {
            builder = builder.credentials(Credentials::new(username, password));
        }

        let transport = builder.build();

        Ok(Self {
            transport,
            from: from_mailbox,
            to: to_mailboxes,
        })
    }
}

#[async_trait::async_trait]
impl Notifier for EmailNotifier {
    /// Send a notification email to all configured recipients.
    async fn send(&self, notification: &Notification) -> Result<(), NotifyError> {
        let mut message_builder = Message::builder().from(self.from.clone());

        for recipient in &self.to {
            message_builder = message_builder.to(recipient.clone());
        }

        let email = message_builder
            .subject(&notification.subject)
            .body(notification.body.clone())
            .map_err(|e| NotifyError::Smtp(e.to_string()))?;

        self.transport
            .send(email)
            .await
            .map_err(|e| NotifyError::Smtp(e.to_string()))?;

        tracing::info!(
            channel = "email",
            subject = %notification.subject,
            recipients = self.to.len(),
            "notification delivered"
        );

        Ok(())
    }

    /// Returns `"email"`.
    fn channel_name(&self) -> &str {
        "email"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_email_address() {
        let mailbox: Result<Mailbox, _> = "alice@example.com".parse();
        assert!(mailbox.is_ok());
    }

    #[test]
    fn parse_email_with_display_name() {
        let mailbox: Result<Mailbox, _> = "Alice <alice@example.com>".parse();
        assert!(mailbox.is_ok());
        let mb = mailbox.unwrap();
        assert_eq!(mb.email.to_string(), "alice@example.com");
    }

    #[test]
    fn parse_invalid_email_address() {
        let mailbox: Result<Mailbox, _> = "not-an-email".parse();
        assert!(mailbox.is_err());
    }

    #[test]
    fn from_config_valid() {
        let notifier = EmailNotifier::from_config(
            "smtp.example.com",
            Some(587),
            Some(true),
            "alerts@example.com",
            &["admin@example.com".to_string()],
        );
        assert!(notifier.is_ok());
    }

    #[test]
    fn from_config_invalid_from_address() {
        let result = EmailNotifier::from_config(
            "smtp.example.com",
            None,
            None,
            "bad-address",
            &["admin@example.com".to_string()],
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Configuration error"), "got: {err}");
    }

    #[test]
    fn from_config_invalid_to_address() {
        let result = EmailNotifier::from_config(
            "smtp.example.com",
            None,
            None,
            "alerts@example.com",
            &["not-valid".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn from_config_empty_recipients() {
        let result =
            EmailNotifier::from_config("smtp.example.com", None, None, "alerts@example.com", &[]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("at least one recipient"),
            "got: {err}"
        );
    }

    #[test]
    fn channel_name_is_email() {
        let notifier = EmailNotifier::from_config(
            "smtp.example.com",
            Some(587),
            Some(true),
            "alerts@example.com",
            &["admin@example.com".to_string()],
        )
        .unwrap();
        assert_eq!(notifier.channel_name(), "email");
    }

    #[test]
    fn from_config_implicit_tls_port() {
        let notifier = EmailNotifier::from_config(
            "smtp.example.com",
            Some(465),
            None,
            "alerts@example.com",
            &["admin@example.com".to_string()],
        );
        assert!(notifier.is_ok());
    }

    #[test]
    fn from_config_no_tls() {
        let notifier = EmailNotifier::from_config(
            "smtp.example.com",
            Some(25),
            Some(false),
            "alerts@example.com",
            &["admin@example.com".to_string()],
        );
        assert!(notifier.is_ok());
    }
}
