// Immersive manga reader — tap to toggle controls
(function () {
  var nav = document.getElementById("reader-nav");
  var hint = document.getElementById("reader-hint");
  var images = document.getElementById("reader-images");
  var hideTimer = null;

  function showNav() {
    nav.classList.add("visible");
    clearTimeout(hideTimer);
    hideTimer = setTimeout(hideNav, 3500);
  }

  function hideNav() {
    nav.classList.remove("visible");
  }

  // Tap on images area toggles nav
  images.addEventListener("click", function (e) {
    // Don't toggle when clicking a link
    if (e.target.closest("a")) return;
    if (nav.classList.contains("visible")) {
      hideNav();
    } else {
      showNav();
    }
  });

  // Keep nav visible while hovering over it with mouse
  nav.addEventListener("mouseenter", function () {
    clearTimeout(hideTimer);
  });

  nav.addEventListener("mouseleave", function () {
    if (nav.classList.contains("visible")) {
      hideTimer = setTimeout(hideNav, 3500);
    }
  });

  // Auto-show nav on scroll up, hide on scroll down
  var lastScrollY = 0;
  window.addEventListener("scroll", function () {
    var sy = window.scrollY;
    if (sy < lastScrollY && sy > 100) {
      // Scrolled up — show nav briefly
      showNav();
    } else if (sy > lastScrollY && sy > 200) {
      // Scrolled down — hide nav
      hideNav();
    }
    lastScrollY = sy;
  });

  // Fade out hint after first interaction
  function dismissHint() {
    if (hint) {
      hint.classList.add("hidden");
    }
    document.removeEventListener("click", dismissHint);
    document.removeEventListener("scroll", dismissHint);
  }
  document.addEventListener("click", dismissHint);
  document.addEventListener("scroll", dismissHint);

  // Show nav briefly on first load for discoverability
  setTimeout(function () {
    showNav();
    // Auto-hide after initial display
    setTimeout(hideNav, 2500);
  }, 400);

  // Keyboard navigation
  document.addEventListener("keydown", function (e) {
    if (e.key === "ArrowLeft" || e.key === "a") {
      let prev = document.querySelector('.reader-nav a[href*="/read/"]');
      if (prev && !prev.classList.contains("disabled")) {
        window.location.href = prev.getAttribute("href");
      }
    }
    if (e.key === "ArrowRight" || e.key === "d") {
      let links = document.querySelectorAll('.reader-nav a[href*="/read/"]');
      let next = links[links.length - 1];
      if (next && !next.classList.contains("disabled")) {
        window.location.href = next.getAttribute("href");
      }
    }
    if (e.key === "Escape" || e.key === "t") {
      nav.classList.toggle("visible");
      if (nav.classList.contains("visible")) {
        clearTimeout(hideTimer);
      }
    }
  });
})();
