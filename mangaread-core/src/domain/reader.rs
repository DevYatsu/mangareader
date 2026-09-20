//! Reader 2.0 preference types.
//!
//! Pure data + validation. No SQL, no HTTP. Persisted as one JSON blob in
//! `users.reader_prefs`; `normalized()` is the single source of truth for
//! which values are legal, so a hand-crafted API body can never poison the
//! stored blob.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const LAYOUTS: &[&str] = &["single", "double", "strip", "vertical"];
pub const DIRECTIONS: &[&str] = &["ltr", "rtl"];
pub const FITS: &[&str] = &["", "height", "screen"];
pub const BGS: &[&str] = &["black", "white", "sepia", "custom"];

/// Stable action ids for the remap UI (design contract §4.3).
pub const ACTION_IDS: &[&str] = &[
    "page.next",
    "page.prev",
    "chapter.next",
    "chapter.prev",
    "ui.toggle",
    "ui.focus",
    "ui.fitCycle",
    "layout.cycle",
    "note.add",
];

/// Default bindings. Keys are `KeyboardEvent.code` values; modifiers are a
/// `+`-joined prefix in a stable order.
///
/// `"?"` is the one entry that is a `KeyboardEvent.key` value rather than a
/// code: the question-mark character has no dedicated code, so the client
/// matches it on `key` (contract §3.1 lists `?` alongside `KeyS`).
pub const DEFAULT_KEYBINDS: &[(&str, &[&str])] = &[
    ("page.next", &["ArrowRight", "ArrowDown", "Space", "PageDown"]),
    ("page.prev", &["ArrowLeft", "ArrowUp", "PageUp"]),
    ("chapter.next", &["Shift+ArrowRight", "KeyN"]),
    ("chapter.prev", &["Shift+ArrowLeft", "KeyP"]),
    ("ui.toggle", &["KeyS", "Escape", "?"]),
    ("ui.focus", &["KeyF"]),
    ("ui.fitCycle", &["KeyW"]),
    ("layout.cycle", &["KeyL"]),
    ("note.add", &["KeyM"]),
];

/// Reader preferences. Every field carries a serde default so an older or
/// partial blob always deserializes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReaderPrefs {
    pub layout: String,
    pub direction: String,
    /// `""` = width mode, `height`, or `screen`.
    pub fit: String,
    pub max_w: i64,
    pub gap: i64,
    pub bg: String,
    pub bg_custom: String,
    pub frame: bool,
    pub double_cover: bool,
    pub auto_next: bool,
    pub auto_prev: bool,
    pub hide_delay: i64,
    pub dim_delay: i64,
    pub dim_opacity: f64,
    pub swipe_threshold: i64,
    /// Two boundary percentages from the left, e.g. `[30, 70]`.
    pub click_zones: Vec<i64>,
    pub keybinds: HashMap<String, Vec<String>>,
    pub show_notes: bool,
}

impl Default for ReaderPrefs {
    fn default() -> Self {
        Self {
            layout: "strip".into(),
            direction: "ltr".into(),
            fit: String::new(),
            max_w: 700,
            gap: 0,
            bg: "black".into(),
            bg_custom: "#0a0a0a".into(),
            frame: false,
            double_cover: false,
            auto_next: false,
            auto_prev: false,
            hide_delay: 3,
            dim_delay: 0,
            dim_opacity: 0.5,
            swipe_threshold: 70,
            click_zones: vec![30, 70],
            keybinds: default_keybinds(),
            show_notes: true,
        }
    }
}

fn default_keybinds() -> HashMap<String, Vec<String>> {
    DEFAULT_KEYBINDS
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.iter().map(|s| (*s).to_string()).collect()))
        .collect()
}

fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    v.max(lo).min(hi)
}

fn allow(value: &str, allowed: &[&str], fallback: &str) -> String {
    if allowed.contains(&value) {
        value.to_string()
    } else {
        fallback.to_string()
    }
}

impl ReaderPrefs {
    /// Clamp and allowlist every field. Idempotent. This is the only place
    /// legal values are defined, so it is also the API's input validator.
    pub fn normalized(mut self) -> Self {
        self.layout = allow(&self.layout, LAYOUTS, "strip");
        self.direction = allow(&self.direction, DIRECTIONS, "ltr");
        if !FITS.contains(&self.fit.as_str()) {
            self.fit = String::new();
        }
        self.max_w = clamp(self.max_w, 400, 1400);
        self.gap = clamp(self.gap, 0, 64);
        self.bg = allow(&self.bg, BGS, "black");
        if self.bg_custom.len() != 7
            || !self.bg_custom.starts_with('#')
            || !self.bg_custom[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            self.bg_custom = "#0a0a0a".into();
        } else {
            self.bg_custom = self.bg_custom.to_lowercase();
        }
        self.hide_delay = clamp(self.hide_delay, 0, 10);
        // dimDelay is a small enumerated choice, not a continuum.
        if ![0, 15, 30, 60].contains(&self.dim_delay) {
            self.dim_delay = 0;
        }
        self.dim_opacity = if self.dim_opacity.is_finite() {
            self.dim_opacity.max(0.2).min(0.8)
        } else {
            0.5
        };
        self.swipe_threshold = clamp(self.swipe_threshold, 40, 160);

        // Zones: always exactly two boundaries, 10..90, at least 10 apart.
        let a = clamp(*self.click_zones.first().unwrap_or(&30), 10, 80);
        let b = clamp(*self.click_zones.get(1).unwrap_or(&70), a + 10, 90);
        self.click_zones = vec![a, b];

        // Keybinds: every known action present, unknown actions dropped,
        // empty bindings dropped, duplicates removed, values length-capped.
        let mut kb: HashMap<String, Vec<String>> = HashMap::new();
        for action in ACTION_IDS {
            let mut keys: Vec<String> = self
                .keybinds
                .get(*action)
                .map(|v| {
                    let mut seen = std::collections::HashSet::new();
                    v.iter()
                        .filter(|k| !k.trim().is_empty() && k.len() <= 32)
                        .filter(|k| seen.insert((*k).clone()))
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if keys.is_empty() {
                keys = DEFAULT_KEYBINDS
                    .iter()
                    .find(|(k, _)| k == action)
                    .map(|(_, v)| v.iter().map(|s| (*s).to_string()).collect())
                    .unwrap_or_default();
            }
            kb.insert((*action).to_string(), keys);
        }
        self.keybinds = kb;
        self
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    /// Never fails: an unreadable blob degrades to defaults rather than
    /// breaking the reader page.
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str::<ReaderPrefs>(s)
            .map(|p| p.normalized())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_current_behavior() {
        let p = ReaderPrefs::default();
        assert_eq!(p.layout, "strip");      // preserve today's long-strip default
        assert_eq!(p.direction, "ltr");
        assert_eq!(p.fit, "");              // "" = width mode
        assert_eq!(p.max_w, 700);           // matches .reader-images max-width:700px
        assert_eq!(p.gap, 0);
        assert_eq!(p.bg, "black");
        assert_eq!(p.hide_delay, 3);
        assert_eq!(p.dim_delay, 0);
        assert_eq!(p.dim_opacity, 0.5);
        assert_eq!(p.swipe_threshold, 70);
        assert_eq!(p.click_zones, vec![30, 70]);
        assert!(!p.auto_next);
        assert!(p.show_notes);
    }

    #[test]
    fn normalization_clamps_and_rejects_unknown() {
        let mut p = ReaderPrefs::default();
        p.layout = "hologram".into();       // unknown -> default
        p.direction = "diagonal".into();    // unknown -> default
        p.max_w = 99_999;                   // clamp to 1400
        p.gap = 9_999;                      // clamp to 64
        p.hide_delay = 99;                  // clamp to 10
        p.dim_opacity = 5.0;                // clamp to 0.8
        p.swipe_threshold = 1;              // clamp to 40
        p.click_zones = vec![5, 99];        // clamp to 10..90, >=10 apart
        let n = p.normalized();
        assert_eq!(n.layout, "strip");
        assert_eq!(n.direction, "ltr");
        assert_eq!(n.max_w, 1400);
        assert_eq!(n.gap, 64);
        assert_eq!(n.hide_delay, 10);
        assert_eq!(n.dim_opacity, 0.8);
        assert_eq!(n.swipe_threshold, 40);
        assert_eq!(n.click_zones, vec![10, 90]); // each boundary clamped independently
    }

    #[test]
    fn keybinds_are_sanitized() {
        use std::collections::HashMap;
        let mut p = ReaderPrefs::default();
        let mut kb = HashMap::new();
        kb.insert("page.next".to_string(), vec!["KeyX".to_string()]);
        kb.insert("bogus.action".to_string(), vec!["KeyY".to_string()]);
        kb.insert("page.prev".to_string(), vec![]); // empty -> defaults restored
        p.keybinds = kb;
        let n = p.normalized();
        assert!(n.keybinds.contains_key("page.next"));
        assert_eq!(n.keybinds["page.next"], vec!["KeyX".to_string()]);
        assert!(!n.keybinds.contains_key("bogus.action"));
        assert_eq!(
            n.keybinds["page.prev"],
            vec!["ArrowLeft".to_string(), "ArrowUp".to_string(), "PageUp".to_string()]
        );
        // every action present, defaults restored for the missing ones
        assert_eq!(n.keybinds.len(), ACTION_IDS.len());
    }

    #[test]
    fn json_roundtrip_preserves_values() {
        let mut p = ReaderPrefs::default();
        p.layout = "double".into();
        p.max_w = 1100;
        let s = p.to_json();
        assert_eq!(ReaderPrefs::from_json(&s), p);
    }

    #[test]
    fn from_json_garbage_yields_defaults() {
        assert_eq!(ReaderPrefs::from_json("not json"), ReaderPrefs::default());
        assert_eq!(ReaderPrefs::from_json("{}"), ReaderPrefs::default());
    }
}
