//! Identity domain types: users, roles, bookmarks, progress, history, notes.
//!
//! No hashing, no HTTP. Pure data types only.

/// Account role. Ordering: Member < Moderator < Admin.
///
/// Anonymous visitors have no role: they are the absence of a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Member,
    /// Moderates community content (comments, reports). No catalog or site
    /// settings access; those stay admin-only.
    Moderator,
    Admin,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Member => "member",
            Role::Moderator => "moderator",
            Role::Admin => "admin",
        }
    }

    pub fn from_str(s: &str) -> Role {
        match s {
            "admin" => Role::Admin,
            // `editor` is the pre-2026-10 name of the same rank; init_db rewrites it.
            "moderator" | "editor" => Role::Moderator,
            _ => Role::Member,
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Role::Member => 0,
            Role::Moderator => 1,
            Role::Admin => 2,
        }
    }

    pub fn at_least(&self, min: &Role) -> bool {
        self.rank() >= min.rank()
    }
}

/// A registered reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: Role,
    pub locale: String,
    pub created_at: String,
    /// Set when an admin banned the account. Banned users cannot log in and
    /// their sessions stop resolving.
    pub banned_at: Option<String>,
}

/// One moderation audit-log row, with actor and target resolved to names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModLogEntry {
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub detail: String,
    pub created_at: String,
}

/// Fields needed to create a user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUser {
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: Role,
}

/// Reading progress on a chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub user_id: i64,
    pub chapter_id: i64,
    pub page: i64,
    pub status: String,
    pub updated_at: String,
}

/// A bookmark linking a user to a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub user_id: i64,
    pub series_id: i64,
    pub created_at: String,
}

/// A history entry recording a chapter read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: i64,
    pub user_id: i64,
    pub chapter_id: i64,
    pub read_at: String,
}

/// A note attached to a chapter page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: i64,
    pub user_id: i64,
    pub chapter_id: i64,
    pub page: i64,
    pub body: String,
    pub created_at: String,
}
