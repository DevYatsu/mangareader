use maud::{html, Markup, PreEscaped, DOCTYPE};

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
pub fn base_layout(title: &str, content: Markup, sync_banner: Option<&str>, config_css: &str, site_title: &str) -> Markup {
    let home_class = if title == "Home" { "active" } else { "" };
    let series_class = if title == "Series" { "active" } else { "" };
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
                link rel="icon" type="image/svg+xml" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='6' fill='%23f59e0b'/%3E%3Cpath d='M8 8h16v2H8zm0 5h16v2H8zm0 5h12v2H8z' fill='%23000' opacity='0.9'/%3E%3C/svg%3E";
                style { (PreEscaped(config_css.to_string())) }
            }
            body {
                @if let Some(text) = sync_banner {
                    div class="sync-banner" {
                        span class="sync-banner-spinner" {}
                        (text)
                    }
                }
                div.site-wrap {
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
                        }
                    }
                    main { (content) }
                    footer {
                        p { (site_title) }
                        p class="footer-tagline" { "Your personal manga library." }
                    }
                }
            }
        }
    }
}

/// Render an error page with a heading, detail message, and a link back home.
///
/// The header and footer match the standard `base_layout`, so the error page
/// does not feel disconnected from the rest of the site.
pub fn error_layout(heading: &str, message: &str) -> Markup {
    let site_title = std::env::var("SITE_TITLE").unwrap_or_else(|_| "MangaRead".into());
    let config = crate::config::SiteConfig::from_env();
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
                style { (PreEscaped(config.css_overrides())) }
            }
            body {
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
            }
        }
    }
}

/// Render the reader-specific page layout.
///
/// A minimal layout without the standard site header or footer, intended for
/// the full-screen chapter reader view. Loads the reader stylesheet and the
/// `app.js` script for page navigation and progress tracking.
pub fn reader_layout(title: &str, content: Markup, config_css: &str, site_title: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — " (site_title) }
                link rel="stylesheet" href="/static/style.css";
                style { (PreEscaped(config_css.to_string())) }
            }
            body class="reader-page" {
                (content)
                script src="/static/app.js" {}
            }
        }
    }
}
