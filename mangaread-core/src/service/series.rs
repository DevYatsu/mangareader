use rusqlite::Connection;

use crate::db;
use crate::domain::*;

/// Result of merging chapters from multiple manga entries.
#[derive(Clone)]
pub struct MergedChapter {
    pub number: f64,
    pub source: String,
    pub manga_id: i64,
    pub chapter_id: i64,
}

/// Pick the "best" manga entry from a list — the one with the most chapters.
pub fn pick_primary<'a>(entries: &'a [Manga], db: &Connection) -> Option<&'a Manga> {
    entries
        .iter()
        .max_by_key(|m| db::list_chapters_by_manga(db, m.id).map(|c| c.len()).unwrap_or(0))
}

/// Pick the preferred source — the one with the most chapters.
pub fn preferred_source<'a>(entries: &'a [Manga], db: &Connection) -> Option<&'a str> {
    pick_primary(entries, db).map(|m| m.source.as_str())
}

/// Merge chapters from all manga entries into a single sorted list.
///
/// For each chapter number that exists in multiple sources, the preferred
/// source's version is used.  Chapters only available on one source use
/// that source regardless of preference.
pub fn merge_chapters(
    entries: &[Manga],
    prefer_source: &str,
    db: &Connection,
) -> Vec<MergedChapter> {
    // number -> (source, manga_id, chapter_id)
    let mut merged: Vec<(f64, String, i64, i64)> = Vec::new();

    for m in entries {
        if let Ok(chs) = db::list_chapters_by_manga(db, m.id) {
            for ch in &chs {
                let num = ch.number;
                let src = m.source.clone();
                // Check if we already have this chapter number
                if let Some(existing) = merged.iter_mut().find(|(n, ..)| (*n - num).abs() < f64::EPSILON) {
                    // Overwrite with preferred source
                    if src == prefer_source {
                        *existing = (num, src, m.id, ch.id);
                    }
                } else {
                    merged.push((num, src, m.id, ch.id));
                }
            }
        }
    }

    // Sort ascending by number
    merged.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    merged
        .into_iter()
        .rev() // newest first for display
        .map(|(num, src, manga_id, ch_id)| MergedChapter {
            number: num,
            source: src,
            manga_id,
            chapter_id: ch_id,
        })
        .collect()
}

/// Find the first story chapter (nearest to 1.0) for the "Start Reading" button.
pub fn first_chapter(entries: &[Manga], db: &Connection) -> Option<MergedChapter> {
    let prefer = preferred_source(entries, db).unwrap_or("");
    let merged = merge_chapters(entries, prefer, db);
    merged
        .iter()
        .rev() // oldest first (merge_chapters returns newest-first)
        .find(|c| (c.number - 1.0).abs() < f64::EPSILON)
        .or_else(|| merged.last())
        .cloned()
}

/// Check if the primary manga's chapters are stale (never synced or older than
/// `stale_minutes`).
pub fn is_series_fresh(entries: &[Manga], db: &Connection, stale_minutes: i64) -> bool {
    pick_primary(entries, db)
        .and_then(|m| db::is_manga_fresh(db, m.id, stale_minutes).ok())
        .unwrap_or(false)
}
