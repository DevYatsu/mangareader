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

/// Render the standard page layout (HTML document shell).
///
/// Includes the `<title>`, Google Fonts preconnects, stylesheet, site header,
/// main content area, and footer. The caller supplies the page title and
/// inner body content as Maud `Markup`.
///
/// If `sync_banner` is `Some(...)`, a banner is shown at the top of the
/// page indicating the background sync loop is running.
///
/// `config_css` is a `<style>` fragment overriding CSS custom properties
/// (accent color, background, etc.) — generated from [`SiteConfig::css_overrides`].
/// `user` drives the nav session state (`None` → Login link, `Some` → name
/// + logout form) and sets `data-user-id` on `<body>` when logged in.
/// `theme` is the resolved theme key: allowlisted names add a
/// `theme-{theme}` body class plus the [`crate::config::theme_css`] link;
/// unknown/empty names render with no body class and no extra link.
pub fn base_layout(title: &str, content: Markup, sync_banner: Option<&str>, config_css: &str, site_title: &str, user: Option<&AuthUser>, theme: &str) -> Markup {
    let home_class = if title == "Home" { "active" } else { "" };
    let series_class = if title == "Series" { "active" } else { "" };
    let bookmarks_class = if title == "Bookmarks" { "active" } else { "" };
    let theme_class = theme_body_class(theme);
    let theme_link = crate::config::theme_css(theme);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — " (site_title) }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=Inter:wght@400;500;600&display=swap";
                link rel="stylesheet" href="/static/style.css";
                @if let Some(href) = &theme_link {
                    link id="theme-css" rel="stylesheet" href=(href);
                }
                link rel="icon" type="image/svg+xml" href="/static/favicon.svg";
                style { (PreEscaped(config_css.to_string())) }
            }
            body class=[theme_class] data-user-id=[user.map(|u| u.id.to_string())] {
                @if let Some(text) = sync_banner {
                    div class="sync-banner" {
                        span class="sync-banner-spinner" {}
                        (text)
                    }
                }
                div.site-wrap {
                    div.container {
                    header class="site-header" {
                        a class="site-logo" href="/" {
                            svg class="logo-mark" width="22" height="22" viewBox="0 0 32 32" fill="none" {
                                rect x="2" y="2" width="28" height="28" rx="6" fill="currentColor" opacity="0.15" {}
                                path d="M8 9h16v2H8zm0 6h16v2H8zm0 6h12v2H8z" fill="currentColor" opacity="0.85" {}
                            }
                            span class="logo-text" { (site_title) }
                        }
                        nav class="site-nav" {
                            a href="/" class=(home_class) {
                                svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" {
                                    path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" {}
                                    polyline points="9 22 9 12 15 12 15 22" {}
                                }
                                "Home"
                            }
                            a href="/series" class=(series_class) {
                                svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" {
                                    rect x="3" y="3" width="7" height="7" rx="1" {}
                                    rect x="14" y="3" width="7" height="7" rx="1" {}
                                    rect x="14" y="14" width="7" height="7" rx="1" {}
                                    rect x="3" y="14" width="7" height="7" rx="1" {}
                                }
                                "Series"
                            }
                            a href="/bookmarks" class=(bookmarks_class) {
                                svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" {
                                    path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" {}
                                }
                                "Bookmarks"
                            }
                            @if let Some(u) = user {
                                @if u.role == Role::Admin {
                                    a href="/admin" { "Admin" }
                                }
                                a href="/settings" class="user-name" { (u.name) }
                                form method="post" action="/logout" class="logout-form" {
                                    button type="submit" class="logout-btn" { "Logout" }
                                }
                            } @else {
                                a href="/login" { "Login" }
                            }
                        }
                    }
                    main { (content) }
                    footer {
                        p { (site_title) }
                        p class="footer-tagline" { "Your personal manga library." }
                    }
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
/// `theme` behaves as in [`base_layout`], except the body class is appended
/// to the fixed `reader-page` class instead of replacing it.
pub fn reader_layout(title: &str, content: Markup, config_css: &str, site_title: &str, user: Option<&AuthUser>, theme: &str) -> Markup {
    let theme_class = theme_body_class(theme).map(|c| format!("reader-page {c}")).unwrap_or_else(|| "reader-page".to_string());
    let theme_link = crate::config::theme_css(theme);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — " (site_title) }
                link rel="stylesheet" href="/static/style.css";
                @if let Some(href) = &theme_link {
                    link rel="stylesheet" href=(href);
                }
                style { (PreEscaped(config_css.to_string())) }
            }
            body class=(theme_class) data-user-id=[user.map(|u| u.id.to_string())] {
                (content)
                script src="/static/app.js" {}
                script src="/static/auth.js" {}
            }
        }
    }
}
