use crate::auth::AuthUser;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use mangaread_core::domain::Role;

/// Body class for a theme key, or `None` when no class applies.
///
/// Every allowlisted [`crate::config::THEMES`] key (including `default`)
/// yields `theme-{key}`; unknown/empty names yield no class so the base
/// stylesheet look is preserved.
fn theme_body_class(theme: &str) -> Option<String> {
    if crate::config::is_theme(theme) {
        Some(format!("theme-{theme}"))
    } else {
        None
    }
}

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

/// Render an error page with a heading, detail message, and a link back home.
///
/// The header and footer match the standard `base_layout`, so the error page
/// does not feel disconnected from the rest of the site.
///
/// `theme` behaves as in [`base_layout`]: allowlisted names add a
/// `theme-{theme}` body class plus the theme stylesheet link.
pub fn error_layout(heading: &str, message: &str, theme: &str) -> Markup {
    let site_title = std::env::var("SITE_TITLE").unwrap_or_else(|_| "MangaRead".into());
    let config = crate::config::SiteConfig::from_env();
    let theme_class = theme_body_class(theme);
    let theme_link = crate::config::theme_css(theme);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (heading) " — " (site_title) }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=Inter:wght@400;500;600&display=swap";
                link rel="stylesheet" href="/static/style.css";
                @if let Some(href) = &theme_link {
                    link rel="stylesheet" href=(href);
                }
                style { (PreEscaped(config.css_overrides())) }
            }
            body class=[theme_class] {
                div.container {
                    header class="site-header" {
                        a class="site-logo" href="/" { (site_title) }
                        nav class="site-nav" {
                            a href="/" { "Home" }
                            a href="/series" { "Series" }
                        }
                    }
                    main {
                        section class="error-section" {
                            h1 class="error-heading" { (heading) }
                            p class="error-message" { (message) }
                            a href="/" class="error-home-btn" { "Back to Home" }
                        }
            }
            footer {
                p { (site_title) }
            }
        }
        script src="/static/app.js" {}
    }
}
    }
}

/// Render the reader-specific page layout.
///
/// A minimal layout without the standard site header or footer, intended for
/// the full-screen chapter reader view. Loads the reader stylesheet and the
/// `app.js` script for page navigation and progress tracking.
///
/// `user` sets `data-user-id` on `<body>` when logged in so the sync adapter
/// (`auth.js`) can write reading progress through to the server.
///
/// `prefs` supplies the `data-r-*` body attributes via
/// [`crate::routes::reader::body_prefs_attrs`], so the first paint already
/// reflects the stored layout. The reader stylesheet and the deferred
/// `reader.js` are loaded last.
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
