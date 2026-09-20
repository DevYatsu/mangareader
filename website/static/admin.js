// Live preview for /admin/branding. Pure preview: no fetch calls —
// saving still goes through the form submit button.
(function () {
  "use strict";

  // Find the theme stylesheet link, creating it when the site runs on
  // the default theme (which ships no extra stylesheet).
  function themeLink() {
    var link = document.getElementById("theme-css");
    if (!link) {
      link = document.createElement("link");
      link.id = "theme-css";
      link.rel = "stylesheet";
      document.head.appendChild(link);
    }
    return link;
  }

  function previewTheme(key) {
    var link = themeLink();
    if (!key || key === "default") {
      link.removeAttribute("href");
    } else {
      link.setAttribute("href", "/static/themes/" + key + ".css");
    }
  }

  function previewAccent(value) {
    document.documentElement.style.setProperty("--accent", value);
  }

  function previewBg(value) {
    document.documentElement.style.setProperty("--bg-page", value);
  }

  document.addEventListener("DOMContentLoaded", function () {
    var form = document.querySelector(".admin-form");
    if (!form) return;
    form.querySelectorAll('input[name="site_theme"]').forEach(function (radio) {
      radio.addEventListener("change", function () {
        if (radio.checked) previewTheme(radio.value);
      });
    });
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
