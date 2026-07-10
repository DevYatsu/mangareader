//! Site configuration driven by environment variables.
//!
//! # Overridable settings
//!
//! | Variable | Default | Description |
//! |----------|---------|-------------|
//! | `SITE_TITLE` | `MangaRead` | Site name in header / `<title>` |
//! | `SITE_COLOR` | `#f59e0b` | Primary accent color (hex) |
//! | `SITE_COLOR_HOVER` | *(derived)* | Hover variant (lightened) |
//! | `SITE_BG` | `#0f0f0f` | Page background color |

/// All user-facing settings derived from environment variables.
pub struct SiteConfig {
    pub site_title: String,
    pub accent: String,
    pub accent_hover: String,
    pub bg_page: String,
}

/// Lighten a hex color by blending it toward white.
fn lighten(hex: &str, amount: f32) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return hex.to_string();
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    let lerp = |a: u8, t: f32| (a as f32 + (255.0 - a as f32) * t) as u8;
    format!("#{:02x}{:02x}{:02x}", lerp(r, amount), lerp(g, amount), lerp(b, amount))
}

impl SiteConfig {
    /// Build from environment variables with sensible defaults.
    pub fn from_env() -> Self {
        let accent = std::env::var("SITE_COLOR").unwrap_or_else(|_| "#f59e0b".into());
        let bg_page = std::env::var("SITE_BG").unwrap_or_else(|_| "#0f0f0f".into());

        Self {
            site_title: std::env::var("SITE_TITLE").unwrap_or_else(|_| "MangaRead".into()),
            accent_hover: std::env::var("SITE_COLOR_HOVER")
                .unwrap_or_else(|_| lighten(&accent, 0.25)),
            accent,
            bg_page,
        }
    }

    /// Generate a `<style>` block overriding CSS custom properties.
    pub fn css_overrides(&self) -> String {
        format!(
            ":root{{--accent:{};--accent-hover:{};--accent-glow:{}33;--accent-dim:{}14;--bg-page:{};}}",
            self.accent,
            self.accent_hover,
            self.accent,
            self.accent,
            self.bg_page,
        )
    }
}
