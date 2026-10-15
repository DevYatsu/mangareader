# Bulma Migration — Slice A: Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the Ink & Ember identity and the three-layer CSS cascade, and rebuild the site shell (navbar, footer, error page, auth pages) on Bulma 1.0.4.

**Architecture:** Three plain-CSS layers served by the existing `ServeDir`, no build step: `static/vendor/bulma.min.css` (untouched upstream) → `static/theme.css` (Ink & Ember `--bulma-*` overrides + `--m-*` tokens + `m-*` primitives) → `static/reader.css` (reader-only, later slice). Every Bulma override targets `:root[data-theme="dark"]` (specificity `0,2,0`) so it wins on specificity, not file order.

**Tech Stack:** Rust, Axum 0.8, Maud 0.27, tower-http 0.7, Bulma 1.0.4 (vendored). No Node, no Sass, no CDN.

**Spec:** `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md`

## Global Constraints

- No Node, no Sass, no npm, no CDN at runtime. Bulma is vendored.
- Never change `name=`, `action=`, or `method=` on any form. Class/style attributes only.
- Use Bulma's own `is-*` / `has-*` modifiers for anything Bulma styles. Invent an `m-*` class only for a primitive Bulma does not define, and only from §6 of the spec.
- Override Bulma `-h` / `-s` / `-l` component variables, never the composed color. Set `--bulma-primary-rgb` as a literal because Bulma stores it as one.
- Preserve every `data-r-*` attribute on the reader body exactly. The existing test suite asserts on them.
- Dark only in this pass. Emit `data-theme="dark"` server-side; no client scheme detection.
- This is a styling change. Zero behavior changes.

## Spec deviation to record

Spec §8 stages `resolve_theme()` → `resolve_scheme()` as a rename in Slice E. This plan instead **deletes per-page theme resolution in Slice A**, because Slice A drops the `theme` argument from `base_layout` (the argument sweep is unavoidable once the layout stops emitting a theme link). A `resolve_scheme()` that unconditionally returns `"dark"` would have no consumer, since `data-theme="dark"` is now a constant in the layout. Deleting is strictly simpler than renaming dead code. The `THEMES` allowlist, `theme_css()`, `is_theme()`, and the theme files stay alive until Slices C/E so the settings and admin pages keep compiling.

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `website/static/vendor/bulma.min.css` | Untouched upstream Bulma 1.0.4 | Create (download) |
| `website/static/vendor/README.md` | Records vendored version + provenance | Create |
| `website/static/theme.css` | Ink & Ember identity layer (3 parts) | Create |
| `website/static/style.css` | Legacy stylesheet (2465 lines) | Keep until Slice E |
| `website/src/templates/layout.rs` | Document shell, navbar, footer, error content | Rewrite |
| `website/src/config.rs` | `hex_to_hsl()`, `theme_overrides()`, contrast test | Modify |
| `website/src/main.rs` | Add `CompressionLayer` | Modify |
| `website/Cargo.toml` | Enable tower-http `compression-gzip` | Modify |
| `website/static/app.js` | Add navbar burger toggle | Modify |
| `website/src/routes/auth_pages.rs` | Login/register markup | Modify |
| `website/src/routes/password_reset.rs` | Reset/forgot markup | Modify |
| `website/src/routes/oauth.rs` | OAuth error markup | Modify |
| all `base_layout` call sites | New signature | Modify |

---

### Task 1: Vendor Bulma 1.0.4

**Files:**
- Create: `website/static/vendor/bulma.min.css`
- Create: `website/static/vendor/README.md`

**Interfaces:**
- Consumes: nothing.
- Produces: the stylesheet the layouts link at `/static/vendor/bulma.min.css`.

- [ ] **Step 1: Create the vendor directory and download the pinned build**

```bash
cd /Users/yanis/Programming/mangareader/website/static
mkdir -p vendor
curl -fsSL -o vendor/bulma.min.css \
  https://cdn.jsdelivr.net/npm/bulma@1.0.4/css/bulma.min.css
```

- [ ] **Step 2: Verify the download is the real file, not an error page**

```bash
cd /Users/yanis/Programming/mangareader/website/static
wc -c vendor/bulma.min.css          # expect ~677931 bytes
grep -c '\[data-theme=dark\]' vendor/bulma.min.css   # expect 1
grep -c '\[data-theme=light\]' vendor/bulma.min.css  # expect 1
```

Expected: byte count in the 670–685 KB range, and both `grep -c` return `1`. If either returns `0`, the download is wrong — do not continue.

- [ ] **Step 3: Record provenance**

Create `website/static/vendor/README.md`:

```markdown
# Vendored assets

## bulma.min.css

- Version: **1.0.4** (published 2025-04-19)
- Source: `https://cdn.jsdelivr.net/npm/bulma@1.0.4/css/bulma.min.css`
- License: MIT
- SHA-256: <!-- fill in from the command below -->

Vendored rather than loaded from a CDN so the app renders fully offline on
Raspberry Pi class hardware with no external network dependency.

To upgrade: download the new pinned version, re-run the verification greps in
`docs/superpowers/plans/2026-09-20-bulma-slice-a-foundation.md` (Task 1, Step 2),
and re-run `cargo test` to confirm the Ink & Ember token overrides still land.
```

- [ ] **Step 4: Fill in the checksum**

```bash
cd /Users/yanis/Programming/mangareader/website/static
shasum -a 256 vendor/bulma.min.css
```

Paste the output over the `<!-- fill in ... -->` placeholder.

- [ ] **Step 5: Commit**

```bash
cd /Users/yanis/Programming/mangareader
git add website/static/vendor/
git commit -m "chore(web): vendor Bulma 1.0.4"
```

---

### Task 2: Enable gzip compression for the ~678 KB stylesheet

**Files:**
- Modify: `website/Cargo.toml:12`
- Modify: `website/src/main.rs` (import block, and the router chain ending at `.with_state(state)`)

**Interfaces:**
- Consumes: nothing.
- Produces: every response compressed when the client accepts it. No signature changes.

**Why:** the vendored Bulma is ~678 KB uncompressed. `CompressionLayer` is not currently in the stack and `tower-http` is built with only the `fs` feature.

- [ ] **Step 1: Enable the tower-http feature**

In `website/Cargo.toml`, change:

```toml
tower-http = { version = "0.7", features = ["fs"] }
```

to:

```toml
tower-http = { version = "0.7", features = ["fs", "compression-gzip"] }
```

- [ ] **Step 2: Add the layer to the router**

In `website/src/main.rs`, add to the `tower_http` imports:

```rust
use tower_http::compression::CompressionLayer;
```

Then in the router chain, insert `.layer(CompressionLayer::new())` immediately before `.with_state(state)`:

```rust
        .nest_service(
            "/static",
            ServeDir::new(
                std::env::var("MANGAREAD_STATIC_DIR")
                    .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/static").to_string()),
            ),
        )
        .layer(CompressionLayer::new())
        .with_state(state)
```

- [ ] **Step 3: Build and run the tests**

```bash
cd /Users/yanis/Programming/mangareader
cargo build -p website 2>&1 | tail -20
cargo test -p website 2>&1 | tail -20
```

Expected: clean build, all tests pass. Run the tests before touching any markup — this proves the layer compiles in isolation.

- [ ] **Step 4: Verify compression actually happens**

Start the server, then:

```bash
curl -s -H 'Accept-Encoding: gzip' -o /dev/null -D - http://localhost:3000/static/style.css \
  | grep -iE 'content-encoding|content-length'
```

Expected: `content-encoding: gzip` present, and the reported length is far smaller than 2465 lines of CSS. While `style.css` is still the only stylesheet this is the honest test; re-run it against `bulma.min.css` after Task 3.

- [ ] **Step 5: Commit**

```bash
git add website/Cargo.toml website/Cargo.lock website/src/main.rs
git commit -m "perf(web): gzip responses via tower-http CompressionLayer"
```

---

### Task 3: Ink & Ember token layer + WCAG contrast test

**Files:**
- Create: `website/static/theme.css`
- Modify: `website/src/config.rs` (add a `contrast` test module at the end of the file)

**Interfaces:**
- Consumes: nothing.
- Produces: `static/theme.css` declaring the `--bulma-*` overrides and `--m-*` tokens that every later slice styles against.

**TDD order matters here:** write the contrast test first. It fails because `theme.css` does not exist, and its failure message tells you exactly which token is missing or which pairing is too low.

- [ ] **Step 1: Write the failing contrast test**

Append to `website/src/config.rs`:

```rust
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
        luminance(angle(&token("bulma-scheme-h")), percent(&token("bulma-scheme-s")), lightness_pct)
    }

    fn ratio(a: f64, b: f64) -> f64 {
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    fn scheme_main() -> f64 {
        scheme_luminance(percent(&token("bulma-scheme-main-l")))
    }

    #[test]
    fn body_text_meets_wcag_aa() {
        let r = ratio(scheme_luminance(percent(&token("bulma-text-l"))), scheme_main());
        assert!(r >= 4.5, "--bulma-text-l contrast is {r:.2}:1, need >= 4.5");
    }

    #[test]
    fn muted_text_meets_wcag_aa() {
        let r = ratio(scheme_luminance(percent(&token("bulma-text-weak-l"))), scheme_main());
        assert!(r >= 4.5, "--bulma-text-weak-l contrast is {r:.2}:1, need >= 4.5");
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
        assert!(r > 1.0, "scheme-main-bis is indistinguishable from scheme-main");
    }
}
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cd /Users/yanis/Programming/mangareader
cargo test -p website contrast 2>&1 | tail -20
```

Expected: FAIL. The build error is `couldn't read ../static/theme.css` (file does not exist yet). That is the correct first failure.

- [ ] **Step 3: Write the token layer**

Create `website/static/theme.css`:

```css
/* ==========================================================================
   theme.css — "Ink & Ember" identity layer
   --------------------------------------------------------------------------
   Layer 2 of 3:  vendor/bulma.min.css  ->  theme.css  ->  reader.css

   Every rule that changes Bulma itself targets :root[data-theme="dark"]
   (specificity 0,2,0), which beats Bulma's own :root and [data-theme=*]
   blocks (0,1,0) on specificity rather than on load order.

   Contract: each token below sits on its own line as `--name: value;`.
   The contrast test in src/config.rs reads this file.
   ========================================================================== */

/* --------------------------------------------------------------------------
   Part 1 — Bulma palette and scheme overrides
   -------------------------------------------------------------------------- */
:root[data-theme="dark"] {
  /* Ember accent: #f59e0b, hsl(38 92% 50%).
     Override the hue/saturation/lightness components, never the composed
     color: Bulma generates the whole palette (-00..-100, -invert, -soft,
     -bold, -on-scheme) from these. --bulma-primary-rgb is a literal in
     Bulma and is not recomputed, so it is set by hand. */
  --bulma-primary-h: 38deg;
  --bulma-primary-s: 92%;
  --bulma-primary-l: 50%;
  --bulma-primary-rgb: 245, 158, 11;

  /* Warm neutral scheme. Greys read as ink rather than cool blue-grey, and
     the page surface sits deeper than Bulma's 9% default. */
  --bulma-scheme-h: 30deg;
  --bulma-scheme-s: 8%;
  --bulma-scheme-main-l: 7%;
  --bulma-scheme-main-bis-l: 9%;
  --bulma-scheme-main-ter-l: 11%;
  --bulma-background-l: 13%;
  --bulma-border-l: 22%;
  --bulma-text-l: 72%;
  --bulma-text-strong-l: 94%;

  /* Editorial type: Inter for body, Space Grotesk for display. */
  --bulma-family-primary: "Inter", system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  --bulma-family-secondary: "Space Grotesk", var(--bulma-family-primary);
  --bulma-body-family: var(--bulma-family-primary);

  /* Calmer corners than Bulma's defaults. */
  --bulma-radius: 0.5rem;
  --bulma-radius-medium: 0.625rem;
  --bulma-radius-large: 0.875rem;
}

/* --------------------------------------------------------------------------
   Part 2 — Ink & Ember tokens
   -------------------------------------------------------------------------- */
:root[data-theme="dark"] {
  --m-font-display: var(--bulma-family-secondary);
  --m-tracking-display: -0.02em;
  --m-leading-display: 1.15;
  --m-gutter: 1.5rem;
  --m-cover-radius: var(--bulma-radius);
  --m-ease: cubic-bezier(0.22, 0.61, 0.36, 1);
  --m-dur-fast: 120ms;
  --m-dur: 200ms;
}

/* --------------------------------------------------------------------------
   Part 3 — primitives Bulma does not define

   Everything below exists because Bulma has no equivalent. No class may be
   added here without first being listed in §6 of the design spec.
   -------------------------------------------------------------------------- */

/* Bulma emits no font-family for .title (--bulma-title-family is false), so
   titles inherit body type unless the display face is assigned explicitly. */
.title,
.subtitle,
.m-display {
  font-family: var(--m-font-display);
}

.title {
  letter-spacing: var(--m-tracking-display);
  line-height: var(--m-leading-display);
}

/* Bulma has no navbar current-page state. */
.navbar-item.m-nav-active {
  color: var(--bulma-primary);
}

/* Bulma has no empty-state component. */
.m-empty {
  padding: 3rem 1.5rem;
  text-align: center;
  color: var(--bulma-text-weak);
}

.m-empty-icon {
  margin-bottom: 1rem;
  color: var(--bulma-border);
}

/* Site chrome: the wordmark pairs the logo mark with the display face. */
.m-wordmark {
  gap: 0.625rem;
  font-family: var(--m-font-display);
  font-weight: 600;
  letter-spacing: var(--m-tracking-display);
}

.m-footer {
  padding: 2.5rem 1.5rem 3rem;
}

/* The sync banner is a Bulma notification; this keeps it out of normal flow
   so it never shifts the navbar. */
.sync-banner {
  margin-bottom: 0;
  border-radius: 0;
}

.sync-banner-spinner {
  display: inline-block;
  width: 0.75rem;
  height: 0.75rem;
  margin-right: 0.5rem;
  border: 2px solid currentColor;
  border-top-color: transparent;
  border-radius: 50%;
  vertical-align: -0.1em;
  animation: m-spin 0.7s linear infinite;
}

@keyframes m-spin {
  to {
    transform: rotate(360deg);
  }
}

/* Respect the OS reduced-motion preference for our own animation. */
@media (prefers-reduced-motion: reduce) {
  .sync-banner-spinner {
    animation: none;
  }
}
```

- [ ] **Step 4: Run the contrast test to confirm it passes**

```bash
cd /Users/yanis/Programming/mangareader
cargo test -p website contrast -- --nocapture 2>&1 | tail -25
```

Expected: PASS, 4 tests. The expected ratios are roughly `text-l` 9.5:1, `text-weak-l` 5.5:1, ember accent 8.7:1. All clear WCAG AA for body text.

If the accent test fails, lower `--bulma-primary-l` (for example to `46%`) — never change the hue, because the hue is the identity.

- [ ] **Step 5: Commit**

```bash
git add website/static/theme.css website/src/config.rs
git commit -m "feat(web): Ink and Ember token layer with a WCAG contrast guard"
```

---

### Task 4: Emit admin-derived overrides as Bulma variables

**Files:**
- Modify: `website/src/config.rs` (`hex_to_hsl` free fn, `SiteConfig::css_overrides`, plus tests)

**Interfaces:**
- Consumes: the Bulma variable names from Task 3.
- Produces: `crate::config::hex_to_hsl(hex: &str) -> (f64, f64, f64)` returning `(hue_deg, saturation_pct, lightness_pct)`, and a `css_overrides()` that returns a `:root[data-theme="dark"]` block. The method keeps its current name through Slices A–C; Slice E renames it to `theme_overrides()`.

**Why no rename now:** renaming means sweeping every call site in files that later slices rewrite anyway. The body changes now because admin rebranding must keep working; the name changes in the cleanup slice.

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` in `website/src/config.rs`:

```rust
    #[test]
    fn hex_to_hsl_converts_ember() {
        let (h, s, l) = hex_to_hsl("#f59e0b");
        assert!((h - 38.0).abs() < 0.5, "hue was {h}");
        assert!((s - 92.0).abs() < 0.5, "saturation was {s}");
        assert!((l - 50.0).abs() < 0.5, "lightness was {l}");
    }

    #[test]
    fn hex_to_hsl_handles_greys_and_bad_input() {
        let (h, s, l) = hex_to_hsl("#131210");
        assert!(s < 1.0, "expected a near-neutral saturation, got {s}");
        assert!((l - 7.0).abs() < 0.5, "lightness was {l}");
        assert_eq!(hex_to_hsl("nonsense"), (0.0, 0.0, 0.0));
    }

    #[test]
    fn overrides_target_the_dark_scheme_block() {
        let cfg = SiteConfig {
            site_title: "T".into(),
            accent: "#f59e0b".into(),
            accent_hover: "#fbbf24".into(),
            bg_page: "#131210".into(),
            accent_r: 245,
            accent_g: 158,
            accent_b: 11,
        };
        let css = cfg.css_overrides();
        assert!(css.starts_with(":root[data-theme=\"dark\"]{"), "got {css}");
        assert!(css.contains("--bulma-primary-h:38"), "got {css}");
        assert!(css.contains("--bulma-primary-rgb:245,158,11"), "got {css}");
        assert!(css.contains("--bulma-scheme-main-l:"), "got {css}");
    }
```

Update the existing `settings_overlay_beats_env` test only if it asserted `bg_page`; it currently asserts `site_title` and `accent`, both unchanged.

- [ ] **Step 2: Run the tests to confirm they fail**

```bash
cargo test -p website config:: 2>&1 | tail -25
```

Expected: FAIL — `cannot find function hex_to_hsl`, and the overrides test fails because the block still starts with `:root{`.

- [ ] **Step 3: Implement `hex_to_hsl`**

Add to `website/src/config.rs`, near `parse_hex`:

```rust
/// Convert a `#rrggbb` string to `(hue_degrees, saturation_percent, lightness_percent)`.
///
/// Used to translate the admin accent and background colours into the
/// hue/saturation/lightness components Bulma derives its palettes from.
/// Invalid input returns `(0.0, 0.0, 0.0)`, matching `parse_hex`.
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
```

- [ ] **Step 4: Rewrite `css_overrides`**

Replace the body of `SiteConfig::css_overrides`:

```rust
    /// CSS fragment carrying admin-derived colour overrides.
    ///
    /// Emitted inside `:root[data-theme="dark"]` (specificity 0,2,0) so these
    /// beat the static identity tokens in `theme.css` on specificity rather
    /// than on document order. Only the hue/saturation/lightness components
    /// are written: Bulma derives its palettes from them, so the composed
    /// colours recompute. `--bulma-primary-rgb` is a literal in Bulma and is
    /// therefore written by hand.
    pub fn css_overrides(&self) -> String {
        let (h, s, l) = hex_to_hsl(&self.accent);
        let (_, _, bg_l) = hex_to_hsl(&self.bg_page);
        // Elevation steps stay proportional to the chosen page lightness so
        // the surface hierarchy survives any admin background.
        let bis = (bg_l + 2.0).min(100.0);
        let ter = (bg_l + 4.0).min(100.0);
        let background = (bg_l + 6.0).min(100.0);
        format!(
            ":root[data-theme=\"dark\"]{{--bulma-primary-h:{h:.2}deg;--bulma-primary-s:{s:.2}%;\
--bulma-primary-l:{l:.2}%;--bulma-primary-rgb:{},{},{};--bulma-scheme-main-l:{bg_l:.2}%;\
--bulma-scheme-main-bis-l:{bis:.2}%;--bulma-scheme-main-ter-l:{ter:.2}%;\
--bulma-background-l:{background:.2}%;}}",
            self.accent_r, self.accent_g, self.accent_b
        )
    }
```

- [ ] **Step 5: Change the default background to the identity surface**

In `SiteConfig::from_env`, change the `SITE_BG` default from `#0f0f0f` to `#131210`, the page surface the identity defines (`hsl(30 8% 7%)`). Without this, an unmodified deployment would override `--bulma-scheme-main-l` with a value that does not match the designed identity.

```rust
        let bg_page = std::env::var("SITE_BG").unwrap_or_else(|_| "#131210".into());
```

Also update the `SITE_BG` default quoted in the module doc comment at the top of `config.rs` and in `.env.example`.

- [ ] **Step 6: Run the tests to confirm they pass**

```bash
cargo test -p website config:: 2>&1 | tail -25
```

Expected: PASS, all `config::` tests including the three new ones.

- [ ] **Step 7: Commit**

```bash
git add website/src/config.rs .env.example
git commit -m "feat(web): derive Bulma scheme overrides from admin accent and background"
```

---

### Task 5: Rewrite the site shell on Bulma

**Files:**
- Modify: `website/src/templates/layout.rs` (rewrite `base_layout`, add `Nav`, delete `theme_body_class`)
- Modify: `website/static/app.js` (append the navbar burger toggle)
- Modify: every `base_layout` call site

**Interfaces:**
- Consumes: `static/vendor/bulma.min.css` and `static/theme.css` (Task 1, Task 3).
- Produces:
  - `pub enum Nav { Home, Series, Bookmarks, Admin, None }` (derives `Clone, Copy, PartialEq, Eq`)
  - `pub fn base_layout(title: &str, content: Markup, sync_banner: Option<&str>, overrides: &str, site_title: &str, user: Option<&AuthUser>, nav: Nav) -> Markup`
  - `pub fn reader_layout(title: &str, content: Markup, overrides: &str, site_title: &str, user: Option<&AuthUser>, prefs: &mangaread_core::ReaderPrefs) -> Markup`

The `theme` parameter is **removed** from both layouts and `data-theme="dark"` is emitted on `<html>`. Because the signature changes, this task must land as one atomic commit: the crate does not compile until every call site is updated.

- [ ] **Step 1: Replace the top of `layout.rs`**

Delete `theme_body_class` and the old imports, and add the page-identity enum:

```rust
use crate::auth::AuthUser;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use mangaread_core::domain::Role;

/// Which top-level page is currently displayed, used to mark the active
/// navbar entry.
///
/// This replaces the previous inference from the page title string, which
/// silently broke whenever a page title changed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Home,
    Series,
    Bookmarks,
    Admin,
    None,
}

impl Nav {
    /// `navbar-item` plus the one custom class Bulma does not provide:
    /// Bulma has no navbar current-page state.
    fn class(self, page: Nav) -> &'static str {
        if self == page {
            "navbar-item m-nav-active"
        } else {
            "navbar-item"
        }
    }
}
```

- [ ] **Step 2: Replace `base_layout`**

```rust
/// Render the standard page layout (HTML document shell).
///
/// Emits `<html data-theme="dark">` server-side, so the dark scheme is fixed
/// in the first paint with no flash and no client-side scheme detection.
///
/// The body renders a Bulma `navbar`, the page content inside
/// `main.section > .container`, and a Bulma `footer`. Pages therefore supply
/// only their own content and must not add another `.section > .container`.
///
/// `overrides` is the `:root[data-theme="dark"]` fragment from
/// [`crate::config::SiteConfig::css_overrides`], carrying admin-derived colors.
/// `nav` marks the active navbar entry.
pub fn base_layout(
    title: &str,
    content: Markup,
    sync_banner: Option<&str>,
    overrides: &str,
    site_title: &str,
    user: Option<&AuthUser>,
    nav: Nav,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — " (site_title) }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=Inter:wght@400;500;600&display=swap";
                link rel="stylesheet" href="/static/vendor/bulma.min.css";
                link rel="stylesheet" href="/static/theme.css";
                link rel="icon" type="image/svg+xml" href="/static/favicon.svg";
                style { (PreEscaped(overrides.to_string())) }
            }
            body data-user-id=[user.map(|u| u.id.to_string())] {
                @if let Some(text) = sync_banner {
                    div class="notification is-warning sync-banner" {
                        span class="sync-banner-spinner" {}
                        (text)
                    }
                }
                nav class="navbar" role="navigation" aria-label="main navigation" {
                    div class="navbar-brand" {
                        a class="navbar-item m-wordmark" href="/" {
                            svg width="22" height="22" viewBox="0 0 32 32" fill="none" {
                                rect x="2" y="2" width="28" height="28" rx="6" fill="currentColor" opacity="0.15" {}
                                path d="M8 9h16v2H8zm0 6h16v2H8zm0 6h12v2H8z" fill="currentColor" opacity="0.85" {}
                            }
                            span { (site_title) }
                        }
                        // Bulma ships no JavaScript; app.js drives the burger.
                        a role="button" class="navbar-burger" aria-label="menu" aria-expanded="false" data-target="m-nav-menu" {
                            span aria-hidden="true" {}
                            span aria-hidden="true" {}
                            span aria-hidden="true" {}
                        }
                    }
                    div id="m-nav-menu" class="navbar-menu" {
                        div class="navbar-start" {
                            a class=(nav.class(Nav::Home)) href="/" { "Home" }
                            a class=(nav.class(Nav::Series)) href="/series" { "Series" }
                            a class=(nav.class(Nav::Bookmarks)) href="/bookmarks" { "Bookmarks" }
                            @if let Some(u) = user {
                                @if u.role == Role::Admin {
                                    a class=(nav.class(Nav::Admin)) href="/admin" { "Admin" }
                                }
                            }
                        }
                        div class="navbar-end" {
                            @if let Some(u) = user {
                                a class="navbar-item" href="/settings" { (u.name) }
                                div class="navbar-item" {
                                    form method="post" action="/logout" {
                                        button type="submit" class="button is-small is-ghost" { "Logout" }
                                    }
                                }
                            } @else {
                                a class="navbar-item" href="/login" { "Login" }
                                div class="navbar-item" {
                                    a class="button is-small is-primary" href="/register" { "Sign up" }
                                }
                            }
                        }
                    }
                }
                main class="section" {
                    div class="container" {
                        (content)
                    }
                }
                footer class="footer m-footer" {
                    div class="content has-text-centered" {
                        p { strong { (site_title) } }
                        p class="has-text-grey" { "Your personal manga library." }
                    }
                }
                script src="/static/app.js" {}
                script src="/static/auth.js" {}
            }
        }
    }
}
```

The nav icons from the old markup are dropped: Bulma's navbar is text-first, and the display face carries the identity better than four 15px glyphs.

- [ ] **Step 3: Replace `reader_layout`'s signature and links**

The reader keeps its `PreEscaped` body-tag technique, because Maud 0.27 still cannot splice a bare attribute. Only the stylesheet links and the parameter list change:

```rust
pub fn reader_layout(
    title: &str,
    content: Markup,
    overrides: &str,
    site_title: &str,
    user: Option<&AuthUser>,
    prefs: &mangaread_core::ReaderPrefs,
) -> Markup {
    let user_attr = user
        .map(|u| format!(" data-user-id=\"{}\"", u.id))
        .unwrap_or_default();
    // `body_prefs_attrs` returns raw attribute text. Maud 0.27 has no
    // bare-attribute splice (its attribute parser stops at `(`), so the body
    // opening tag is emitted through `PreEscaped`. The values are escaped by
    // `body_prefs_attrs`/`attr_escape`; only trusted, non-user markup follows.
    let body_open = format!(
        "<body class=\"reader-page\"{}{}>",
        user_attr,
        crate::routes::reader::body_prefs_attrs(prefs),
    );
    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — " (site_title) }
                link rel="stylesheet" href="/static/vendor/bulma.min.css";
                link rel="stylesheet" href="/static/theme.css";
                // reader.css is authored in the next slice; it stays linked so
                // the reader keeps its current layout until then.
                link rel="stylesheet" href="/static/reader.css";
                style { (PreEscaped(overrides.to_string())) }
            }
            (PreEscaped(body_open))
            (content)
            script src="/static/app.js" {}
            script src="/static/auth.js" {}
            script src="/static/reader.js" defer {}
            (PreEscaped("</body>"))
        }
    }
}
```

Note: `style.css` is no longer linked, and `theme_css()` is no longer called. `style.css` stays on disk until Slice E, so the reader's current appearance is carried by `reader.css` plus the new token layer.

- [ ] **Step 4: Add the navbar burger toggle to `app.js`**

Append to `website/static/app.js`:

```javascript
// Bulma ships no JavaScript, so the navbar burger toggle is ours.
document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll('.navbar-burger').forEach((burger) => {
    burger.addEventListener('click', () => {
      const target = document.getElementById(burger.dataset.target);
      if (!target) return;
      burger.classList.toggle('is-active');
      target.classList.toggle('is-active');
      burger.setAttribute(
        'aria-expanded',
        String(burger.classList.contains('is-active')),
      );
    });
  });
});
```

- [ ] **Step 5: Update every call site**

Run the build and let the compiler enumerate the call sites:

```bash
cd /Users/yanis/Programming/mangareader
cargo build -p website 2>&1 | grep -E '^(error|  -->)' | head -60
```

For each call site: **remove** the `theme` argument, **change** the `config_css` argument to whatever expression already produces it, and **add** a `Nav` value as the last argument. Delete any now-unused `resolve_theme` binding and the `theme` element of any `(cfg, theme)` tuple.

Nav mapping:

| Call site | `Nav` |
|---|---|
| `home.rs` | `Nav::Home` |
| `series.rs` (all three) | `Nav::Series` |
| `source.rs` | `Nav::Series` |
| `bookmarks.rs` | `Nav::Bookmarks` |
| `admin.rs` | `Nav::Admin` |
| `settings.rs`, `legal.rs`, `password_reset.rs`, `auth_pages.rs`, `oauth.rs`, `reader.rs` (test) | `Nav::None` |

Worked example — `auth_pages.rs` `page()` becomes:

```rust
fn page(
    cfg: &crate::config::SiteConfig,
    title: &str,
    content: maud::Markup,
    user: Option<&AuthUser>,
) -> Html<String> {
    let css = cfg.css_overrides();
    Html(
        crate::templates::layout::base_layout(
            title,
            content,
            None,
            &css,
            &cfg.site_title,
            user,
            crate::templates::layout::Nav::None,
        )
        .0,
    )
}
```

Its three callers drop the theme argument: `page(&cfg, "Login", ...)`. Also remove the now-dead `resolve_theme(&db, user.as_ref())` from each handler's `(cfg, theme)` tuple in `auth_pages.rs` (there are four: `login_page`, `login_submit`, `register_page`, `register_submit`).

- [ ] **Step 6: Delete `resolve_theme` and its callers**

Once no call site references it, delete `resolve_theme` from `website/src/config.rs`. Then:

```bash
cargo build -p website 2>&1 | grep -E '^(error|warning)' | head -30
```

Expected: no errors and no `unused` warnings.

- [ ] **Step 7: Run the full test suite**

```bash
cargo test -p website 2>&1 | tail -30
```

Expected: PASS. This is the critical checkpoint — `reader_layout_body_carries_prefs_attrs` and the other `data-r-*` tests must still pass, proving the reader body attributes survived the shell rewrite.

- [ ] **Step 8: Commit**

```bash
git add website/src/templates/layout.rs website/static/app.js website/src/ website/static/
git commit -m "feat(web): rebuild the site shell on Bulma navbar and footer"
```

---

### Task 6: Replace `error_layout` with a shared error content helper

**Files:**
- Modify: `website/src/templates/layout.rs` (delete `error_layout`, add `error_content`)
- Modify: `series.rs:36`, `manga.rs:23`, `reader.rs:174`, `reader.rs:184`, `oauth.rs:43`

**Interfaces:**
- Consumes: `base_layout` and `Nav` from Task 5.
- Produces: `pub fn error_content(heading: &str, message: &str) -> Markup`.

**Why:** the current `error_layout` duplicates the entire document shell, calls `SiteConfig::from_env()` instead of the database settings, ignores `config_css`, and its markup has mismatched block nesting. Delegating to `base_layout` removes the duplicate shell entirely and fixes the settings bug for free.

- [ ] **Step 1: Replace `error_layout` with `error_content`**

Delete the whole `error_layout` function and add:

```rust
/// Inner content for an error page.
///
/// Deliberately not a layout: the shell comes from [`base_layout`], so error
/// pages share the navbar, footer, and admin-configured colours with every
/// other page. The previous standalone `error_layout` duplicated the document
/// shell and read colors from the environment instead of the database.
pub fn error_content(heading: &str, message: &str) -> Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-6-desktop is-8-tablet" {
                div class="box has-text-centered" {
                    h1 class="title is-3" { (heading) }
                    p class="subtitle is-6 has-text-grey" { (message) }
                    a class="button is-primary mt-4" href="/" { "Back to Home" }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Update the five call sites**

The pattern for a handler that already holds `cfg` and `user`:

```rust
crate::templates::layout::base_layout(
    heading,
    crate::templates::layout::error_content(heading, message),
    None,
    &cfg.css_overrides(),
    &cfg.site_title,
    user.as_ref(),
    crate::templates::layout::Nav::None,
)
```

In `oauth.rs`, `error_page` becomes:

```rust
fn error_page(cfg: &crate::config::SiteConfig, msg: &str, user: Option<&AuthUser>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Html(
            crate::templates::layout::base_layout(
                "Login",
                crate::templates::layout::error_content("Login failed", msg),
                None,
                &cfg.css_overrides(),
                &cfg.site_title,
                user,
                crate::templates::layout::Nav::None,
            )
            .0,
        ),
    )
        .into_response()
}
```

Its three callers (`fail(...)`) and its `(cfg, theme)` tuple drop the theme.

- [ ] **Step 3: Build and test**

```bash
cargo build -p website 2>&1 | tail -10
cargo test -p website 2>&1 | tail -20
```

Expected: clean build, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add website/src/
git commit -m "refactor(web): share one page shell across error pages"
```

---

### Task 7: Restyle the auth pages on Bulma forms

**Files:**
- Modify: `website/src/routes/auth_pages.rs` (`login_form`, `register_form`, `oauth_buttons`)

**Interfaces:**
- Consumes: the Bulma class vocabulary.
- Produces: no Rust signature changes. The forms keep their exact `method`, `action`, and `name` attributes.

- [ ] **Step 1: Restyle the OAuth buttons**

```rust
/// "Continue with X" links for the configured providers; empty when none.
fn oauth_buttons(providers: &[Provider]) -> maud::Markup {
    html! {
        @if !providers.is_empty() {
            div class="buttons mt-5" {
                @for p in providers {
                    a class="button is-fullwidth" href=(format!("/auth/{}", p.key())) {
                        "Continue with " (provider_label(p))
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Restyle the login form**

`name`, `action`, and `method` are unchanged — only structure and classes move.

```rust
fn login_form(error: Option<&str>, providers: &[Provider]) -> maud::Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-5-desktop is-6-tablet" {
                div class="box" {
                    h1 class="title is-3" { "Login" }
                    @if let Some(msg) = error {
                        div class="notification is-danger is-light" { (msg) }
                    }
                    form method="post" action="/login" {
                        div class="field" {
                            label class="label" { "Email" }
                            div class="control" {
                                input class="input" type="email" name="email" required="";
                            }
                        }
                        div class="field" {
                            label class="label" { "Password" }
                            div class="control" {
                                input class="input" type="password" name="password" required="";
                            }
                        }
                        div class="field" {
                            div class="control" {
                                label class="checkbox" {
                                    input type="checkbox" name="remember" value="on";
                                    " Remember me (30 days)"
                                }
                            }
                        }
                        div class="field" {
                            div class="control" {
                                button type="submit" class="button is-primary is-fullwidth" { "Login" }
                            }
                        }
                    }
                    (oauth_buttons(providers))
                    div class="is-size-7 has-text-grey mt-5" {
                        p { a href="/register" { "Create an account" } }
                        p { a href="/forgot" { "Forgot password?" } }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 3: Restyle the register form the same way**

```rust
fn register_form(error: Option<&str>, providers: &[Provider]) -> maud::Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-5-desktop is-6-tablet" {
                div class="box" {
                    h1 class="title is-3" { "Register" }
                    @if let Some(msg) = error {
                        div class="notification is-danger is-light" { (msg) }
                    }
                    form method="post" action="/register" {
                        div class="field" {
                            label class="label" { "Name" }
                            div class="control" {
                                input class="input" type="text" name="name" maxlength="32" required="";
                            }
                        }
                        div class="field" {
                            label class="label" { "Email" }
                            div class="control" {
                                input class="input" type="email" name="email" required="";
                            }
                        }
                        div class="field" {
                            label class="label" { "Password (min 8 chars)" }
                            div class="control" {
                                input class="input" type="password" name="password" required="";
                            }
                        }
                        div class="field" {
                            div class="control" {
                                button type="submit" class="button is-primary is-fullwidth" { "Register" }
                            }
                        }
                    }
                    (oauth_buttons(providers))
                    div class="is-size-7 has-text-grey mt-5" {
                        p { a href="/login" { "Already have an account? Login" } }
                    }
                }
            }
        }
    }
}
```

The surrounding `section.auth-section` / `h1.page-title` wrappers are gone because `base_layout` now supplies `main.section > .container`.

- [ ] **Step 4: Build and test**

```bash
cargo build -p website 2>&1 | tail -10
cargo test -p website 2>&1 | tail -20
```

Expected: clean build, all tests pass.

- [ ] **Step 5: Verify the forms by hand**

Start the server and confirm at `/login` and `/register`:
- Each form posts to the same URL it did before.
- Submitting an invalid login still renders the red notification and a 401.
- The `remember` checkbox still produces the 30-day session.

```bash
curl -s http://localhost:3000/login | grep -oE '<form[^>]*>|name="(email|password|remember|name)"'
```

Expected: `<form method="post" action="/login">` and all four `name=` attributes present.

- [ ] **Step 6: Commit**

```bash
git add website/src/routes/auth_pages.rs
git commit -m "feat(web): restyle auth pages on Bulma forms"
```

---

### Task 8: Restyle the password reset pages

**Files:**
- Modify: `website/src/routes/password_reset.rs` (`forgot_page`, `reset_page`, and the error re-renders in `reset_submit`)

**Interfaces:**
- Consumes: the Bulma class vocabulary and the `base_layout` signature.
- Produces: no signature changes. `name`, `action`, and `method` unchanged.

- [ ] **Step 1: Read the current file and locate the markup**

```bash
cd /Users/yanis/Programming/mangareader
grep -n 'auth-section\|auth-error\|auth-form\|page-title\|base_layout\|resolve_theme' website/src/routes/password_reset.rs
```

- [ ] **Step 2: Restyle the forgot page**

Replace the `section.auth-section` block with a Bulma box:

```rust
div class="columns is-centered" {
    div class="column is-5-desktop is-6-tablet" {
        div class="box" {
            h1 class="title is-3" { "Forgot password" }
            p class="content" {
                "Password resets are issued by an administrator for this instance. "
                "Ask an admin to generate a reset link for your account."
            }
            a class="button is-primary is-fullwidth" href="/login" { "Back to login" }
        }
    }
}
```

- [ ] **Step 3: Restyle the set-password form**

Keep `method`, `action`, and every `name=` exactly as they are; move only the classes:

```rust
div class="columns is-centered" {
    div class="column is-5-desktop is-6-tablet" {
        div class="box" {
            h1 class="title is-3" { "Set a new password" }
            @if let Some(msg) = error {
                div class="notification is-danger is-light" { (msg) }
            }
            form method="post" action=(format!("/reset/{token}")) {
                div class="field" {
                    label class="label" { "New password (min 8 chars)" }
                    div class="control" {
                        input class="input" type="password" name="password" required="";
                    }
                }
                div class="field" {
                    div class="control" {
                        button type="submit" class="button is-primary is-fullwidth" { "Set password" }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: Update the error re-renders**

`reset_submit` re-renders the same markup with an error. Route it through the same helper so the two paths cannot drift. If the handler currently inlines a second copy, extract a `reset_form(action: &str, error: Option<&str>) -> Markup` and call it from both.

- [ ] **Step 5: Build, test, and verify by hand**

```bash
cargo build -p website 2>&1 | tail -10
cargo test -p website 2>&1 | tail -20
curl -s http://localhost:3000/forgot | grep -oE '<form[^>]*>|class="box"'
```

Expected: clean build, tests pass, and the reset form still posts to its original action.

- [ ] **Step 6: Commit**

```bash
git add website/src/routes/password_reset.rs
git commit -m "feat(web): restyle password reset pages on Bulma"
```

---

### Task 9: Slice A verification and page-weight measurement

**Files:**
- No source changes unless a check fails.

**Interfaces:**
- Consumes: everything above.
- Produces: the measured evidence the spec's Slice A exit criteria require.

- [ ] **Step 1: Full test suite**

```bash
cd /Users/yanis/Programming/mangareader
cargo test 2>&1 | tail -30
```

Expected: all workspace tests pass.

- [ ] **Step 2: Manual route sweep**

Start the server and open each route. Confirm it renders with the new navbar and footer, that the ember accent appears on the active nav entry, and that the browser console has no 404s for `/static/style.css`, `/static/theme.css`, or `/static/vendor/bulma.min.css`.

```bash
for p in / /series /bookmarks /login /register /forgot /settings /admin /terms /privacy /nope; do
  printf '%-14s %s\n' "$p" "$(curl -s -o /dev/null -w '%{http_code}' http://localhost:3000$p)"
done
```

Expected: `200` for every real route and `404` for `/nope`. `/settings` and `/admin` may redirect to `/login` when not authenticated — that is correct behavior.

- [ ] **Step 3: Verify transfer size for the stylesheets**

```bash
for f in /static/vendor/bulma.min.css /static/theme.css; do
  raw=$(curl -s -o /dev/null -w '%{size_download}' "http://localhost:3000$f")
  gz=$(curl -s -H 'Accept-Encoding: gzip' -o /dev/null -w '%{size_download}' "http://localhost:3000$f")
  printf '%-34s raw=%8d  gzip=%8d\n' "$f" "$raw" "$gz"
done
```

Expected: `bulma.min.css` is roughly 678 KB raw and well under 100 KB gzipped. Record both numbers in the commit message. If `raw` is near zero, the vendor path is wrong.

- [ ] **Step 4: Confirm the cascade actually wins**

The whole design rests on `:root[data-theme="dark"]` beating Bulma's own blocks. Verify it in the browser, not in the abstract:

```bash
curl -s http://localhost:3000/ | grep -oE '<html[^>]*>|<style>[^<]*</style>'
```

Then, in the browser devtools console on any page:

```javascript
getComputedStyle(document.documentElement).getPropertyValue('--bulma-primary-h');
getComputedStyle(document.documentElement).getPropertyValue('--bulma-scheme-main-l');
```

Expected: `38deg` and something near `7%` (the admin override, which is `5.88%` until `SITE_BG` is set to the identity surface). If these report Bulma's factory `171deg` and `9%`, the override block is not landing and everything downstream is wrong — fix the selector before proceeding.

- [ ] **Step 5: Check the ember accent in the browser**

Open `/login` and confirm the primary button and focused input borders are ember, not Bulma's default teal. This is the fastest visual signal that the palette override propagated through Bulma's derived variables.

- [ ] **Step 6: Commit any fixes, then close the slice**

```bash
git add -A
git commit -m "chore(web): slice A verification fixes" --allow-empty
```

Expected state at the end of Slice A: the shell, error pages, and auth pages are on Bulma and carry the Ink & Ember identity. Content pages still use their legacy markup, rendered inside the new shell, and Slice B converts them.
