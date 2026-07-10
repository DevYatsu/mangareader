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
}
