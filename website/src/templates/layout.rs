use crate::auth::AuthUser;
use mangaread_core::domain::Role;
use maud::{DOCTYPE, Markup, PreEscaped, html};

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
/// [`crate::config::SiteConfig::theme_overrides`], carrying admin-derived colors.
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
                        // A real button, not the `<a role="button">` Bulma's own
                        // snippet uses: below Bulma's 1024px breakpoint the menu
                        // is hidden, so the primary navigation must be reachable
                        // and activatable by keyboard. A button is natively
                        // focusable and fires `click` on Enter/Space.
                        button type="button" class="navbar-burger" aria-label="menu" aria-expanded="false" data-target="m-nav-menu" {
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
                        p class="m-text-muted" { "Your personal manga library." }
                    }
                }
                script src="/static/app.js" {}
                script src="/static/auth.js" {}
            }
        }
    }
}

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
                    p class="subtitle is-6 m-text-muted" { (message) }
                    a class="button is-primary mt-4" href="/" { "Back to Home" }
                }
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
