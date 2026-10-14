//! Outbound email: the [`Mailer`] port, its SMTP adapter, and sign-up policy.
//!
//! Handlers read [`MailSettings`] while holding the DB lock, release the
//! lock, and only then call [`Mailer::send`], so a slow SMTP server never
//! blocks other requests on the database mutex.

use async_trait::async_trait;
use sha2::{Digest, Sha256};

/// SMTP settings resolved from `site_settings` over the environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailSettings {
    /// lettre URL, e.g. `smtps://user:pass@smtp.example.com` or
    /// `smtp://smtp.example.com:587?tls=required`.
    pub url: String,
    /// `From:` header, e.g. `MangaRead <noreply@example.com>`.
    pub from: String,
}

impl MailSettings {
    /// `None` when no SMTP URL is configured anywhere.
    pub fn load(conn: &rusqlite::Connection) -> Option<Self> {
        let url = setting_or_env(conn, "smtp_url", "SMTP_URL")?;
        let from = setting_or_env(conn, "smtp_from", "SMTP_FROM")
            .unwrap_or_else(|| "MangaRead <noreply@localhost>".to_string());
        Some(MailSettings { url, from })
    }
}

/// A non-empty DB setting, else a non-empty env var.
fn setting_or_env(conn: &rusqlite::Connection, key: &str, env: &str) -> Option<String> {
    mangaread_core::db::get_setting(conn, key)
        .ok()
        .flatten()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::env::var(env).ok().filter(|v| !v.trim().is_empty()))
}

/// How `/register` behaves, decided by the admin's email setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignupMode {
    /// Email first; the account is created from the emailed link.
    Email,
    /// No SMTP, and the admin explicitly allowed unverified sign-up.
    Direct,
    /// No SMTP and unverified sign-up is off (the default).
    Closed,
}

pub const ALLOW_UNVERIFIED_KEY: &str = "allow_unverified_signup";

pub fn signup_mode(conn: &rusqlite::Connection) -> SignupMode {
    if MailSettings::load(conn).is_some() {
        SignupMode::Email
    } else if mangaread_core::db::get_setting(conn, ALLOW_UNVERIFIED_KEY)
        .ok()
        .flatten()
        .as_deref()
        == Some("1")
    {
        SignupMode::Direct
    } else {
        SignupMode::Closed
    }
}

/// SHA-256 of a link token, hex-encoded. Only this hash is stored, so a
/// database leak does not hand out working links.
pub fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

/// The outbound-mail port (Strategy pattern): production uses SMTP, tests
/// record messages in memory.
#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(
        &self,
        settings: &MailSettings,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), String>;
}

/// SMTP adapter. Builds a transport per message so admin edits to the
/// settings apply without a restart; at 1-20 users the reconnect cost is
/// negligible.
pub struct SmtpMailer;

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(
        &self,
        settings: &MailSettings,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), String> {
        use lettre::{
            AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
            message::header::ContentType,
        };
        let message = Message::builder()
            .from(
                settings
                    .from
                    .parse()
                    .map_err(|e| format!("from address: {e}"))?,
            )
            .to(to.parse().map_err(|e| format!("to address: {e}"))?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_string())
            .map_err(|e| e.to_string())?;
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(&settings.url)
            .map_err(|e| format!("SMTP URL: {e}"))?
            .timeout(Some(std::time::Duration::from_secs(15)))
            .build();
        transport
            .send(message)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Test adapter: records `(to, subject, body)` instead of sending.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryMailer {
    pub sent: std::sync::Mutex<Vec<(String, String, String)>>,
}

#[cfg(test)]
#[async_trait]
impl Mailer for MemoryMailer {
    async fn send(
        &self,
        _settings: &MailSettings,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), String> {
        self.sent
            .lock()
            .unwrap()
            .push((to.to_string(), subject.to_string(), body.to_string()));
        Ok(())
    }
}

#[cfg(test)]
impl MemoryMailer {
    /// The first `http…/{path}/{token}` link in the newest message to `to`.
    pub fn last_link_to(&self, to: &str, path: &str) -> Option<String> {
        let sent = self.sent.lock().unwrap();
        let (_, _, body) = sent.iter().rev().find(|(t, _, _)| t == to)?;
        let marker = format!("/{path}/");
        let start = body.find(&marker)? + marker.len();
        let token: String = body[start..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        Some(format!("/{path}/{token}"))
    }

    pub fn count_to(&self, to: &str) -> usize {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .filter(|(t, ..)| t == to)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_stable_hex_and_distinct() {
        let h = token_hash("abc");
        assert_eq!(h.len(), 64);
        assert_eq!(h, token_hash("abc"));
        assert_ne!(h, token_hash("abd"));
    }

    #[test]
    fn signup_mode_defaults_to_closed_and_respects_settings() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        mangaread_core::db::init_db(&conn).unwrap();
        // Tests must not depend on the developer's SMTP_URL.
        if std::env::var("SMTP_URL").is_ok() {
            return;
        }
        assert_eq!(signup_mode(&conn), SignupMode::Closed);
        mangaread_core::db::set_setting(&conn, ALLOW_UNVERIFIED_KEY, "1", 1).unwrap();
        assert_eq!(signup_mode(&conn), SignupMode::Direct);
        mangaread_core::db::set_setting(&conn, "smtp_url", "smtp://localhost:2525", 1).unwrap();
        assert_eq!(signup_mode(&conn), SignupMode::Email);
    }
}
