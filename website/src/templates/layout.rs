use maud::{html, Markup, DOCTYPE};

/// Render the standard page layout (HTML document shell).
///
/// Includes the `<title>`, Google Fonts preconnects, stylesheet, site header,
/// main content area, and footer. The caller supplies the page title and
/// inner body content as Maud `Markup`.
///
/// If `sync_banner` is `Some(...)`, a banner is shown at the top of the
/// page indicating the background sync loop is running.
pub fn base_layout(title: &str, content: Markup, sync_banner: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — MangaRead" }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=Inter:wght@400;500;600&display=swap";
                link rel="stylesheet" href="/static/style.css";
            }
            body {
                @if let Some(text) = sync_banner {
                    div class="sync-banner" {
                        span class="sync-banner-spinner" {}
                        (text)
                    }
                }
                div.container {
                    header class="site-header" {
                        a class="site-logo" href="/" { "MangaRead" }
                        nav class="site-nav" {
                            a href="/" {
                                svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                    path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" {}
                                    polyline points="9 22 9 12 15 12 15 22" {}
                                }
                                "Home"
                            }
                            a href="/series" {
                                svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                    rect x="3" y="3" width="7" height="7" {}
                                    rect x="14" y="3" width="7" height="7" {}
                                    rect x="14" y="14" width="7" height="7" {}
                                    rect x="3" y="14" width="7" height="7" {}
                                }
                                "Series"
                            }
                        }
                    }
                    main { (content) }
                    footer {
                        p { "MangaRead" }
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
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (heading) " — MangaRead" }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=Inter:wght@400;500;600&display=swap";
                link rel="stylesheet" href="/static/style.css";
            }
            body {
                div.container {
                    header class="site-header" {
                        a class="site-logo" href="/" { "MangaRead" }
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
                        p { "MangaRead" }
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
pub fn reader_layout(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — MangaRead" }
                link rel="stylesheet" href="/static/style.css";
            }
            body class="reader-page" {
                (content)
                script src="/static/app.js" {}
            }
        }
    }
}
