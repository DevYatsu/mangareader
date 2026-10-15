//! Chapter comments and their moderation reports.

/// One stored comment, with its author's display name joined in.
///
/// Comments attach to `(series_id, chapter_number)`, not to a chapter row:
/// the same chapter from two sources is two rows, and keying by row would
/// split one discussion in two.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub id: i64,
    pub series_id: i64,
    pub chapter_number: f64,
    pub user_id: i64,
    pub author: String,
    /// Top-level comments have none. Replies are one level deep.
    pub parent_id: Option<i64>,
    pub body: String,
    pub spoiler: bool,
    pub created_at: String,
    pub edited_at: Option<String>,
    /// Set by a moderator; only moderators still see the body.
    pub hidden_at: Option<String>,
    /// Set by the author; nobody sees the body.
    pub deleted_at: Option<String>,
}

/// Fields needed to post a comment.
#[derive(Debug, Clone, PartialEq)]
pub struct NewComment {
    pub series_id: i64,
    pub chapter_number: f64,
    pub user_id: i64,
    pub parent_id: Option<i64>,
    pub body: String,
    pub spoiler: bool,
}

/// Open reports on one comment, grouped for the moderation queue.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportGroup {
    pub comment: Comment,
    pub series_title: String,
    pub report_count: i64,
    /// Distinct non-empty reasons given by reporters.
    pub reasons: Vec<String>,
    pub first_reported_at: String,
}
