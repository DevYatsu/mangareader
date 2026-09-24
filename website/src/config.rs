//! Site configuration driven by environment variables.
//!
//! # Env vars read by Rust
//!
//! | Variable | Default | Description |
//! |----------|---------|-------------|
//! | `SITE_TITLE` | `MangaRead` | Site name in header / `<title>` |
//! | `SITE_COLOR` | `#f59e0b` | Primary accent color (hex) |
//! | `SITE_BG` | `#131210` | Page background color |
//!
//! Shipped CSS receives the admin color through shared Bulma component
//! variables in [`SiteConfig::theme_overrides`]. Font and layout tokens live
//! in the shared identity stylesheet.

/// All user-facing settings derived from environment variables.
pub struct SiteConfig {
    pub site_title: String,
    pub accent: String,
    pub bg_page: String,
    /// Red component of accent (0-255) for rgba() usage in CSS.
    pub accent_r: u8,
    /// Green component of accent (0-255).
    pub accent_g: u8,
    /// Blue component of accent (0-255).
    pub accent_b: u8,
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

/// Convert a `#rrggbb` string to `(hue_degrees, saturation_percent, lightness_percent)`.
///
/// Used to translate the admin accent and background colours into the
/// hue/saturation/lightness components Bulma derives its palettes from.
///
/// A wrong-length string returns `(0.0, 0.0, 0.0)`. A right-length string
/// whose byte pairs are not valid hex inherits `parse_hex`'s fallback: each
/// unparseable pair becomes `0`, so `"ffzzzz"` is treated as `#ff0000` rather
/// than rejected. Only the length check guarantees all-zero output.
pub fn hex_to_hsl(hex: &str) -> (f64, f64, f64) {
    let (r, g, b) = parse_hex(hex);
    if hex.trim_start_matches('#').len() != 6 {
        return (0.0, 0.0, 0.0);
    }
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l * 100.0);
    }
    let d = max - min;
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s * 100.0, l * 100.0)
}

impl SiteConfig {
    /// Build from environment variables with sensible defaults.
    pub fn from_env() -> Self {
        let accent = std::env::var("SITE_COLOR").unwrap_or_else(|_| "#f59e0b".into());
        let bg_page = std::env::var("SITE_BG").unwrap_or_else(|_| "#131210".into());
        let (accent_r, accent_g, accent_b) = parse_hex(&accent);

        Self {
            site_title: std::env::var("SITE_TITLE").unwrap_or_else(|_| "MangaRead".into()),
            accent,
            bg_page,
            accent_r,
            accent_g,
            accent_b,
        }
    }

    /// CSS fragment carrying admin-derived colour overrides.
    ///
    /// Emitted inside `:root[data-theme="dark"]` (specificity 0,2,0) so these
    /// beat the static identity tokens in `theme.css` on specificity rather
    /// than on document order. Only the hue/saturation/lightness components
    /// are written: Bulma derives its palettes from them, so the composed
    /// colours recompute. `--bulma-primary-rgb` is a literal in Bulma and is
    /// therefore written by hand.
    ///
    /// The reader consumes the same Bulma primary/surface tokens as every
    /// other page. No legacy color aliases are emitted.
    pub fn theme_overrides(&self) -> String {
        let (h, s, l) = hex_to_hsl(&self.accent);
        let (_, _, bg_l) = hex_to_hsl(&self.bg_page);
        // Elevation steps stay proportional to the chosen page lightness so
        // the surface hierarchy survives any admin background.
        let bis = (bg_l + 2.0).min(100.0);
        let ter = (bg_l + 4.0).min(100.0);
        let background = (bg_l + 6.0).min(100.0);
        let (ar, ag, ab) = (self.accent_r, self.accent_g, self.accent_b);
        // Whole-number precision matches the identity tokens in `theme.css`
        // (`38deg` / `92%` / `50%`), so the env defaults reproduce the
        // designed palette exactly rather than drifting by fractions of a
        // degree. `hex_to_hsl` stays exact; rounding happens at emission.
        format!(
            ":root[data-theme=\"dark\"]{{--bulma-primary-h:{h:.0}deg;--bulma-primary-s:{s:.0}%;\
--bulma-primary-l:{l:.0}%;--bulma-primary-rgb:{ar},{ag},{ab};--bulma-scheme-main-l:{bg_l:.0}%;\
--bulma-scheme-main-bis-l:{bis:.0}%;--bulma-scheme-main-ter-l:{ter:.0}%;--bulma-background-l:{background:.0}%;}}"
        )
    }

    /// Overlay DB-backed settings on top of the env boot defaults.
    ///
    /// Applies `site_title`, `site_accent` (recomputing its RGB components),
    /// and `site_bg` when present and non-empty. Invalid hex falls back to the
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

    #[test]
    fn hex_to_hsl_converts_ember() {
        let (h, s, l) = hex_to_hsl("#f59e0b");
        assert!((h - 38.0).abs() < 0.5, "hue was {h}");
        assert!((s - 92.0).abs() < 0.5, "saturation was {s}");
        assert!((l - 50.0).abs() < 0.5, "lightness was {l}");
    }

    #[test]
    fn hex_to_hsl_handles_greys_and_bad_input() {
        let (_h, s, l) = hex_to_hsl("#131210");
        // The identity surface is a warm near-neutral (hsl(40 8.57% 6.86%)),
        // not a pure grey: 8% is the designed scheme saturation.
        assert!(s < 10.0, "expected a near-neutral saturation, got {s}");
        assert!((l - 7.0).abs() < 0.5, "lightness was {l}");
        // A true grey exercises the achromatic early-return (`max == min`),
        // which the warm #131210 above never reaches.
        let (gh, gs, gl) = hex_to_hsl("#808080");
        assert_eq!((gh, gs), (0.0, 0.0), "grey must report zero hue/saturation");
        assert!((gl - 50.2).abs() < 0.5, "grey lightness was {gl}");
        assert_eq!(hex_to_hsl("nonsense"), (0.0, 0.0, 0.0));
    }

    #[test]
    fn overrides_emit_the_exact_dark_scheme_block() {
        let cfg = SiteConfig {
            site_title: "T".into(),
            accent: "#f59e0b".into(),
            bg_page: "#131210".into(),
            accent_r: 245,
            accent_g: 158,
            accent_b: 11,
        };
        // The exact string IS the contract: one assertion locks the selector,
        // the emission format, and the primary/surface components together.
        // The expected value below is the default config's output, which
        // reproduces `theme.css` exactly.
        assert_eq!(
            cfg.theme_overrides(),
            concat!(
                ":root[data-theme=\"dark\"]{--bulma-primary-h:38deg;--bulma-primary-s:92%;",
                "--bulma-primary-l:50%;--bulma-primary-rgb:245,158,11;",
                "--bulma-scheme-main-l:7%;--bulma-scheme-main-bis-l:9%;",
                "--bulma-scheme-main-ter-l:11%;--bulma-background-l:13%;}"
            ),
        );
    }

    #[test]
    fn overrides_carry_the_configured_accent() {
        // A non-default accent still reaches the shared Bulma primary tokens.
        let cfg = SiteConfig {
            site_title: "T".into(),
            accent: "#123456".into(),
            bg_page: "#0a0b0c".into(),
            accent_r: 18,
            accent_g: 52,
            accent_b: 86,
        };
        let css = cfg.theme_overrides();
        assert!(css.contains("--bulma-primary-rgb:18,52,86;"), "got {css}");
        assert!(!css.contains("legacy"), "got {css}");
    }
}

#[cfg(test)]
mod contrast {
    //! WCAG contrast guard for the Ink & Ember identity tokens.
    //!
    //! The token values live in `static/theme.css` because that is what the
    //! browser consumes; this test reads the shipped file so a token edit that
    //! breaks contrast fails `cargo test` instead of shipping.
    //!
    //! Contract: every token this test reads must sit on its own line as
    //! `--name: value;`.

    const THEME_CSS: &str = include_str!("../static/theme.css");

    /// Read a `--name: value;` declaration written one-per-line.
    fn token(name: &str) -> String {
        let needle = format!("--{name}:");
        THEME_CSS
            .lines()
            .find_map(|line| {
                let t = line.trim();
                t.strip_prefix(&needle)
                    .map(|v| v.trim().trim_end_matches(';').trim().to_string())
            })
            .unwrap_or_else(|| panic!("token --{name} not found on its own line in theme.css"))
    }

    fn angle(raw: &str) -> f64 {
        raw.trim_end_matches("deg")
            .parse()
            .unwrap_or_else(|_| panic!("expected an angle, got {raw}"))
    }

    fn percent(raw: &str) -> f64 {
        raw.trim_end_matches('%')
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("expected a percentage, got {raw}"))
            / 100.0
    }

    /// HSL to linear-light RGB channels, then to WCAG relative luminance.
    fn luminance(h_deg: f64, s: f64, l: f64) -> f64 {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h_prime = h_deg / 60.0;
        let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
        let (r1, g1, b1) = match h_prime as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        let channel = |v: f64| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r1 + m) + 0.7152 * channel(g1 + m) + 0.0722 * channel(b1 + m)
    }

    fn scheme_luminance(lightness_pct: f64) -> f64 {
        luminance(
            angle(&token("bulma-scheme-h")),
            percent(&token("bulma-scheme-s")),
            lightness_pct,
        )
    }

    fn ratio(a: f64, b: f64) -> f64 {
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    fn scheme_main() -> f64 {
        scheme_luminance(percent(&token("bulma-scheme-main-l")))
    }

    /// Text composes from `--bulma-text-h`/`-s`, which is NOT necessarily the
    /// scheme hue. The guard must model the color the browser paints, or it
    /// over-reports contrast against warm surfaces.
    fn text_luminance(lightness: f64) -> f64 {
        luminance(
            angle(&token("bulma-text-h")),
            percent(&token("bulma-text-s")),
            lightness,
        )
    }

    /// The deepest surface the design defines; `.m-empty` puts
    /// `--bulma-text-weak` on it, so it is the muted-text worst case.
    fn background_luminance() -> f64 {
        scheme_luminance(percent(&token("bulma-background-l")))
    }

    #[test]
    fn body_text_meets_wcag_aa() {
        let r = ratio(
            text_luminance(percent(&token("bulma-text-l"))),
            scheme_main(),
        );
        assert!(r >= 4.5, "--bulma-text-l contrast is {r:.2}:1, need >= 4.5");
    }

    #[test]
    fn muted_text_meets_wcag_aa() {
        let r = ratio(
            text_luminance(percent(&token("bulma-text-weak-l"))),
            background_luminance(),
        );
        assert!(
            r >= 4.5,
            "--bulma-text-weak-l contrast is {r:.2}:1, need >= 4.5"
        );
    }

    /// §4.1 requires greys to read as warm ink. That is now enforced, so a
    /// future edit cannot silently regress text to Bulma's cool 221deg.
    #[test]
    fn text_hue_matches_the_warm_scheme() {
        assert_eq!(
            angle(&token("bulma-text-h")),
            angle(&token("bulma-scheme-h")),
            "text hue must match the warm scheme hue"
        );
    }

    /// Bulma paints anchors from --bulma-link-*, whose factory hue is the blue
    /// 233deg. theme.css binds it to the accent; this locks the binding, so a
    /// future edit cannot silently regress links to Bulma's blue.
    #[test]
    fn link_hue_follows_the_ember_accent() {
        // Unlike the other angle readers, this token is bound with `var()`
        // rather than a literal, so resolve the reference first. A hardcoded
        // literal is itself the regression this guards against.
        let link = token("bulma-link-h");
        let inner = link
            .strip_prefix("var(")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or_else(|| panic!("--bulma-link-h must bind with var(...), got {link}"));
        assert_eq!(
            angle(&token(inner.trim_start_matches("--"))),
            angle(&token("bulma-primary-h")),
            "link hue must follow the ember accent hue"
        );
    }

    #[test]
    fn ember_accent_meets_wcag_aa_on_the_page_surface() {
        let accent = luminance(
            angle(&token("bulma-primary-h")),
            percent(&token("bulma-primary-s")),
            percent(&token("bulma-primary-l")),
        );
        let r = ratio(accent, scheme_main());
        assert!(r >= 4.5, "ember accent contrast is {r:.2}:1, need >= 4.5");
    }

    #[test]
    fn surface_elevation_separates_from_the_page() {
        // Raised surfaces must stay distinguishable from the page background.
        let r = ratio(
            scheme_luminance(percent(&token("bulma-scheme-main-bis-l"))),
            scheme_main(),
        );
        assert!(
            r > 1.0,
            "scheme-main-bis is indistinguishable from scheme-main"
        );
    }
}
