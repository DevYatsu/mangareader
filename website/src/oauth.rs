//! Hand-rolled OAuth2 code flow for google/github/discord (no oauth2 crate).
//!
//! Pure URL/config logic lives here so it is unit-testable; the HTTP token
//! exchange and userinfo fetch use `reqwest` and are exercised via curl in
//! the task report (real providers cannot be scripted in CI).

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Google,
    Github,
    Discord,
}

impl Provider {
    pub fn key(&self) -> &'static str {
        match self {
            Provider::Google => "google",
            Provider::Github => "github",
            Provider::Discord => "discord",
        }
    }
    pub fn from_key(s: &str) -> Option<Provider> {
        match s {
            "google" => Some(Provider::Google),
            "github" => Some(Provider::Github),
            "discord" => Some(Provider::Discord),
            _ => None,
        }
    }
    fn env_prefix(&self) -> &'static str {
        match self {
            Provider::Google => "GOOGLE",
            Provider::Github => "GITHUB",
            Provider::Discord => "DISCORD",
        }
    }
    fn auth_endpoint(&self) -> &'static str {
        match self {
            Provider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Provider::Github => "https://github.com/login/oauth/authorize",
            Provider::Discord => "https://discord.com/oauth2/authorize",
        }
    }
    fn token_endpoint(&self) -> &'static str {
        match self {
            Provider::Google => "https://oauth2.googleapis.com/token",
            Provider::Github => "https://github.com/login/oauth/access_token",
            Provider::Discord => "https://discord.com/api/oauth2/token",
        }
    }
    fn userinfo_endpoint(&self) -> &'static str {
        match self {
            Provider::Google => "https://openidconnect.googleapis.com/v1/userinfo",
            Provider::Github => "https://api.github.com/user",
            Provider::Discord => "https://discord.com/api/users/@me",
        }
    }
    fn scope(&self) -> &'static str {
        match self {
            Provider::Google => "openid email profile",
            Provider::Github => "read:user user:email",
            Provider::Discord => "identify email",
        }
    }
}

/// Client id/secret pair, `None` when the provider is not configured.
pub fn credentials(p: Provider) -> Option<(String, String)> {
    let id = std::env::var(format!("MANGAREAD_OAUTH_{}_ID", p.env_prefix())).ok()?;
    let secret = std::env::var(format!("MANGAREAD_OAUTH_{}_SECRET", p.env_prefix())).ok()?;
    if id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((id, secret))
}

/// Providers with credentials present. Drives the login-page buttons.
pub fn configured_providers() -> Vec<Provider> {
    [Provider::Google, Provider::Github, Provider::Discord]
        .into_iter()
        .filter(|p| credentials(*p).is_some())
        .collect()
}

pub fn base_url() -> String {
    std::env::var("MANGAREAD_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

fn url_encode(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Authorize URL for the provider. Pure function, unit-tested.
pub fn authorize_url(p: &Provider, base: &str, state: &str, client_id: &str) -> String {
    let redirect = format!("{base}/auth/{}/callback", p.key());
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        p.auth_endpoint(),
        url_encode(client_id),
        url_encode(&redirect),
        url_encode(p.scope()),
        url_encode(state),
    )
}

/// Normalized profile every provider maps into.
pub struct OAuthProfile {
    pub provider_user_id: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub display_name: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

pub async fn exchange_code(
    p: Provider,
    client_id: &str,
    client_secret: &str,
    code: &str,
    base: &str,
) -> Result<String, String> {
    let redirect = format!("{base}/auth/{}/callback", p.key());
    let client = reqwest::Client::new();
    let resp = client
        .post(p.token_endpoint())
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect.as_str()),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body: TokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.access_token)
}

#[derive(Deserialize, Default)]
struct GoogleUser {
    #[serde(default)]
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: bool,
    #[serde(default)]
    name: Option<String>,
}
#[derive(Deserialize, Default)]
struct GithubUser {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    login: String,
}
#[derive(Deserialize)]
struct GithubEmail {
    email: String,
    primary: bool,
    verified: bool,
}
#[derive(Deserialize, Default)]
struct DiscordUser {
    #[serde(default)]
    id: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    verified: bool,
}

pub async fn fetch_profile(p: Provider, access_token: &str) -> Result<OAuthProfile, String> {
    let client = reqwest::Client::builder()
        .user_agent("mangaread-oauth/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    match p {
        Provider::Google => {
            let u: GoogleUser = client
                .get(p.userinfo_endpoint())
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            Ok(OAuthProfile {
                provider_user_id: u.sub,
                email: u.email.clone(),
                email_verified: u.email_verified && u.email.is_some(),
                display_name: u.name.unwrap_or_else(|| "reader".to_string()),
            })
        }
        Provider::Github => {
            let u: GithubUser = client
                .get(p.userinfo_endpoint())
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let emails: Vec<GithubEmail> = client
                .get("https://api.github.com/user/emails")
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            let pick = emails
                .iter()
                .find(|e| e.primary && e.verified)
                .or_else(|| emails.iter().find(|e| e.verified));
            Ok(OAuthProfile {
                provider_user_id: u.id.to_string(),
                email: pick.map(|e| e.email.clone()),
                email_verified: pick.is_some(),
                display_name: u.login,
            })
        }
        Provider::Discord => {
            let u: DiscordUser = client
                .get(p.userinfo_endpoint())
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .json()
                .await
                .map_err(|e| e.to_string())?;
            Ok(OAuthProfile {
                provider_user_id: u.id,
                email: u.email.clone(),
                email_verified: u.verified && u.email.is_some(),
                display_name: u.username,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_authorize_url_shape() {
        let url = authorize_url(&Provider::Google, "https://app.test", "state-9", "cid-1");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("client_id=cid-1"));
        assert!(url.contains("state=state-9"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapp.test%2Fauth%2Fgoogle%2Fcallback"));
    }
}
