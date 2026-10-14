//! A member's library: bookmarked series sorted onto shelves.

/// Where a series sits in a member's library. A bookmark is a library
/// entry; new bookmarks land on `Reading`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shelf {
    Reading,
    Plan,
    Completed,
    OnHold,
    Dropped,
}

impl Shelf {
    pub const ALL: [Shelf; 5] = [
        Shelf::Reading,
        Shelf::Plan,
        Shelf::Completed,
        Shelf::OnHold,
        Shelf::Dropped,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Shelf::Reading => "reading",
            Shelf::Plan => "plan",
            Shelf::Completed => "completed",
            Shelf::OnHold => "on_hold",
            Shelf::Dropped => "dropped",
        }
    }

    pub fn parse(s: &str) -> Option<Shelf> {
        Shelf::ALL.into_iter().find(|shelf| shelf.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Shelf::Reading => "Reading",
            Shelf::Plan => "Plan to read",
            Shelf::Completed => "Completed",
            Shelf::OnHold => "On hold",
            Shelf::Dropped => "Dropped",
        }
    }
}

/// Where to pick a series back up.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinueItem {
    pub series_id: i64,
    pub series_title: String,
    pub cover_url: Option<String>,
    pub manga_id: i64,
    pub chapter_id: i64,
    pub chapter_number: f64,
    /// 1-indexed page to resume at (1 when starting the next chapter).
    pub page: i64,
    /// `true` when this is the chapter after a finished one.
    pub is_next: bool,
}
