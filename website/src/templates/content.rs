use maud::{Markup, html};

fn cover_figure(title: &str, cover_url: Option<&str>, badge: Option<&str>) -> Markup {
    html! {
        figure class="image is-2by3 m-cover" {
            @if let Some(url) = cover_url {
                img src=(url) alt=(title) loading="lazy";
            } @else {
                span data-cover-placeholder aria-hidden="true" {}
                span data-cover-initial { (title.chars().next().unwrap_or('?').to_uppercase().to_string()) }
            }
            @if let Some(label) = badge {
                span class="tag is-primary is-light" data-cover-badge { (label) }
            }
            span data-cover-overlay aria-hidden="true" {}
        }
    }
}

/// Render a linked or unlinked cover frame using the shared content primitive.
pub fn cover(
    href: Option<&str>,
    title: &str,
    cover_url: Option<&str>,
    badge: Option<&str>,
) -> Markup {
    let figure = cover_figure(title, cover_url, badge);
    if let Some(url) = href {
        html! { a href=(url) aria-label=(title) { (figure) } }
    } else {
        figure
    }
}

/// Wrap already-rendered cards in the shared Bulma grid.
pub fn manga_grid(cards: &[Markup]) -> Markup {
    html! {
        div class="columns is-mobile is-multiline" {
            @for card in cards {
                div class="column is-6-mobile is-4-tablet is-3-desktop" { (card) }
            }
        }
    }
}

/// Render one server-side manga card. The surrounding grid is owned by the
/// route so cards remain siblings and share the same Bulma breakpoints.
pub fn manga_card(href: &str, title: &str, cover_url: Option<&str>, badge: Option<&str>) -> Markup {
    let figure = cover(Some(href), title, cover_url, badge);
    html! {
        div class="card" {
            div class="card-image" { (figure) }
            div class="card-content" {
                a href=(href) {
                    p class="title is-6" { (title) }
                }
            }
        }
    }
}

/// Render an empty state with the muted icon slot shared by all content pages.
pub fn empty_state(message: &str, show_icon: bool) -> Markup {
    html! {
        div class="m-empty" {
            @if show_icon {
                span class="m-empty-icon" aria-hidden="true" {}
            }
            p { (message) }
        }
    }
}

/// Bookmark shell needs the same empty-state markup plus its existing id and
/// hidden initial state, so keep the optional wrapper as a small companion
/// rather than making the general helper carry page-specific concerns.
pub fn empty_state_with_id(message: &str, show_icon: bool, id: &str, hidden: bool) -> Markup {
    let style = if hidden { "display:none" } else { "" };
    html! {
        div id=(id) class="m-empty" style=(style) {
            @if show_icon {
                span class="m-empty-icon" aria-hidden="true" {}
            }
            p { (message) }
        }
    }
}

/// Render a Bulma breadcrumb and mark the final item as the current page.
pub fn breadcrumb(items: &[(&str, &str)]) -> Markup {
    html! {
        nav class="breadcrumb has-arrow-separator" aria-label="breadcrumb" {
            ul {
                @for (index, (href, label)) in items.iter().enumerate() {
                    @if index + 1 == items.len() {
                        li class="is-active" {
                            a href=(href) aria-current="page" { (label) }
                        }
                    } @else {
                        li {
                            a href=(href) { (label) }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markup(value: Markup) -> String {
        value.into_string()
    }

    #[test]
    fn manga_grid_uses_shared_bulma_grid_shape() {
        let card = manga_card("/series/1", "Title", Some("/cover.jpg"), Some("12 Chs."));
        let markup = markup(manga_grid(&[card]));
        assert!(markup.contains("columns is-mobile is-multiline"));
        assert!(markup.contains("column is-6-mobile is-4-tablet is-3-desktop"));
        assert!(markup.contains("card"));
        assert!(markup.contains("image is-2by3 m-cover"));
        assert!(!markup.contains("manga-grid"));
        assert!(!markup.contains("manga-card"));
        assert!(!markup.contains("cover-wrapper"));
    }

    #[test]
    fn shared_helpers_escape_server_values() {
        let markup = markup(manga_card(
            "/series/<1>",
            "A < B & C",
            Some("/cover?x=1&y=2"),
            Some("1 < 2 & more"),
        ));
        assert!(markup.contains("A &lt; B &amp; C"));
        assert!(markup.contains("/cover?x=1&amp;y=2"));
        assert!(markup.contains("1 &lt; 2 &amp; more"));
        assert!(!markup.contains("< B & C"));
    }

    #[test]
    fn placeholder_cover_has_initial_and_visual_hooks() {
        let markup = markup(cover(None, "Z", None, None));
        assert!(markup.contains("data-cover-placeholder"));
        assert!(markup.contains("data-cover-initial"));
        assert!(markup.contains(">Z<"));
    }

    #[test]
    fn empty_state_uses_muted_icon_slot() {
        let markup = markup(empty_state("Nothing here", true));
        assert!(markup.contains("m-empty"));
        assert!(markup.contains("m-empty-icon"));
    }

    #[test]
    fn breadcrumb_marks_the_last_item_current() {
        let markup = markup(breadcrumb(&[("/", "Home"), ("/series", "Series")]));
        assert!(markup.contains("breadcrumb has-arrow-separator"));
        assert!(markup.contains("is-active"));
        assert!(markup.contains("aria-current=\"page\""));
    }
}
