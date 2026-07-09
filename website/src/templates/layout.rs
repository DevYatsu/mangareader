use maud::{html, Markup, DOCTYPE};

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
