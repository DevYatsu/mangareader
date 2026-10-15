// Live preview for /admin/branding. Pure preview: no fetch calls —
// saving still goes through the form submit button.
//
// The Bulma shell derives its palette from the `--bulma-*-h/s/l`
// components in theme.css, so this preview writes those components.
(function () {
  "use strict";

  // Mirrors `hex_to_hsl` in website/src/config.rs: same sRGB→HSL conversion,
  // same achromatic early-return. Returns [hue_degrees, saturation_pct,
  // lightness_pct]; a wrong-length string returns [0, 0, 0].
  function hexToHsl(hex) {
    var h = String(hex).trim().replace(/^#/, "");
    if (h.length !== 6) return [0, 0, 0];
    var r = parseInt(h.slice(0, 2), 16) / 255;
    var g = parseInt(h.slice(2, 4), 16) / 255;
    var b = parseInt(h.slice(4, 6), 16) / 255;
    var max = Math.max(r, g, b);
    var min = Math.min(r, g, b);
    var l = (max + min) / 2;
    if (max - min === 0) return [0, 0, l * 100];
    var d = max - min;
    var s = d / (1 - Math.abs(2 * l - 1));
    var hue;
    if (max === r) {
      hue = 60 * (((g - b) / d) % 6);
    } else if (max === g) {
      hue = 60 * ((b - r) / d + 2);
    } else {
      hue = 60 * ((r - g) / d + 4);
    }
    if (hue < 0) hue += 360;
    return [hue, s * 100, l * 100];
  }

  function setVar(name, value) {
    document.documentElement.style.setProperty(name, value);
  }

  // Mirrors `SiteConfig::theme_overrides` in config.rs; only the two live color
  // controls are previewed.
  function previewAccent(value) {
    var hsl = hexToHsl(value);
    setVar("--bulma-primary-h", hsl[0].toFixed(0) + "deg");
    setVar("--bulma-primary-s", hsl[1].toFixed(0) + "%");
    setVar("--bulma-primary-l", hsl[2].toFixed(0) + "%");
    // Bulma stores this one as a literal and never recomputes it.
    var h = value.trim().replace(/^#/, "");
    setVar(
      "--bulma-primary-rgb",
      [
        parseInt(h.slice(0, 2), 16),
        parseInt(h.slice(2, 4), 16),
        parseInt(h.slice(4, 6), 16),
      ].join(","),
    );
  }

  // Elevation steps stay proportional to the chosen page lightness, exactly
  // as the server derives them (+2 / +4 / +6), so the surface hierarchy
  // survives any background the admin picks.
  function previewBg(value) {
    var bgL = hexToHsl(value)[2];
    setVar("--bulma-scheme-main-l", bgL.toFixed(0) + "%");
    setVar("--bulma-scheme-main-bis-l", Math.min(bgL + 2, 100).toFixed(0) + "%");
    setVar("--bulma-scheme-main-ter-l", Math.min(bgL + 4, 100).toFixed(0) + "%");
    setVar("--bulma-background-l", Math.min(bgL + 6, 100).toFixed(0) + "%");
  }

  document.addEventListener("DOMContentLoaded", function () {
    var form = document.querySelector("#admin-branding-form");
    if (!form) return;
    var accent = form.querySelector('input[name="site_accent"]');
    if (accent) {
      accent.addEventListener("input", function () {
        previewAccent(accent.value);
      });
    }
    var bg = form.querySelector('input[name="site_bg"]');
    if (bg) {
      bg.addEventListener("input", function () {
        previewBg(bg.value);
      });
    }
  });
})();
