use lettre::{
    message::Mailbox,
    transport::smtp::{authentication::Credentials, AsyncSmtpTransport},
    AsyncTransport, Tokio1Executor, Message,
};
use crate::{
    fail, ok, start,
    config::{Config},
};
use axum::http::StatusCode;
use std::str::FromStr;

pub struct EmailClient {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from:   Mailbox,
    to:     Mailbox,
}

impl EmailClient {
    /// Build an EmailClient from your Config, or return the appropriate axum error.
    pub fn from_config(cfg: &Config) -> Result<Self, (StatusCode, &'static str)> {
        let host = cfg.smtp_host.as_deref().ok_or_else(|| {
            fail!(cfg, "SMTP config error", "SMTP_HOST not configured{}", "")
        })?;
        let user = cfg.smtp_username.clone().unwrap_or_default();
        let pass = cfg.smtp_password.clone().unwrap_or_default();
        let from_addr = cfg.email_from.as_deref().ok_or_else(|| {
            fail!(cfg, "SMTP config error", "EMAIL_FROM not configured{}", "")
        })?;
        let to_addr = cfg.email_to.as_deref().ok_or_else(|| {
            fail!(cfg, "SMTP config error", "EMAIL_TO not configured{}", "")
        })?;

        let from = Mailbox::from_str(from_addr).expect("valid EMAIL_FROM");
        let to   = Mailbox::from_str(to_addr).expect("valid EMAIL_TO");

        let creds  = Credentials::new(user, pass);
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(host)
            .map_err(|e| {
                fail!(cfg, "SMTP config error", "SMTP relay config failed: {}", e)
            })?
            .credentials(creds)
            .build();

        Ok(EmailClient { mailer, from, to })
    }

    /// Send an HTML email with the given subject and HTML body.
    pub async fn send_html(
        &self,
        subject: &str,
        html_body: String,
        cfg: &Config,
    ) -> Result<(), (StatusCode, &'static str)> {
        let email = Message::builder()
            .from(self.from.clone())
            .to(self.to.clone())
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .subject(subject)
            .body(html_body)
            .map_err(|e| {
                fail!(cfg, "Email build error", "Failed to build email: {}", e)
            })?;

        self.mailer
            .send(email)
            .await
            .map_err(|e| {
                fail!(cfg, "Email send error", "Failed to send email: {}", e)
            })
            .map(|_| ok!(cfg, "Email '{}' sent successfully to {:?}", subject, self.to))
    }
}