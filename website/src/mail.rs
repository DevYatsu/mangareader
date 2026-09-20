//! Outbound mail. No-op unless SMTP env is configured:
//! `MANGAREAD_SMTP_HOST` (+ optional `MANGAREAD_SMTP_PORT`,
//! `MANGAREAD_SMTP_USER`, `MANGAREAD_SMTP_PASS`, `MANGAREAD_SMTP_FROM`).

pub fn smtp_configured() -> bool {
    std::env::var("MANGAREAD_SMTP_HOST").is_ok_and(|h| !h.is_empty())
}

pub fn send_reset_email_blocking(to: &str, link: &str) -> Result<(), String> {
    if !smtp_configured() {
        return Ok(());
    }
    use lettre::{Message, SmtpTransport, Transport};
    let from =
        std::env::var("MANGAREAD_SMTP_FROM").unwrap_or_else(|_| "noreply@localhost".to_string());
    let email = Message::builder()
        .from(from.parse().map_err(|e| format!("bad from: {e}"))?)
        .to(to.parse().map_err(|e| format!("bad recipient: {e}"))?)
        .subject("Reset your password")
        .body(format!("Reset your password (valid 1 hour):\n\n{link}\n"))
        .map_err(|e| e.to_string())?;
    let host = std::env::var("MANGAREAD_SMTP_HOST").map_err(|e| e.to_string())?;
    let mut builder = SmtpTransport::relay(&host).map_err(|e| e.to_string())?;
    if let Ok(port) = std::env::var("MANGAREAD_SMTP_PORT")
        && let Ok(p) = port.parse::<u16>()
    {
        builder = SmtpTransport::relay(&host)
            .map_err(|e| e.to_string())?
            .port(p);
    }
    let mailer = match std::env::var("MANGAREAD_SMTP_USER") {
        Ok(user) => {
            let pass = std::env::var("MANGAREAD_SMTP_PASS").unwrap_or_default();
            builder
                .credentials(lettre::transport::smtp::authentication::Credentials::new(
                    user, pass,
                ))
                .build()
        }
        Err(_) => builder.build(),
    };
    mailer.send(&email).map_err(|e| e.to_string())?;
    Ok(())
}

/// Async wrapper: the blocking SMTP send runs on the blocking pool.
pub async fn send_reset_email(to: String, link: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || send_reset_email_blocking(&to, &link))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    #[test]
    fn mail_noop_when_unconfigured() {
        // MANGAREAD_SMTP_HOST is unset in test env → no send attempted.
        assert!(!super::smtp_configured());
        assert!(super::send_reset_email_blocking("a@b.io", "http://x/reset/t").is_ok());
    }
}
