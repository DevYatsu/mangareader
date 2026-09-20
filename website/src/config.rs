//! Site configuration driven by environment variables.
//!
//! # Env vars read by Rust
//!
//! | Variable | Default | Description |
//! |----------|---------|-------------|
//! | `SITE_TITLE` | `MangaRead` | Site name in header / `<title>` |
//! | `SITE_COLOR` | `#f59e0b` | Primary accent color (hex) |
//! | `SITE_BG` | `#0f0f0f` | Page background color |
//!
//! Colors `accent_hover`, `accent_dim`, `accent_glow` are auto-derived.
//! Font, max-width, and other layout tokens are set in CSS variables
//! (see `:root` in `style.css`) and can be overridden via a local
//! override stylesheet or inline `<style>`.

/// All user-facing settings derived from environment variables.
pub struct SiteConfig {
    pub site_title: String,
    pub accent: String,
    pub accent_hover: String,
    pub bg_page: String,
    /// Red component of accent (0-255) for rgba() usage in CSS.
    pub accent_r: u8,
    /// Green component of accent (0-255).
    pub accent_g: u8,
    /// Blue component of accent (0-255).
    pub accent_b: u8,
}

/// Single allowlist of theme names, shared by the layout link builder,
/// the settings/appearance validation, and the admin theme picker.
///
/// The first element of each pair is the theme key (stylesheet file stem
/// and `theme-{key}` body class); the second is the human-readable label.
pub const THEMES: &[(&str, &str)] = &[
    ("default", "Default dark"),
    ("webtoon", "Webtoon clean"),
    ("asura", "Asura dark"),
    ("demonic", "Demonic crimson"),
];

/// Stylesheet URL for a theme, or `None` when no extra stylesheet applies.
///
/// Only `webtoon`/`asura`/`demonic` ship a file; `default`, empty, and
/// unknown names fall back to the base stylesheet (current look preserved).
pub fn theme_css(theme: &str) -> Option<String> {
    match theme {
        "webtoon" | "asura" | "demonic" => Some(format!("/static/themes/{theme}.css")),
        _ => None,
    }
}

/// Whether `theme` is an allowlisted [`THEMES`] key.
pub fn is_theme(theme: &str) -> bool {
    THEMES.iter().any(|(key, _)| *key == theme)
}

/// Whether `s` is a usable `#rrggbb` hex color (leading `#` optional).
fn is_valid_hex(s: &str) -> bool {
    let hex = s.trim().trim_start_matches('#');
    hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Normalize a validated hex color to lowercase with a leading `#`.
fn normalize_hex(s: &str) -> String {
    let hex = s.trim().trim_start_matches('#').to_lowercase();
    format!("#{hex}")
}

/// Parse a `#rrggbb` hex string into its RGB components.
fn parse_hex(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return (0, 0, 0);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    (r, g, b)
}

/// Lighten a hex color by blending it toward white.
fn lighten(hex: &str, amount: f32) -> String {
    let (r, g, b) = parse_hex(hex);
    let lerp = |a: u8, t: f32| (a as f32 + (255.0 - a as f32) * t) as u8;
    format!("#{:02x}{:02x}{:02x}", lerp(r, amount), lerp(g, amount), lerp(b, amount))
}

impl SiteConfig {
    /// Build from environment variables with sensible defaults.
    pub fn from_env() -> Self {
        let accent = std::env::var("SITE_COLOR").unwrap_or_else(|_| "#f59e0b".into());
        let bg_page = std::env::var("SITE_BG").unwrap_or_else(|_| "#0f0f0f".into());
        let (accent_r, accent_g, accent_b) = parse_hex(&accent);

        Self {
            site_title: std::env::var("SITE_TITLE").unwrap_or_else(|_| "MangaRead".into()),
            accent_hover: lighten(&accent, 0.25),
            accent,
            bg_page,
            accent_r,
            accent_g,
            accent_b,
        }
    }

    /// Generate a `<style>` block overriding CSS custom properties
    /// for accent colors (everything else lives in `style.css`'s `:root`).
    pub fn css_overrides(&self) -> String {
        format!(
            ":root{{--accent:{};--accent-hover:{};--accent-dim:{}14;--bg-page:{};--accent-r:{};--accent-g:{};--accent-b:{};}}",
            self.accent,
            self.accent_hover,
            self.accent,
            self.bg_page,
            self.accent_r,
            self.accent_g,
            self.accent_b,
        )
    }

    /// Overlay DB-backed settings on top of the env boot defaults.
    ///
    /// Applies `site_title`, `site_accent` (recomputing hover + rgb), and
    /// `site_bg` when present and non-empty. Invalid hex falls back to the
    /// current value (never blank).
    pub fn from_settings(map: &std::collections::HashMap<String, String>) -> Self {
        let mut cfg = Self::from_env();
        if let Some(title) = map.get("site_title") {
            if !title.trim().is_empty() {
                cfg.site_title = title.clone();
            }
        }
        if let Some(accent) = map.get("site_accent") {
            if !accent.trim().is_empty() && is_valid_hex(accent) {
                cfg.accent = normalize_hex(accent);
                cfg.accent_hover = lighten(&cfg.accent, 0.25);
                let (r, g, b) = parse_hex(&cfg.accent);
                cfg.accent_r = r;
                cfg.accent_g = g;
                cfg.accent_b = b;
            }
        }
        if let Some(bg) = map.get("site_bg") {
            if !bg.trim().is_empty() && is_valid_hex(bg) {
                cfg.bg_page = normalize_hex(bg);
            }
        }
        cfg
    }
}

/// Load the effective site config: env boot defaults overlaid with the
/// `site_settings` table. Falls back to `from_env()` on DB error — a
/// settings read must never 500 a page.
pub fn site_config(conn: &rusqlite::Connection) -> SiteConfig {
    match mangaread_core::db::all_settings(conn) {
        Ok(map) => SiteConfig::from_settings(&map),
        Err(_) => SiteConfig::from_env(),
    }
}

/// Resolve the active theme key for a request.
///
/// The user's own `theme` wins when non-empty and allowlisted; otherwise
/// the `site_theme` setting applies; otherwise `"default"`.
pub fn resolve_theme(
    conn: &rusqlite::Connection,
    user: Option<&crate::auth::AuthUser>,
) -> String {
    if let Some(u) = user {
        if !u.theme.trim().is_empty() && is_theme(&u.theme) {
            return u.theme.clone();
        }
    }
    match mangaread_core::db::get_setting(conn, "site_theme") {
        Ok(Some(t)) if !t.trim().is_empty() && is_theme(&t) => t,
        _ => "default".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn settings_overlay_beats_env() {
        let mut map = HashMap::new();
        map.insert("site_title".into(), "MyDen".into());
        map.insert("site_accent".into(), "#00dc64".into());
        map.insert("site_accent".into(), "not-a-color".into());
        let c = SiteConfig::from_settings(&map);
        assert_eq!(c.site_title, "MyDen");
        assert_eq!(c.accent, "#f59e0b"); // invalid hex keeps boot default
    }
}
