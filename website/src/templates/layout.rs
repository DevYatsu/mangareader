use maud::{html, Markup, DOCTYPE};

/// Render the standard page layout (HTML document shell).
///
/// Includes the `<title>`, Google Fonts preconnects, stylesheet, site header,
/// main content area, and footer. The caller supplies the page title and
/// inner body content as Maud `Markup`.
pub fn base_layout(title: &str, content: Markup) -> Markup {
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
                div.container {
                    header {
                        h1 { a href="/" { "MangaRead" } }
                    }
                    main { (content) }
                    footer {
                        p { "MangaRead — training project" }
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
/// `reader.js` script for page navigation and progress tracking.
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
                script src="/static/reader.js" {}
            }
        }
    }
}
