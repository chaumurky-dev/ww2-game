use std::sync::{Arc, Mutex};

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Sends mail over SMTP (Mailpit locally, SES in prod). With no `SMTP_HOST` it only logs.
/// An in-memory outbox can be enabled for tests.
#[derive(Clone)]
pub struct Mailer {
    transport: Option<AsyncSmtpTransport<Tokio1Executor>>,
    from: String,
    outbox: Option<Arc<Mutex<Vec<Email>>>>,
}

impl Mailer {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let transport = match &cfg.smtp_host {
            Some(host) => {
                let mut b = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
                    .port(cfg.smtp_port);
                if let (Some(u), Some(p)) = (&cfg.smtp_user, &cfg.smtp_pass) {
                    b = b.credentials(Credentials::new(u.clone(), p.clone()));
                }
                Some(b.build())
            }
            None => None,
        };
        Ok(Self {
            transport,
            from: cfg.smtp_from.clone(),
            outbox: None,
        })
    }

    /// A mailer that records messages instead of sending them.
    pub fn capture() -> (Self, Arc<Mutex<Vec<Email>>>) {
        let outbox = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                transport: None,
                from: "Wartide <noreply@localhost>".into(),
                outbox: Some(outbox.clone()),
            },
            outbox,
        )
    }

    /// Queue an email; delivery happens in a background task and failures are only logged.
    pub fn send(&self, to: &str, subject: &str, body: &str) {
        let email = Email {
            to: to.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
        };
        if let Some(o) = &self.outbox {
            o.lock().unwrap().push(email.clone());
        }
        let Some(transport) = self.transport.clone() else {
            tracing::info!(to = %email.to, subject = %email.subject, "email (not sent, no SMTP configured)");
            return;
        };
        let from = self.from.clone();
        tokio::spawn(async move {
            let build = || -> anyhow::Result<Message> {
                let from: Mailbox = from.parse()?;
                let to: Mailbox = email.to.parse()?;
                Ok(Message::builder()
                    .from(from)
                    .to(to)
                    .subject(email.subject.clone())
                    .body(email.body.clone())?)
            };
            match build() {
                Ok(msg) => {
                    if let Err(e) = transport.send(msg).await {
                        tracing::error!(error = ?e, to = %email.to, "failed to send email");
                    }
                }
                Err(e) => tracing::error!(error = ?e, "failed to build email"),
            }
        });
    }
}
