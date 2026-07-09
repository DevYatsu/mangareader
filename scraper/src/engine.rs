use mangaread_core::db;
use mangaread_core::{NewChapter, NewManga, NewPage};
use rusqlite::Connection;
use thirtyfour::WebDriver;
use tracing;

use crate::adapter::SiteAdapter;

pub struct ScraperEngine {
    adapters: Vec<Box<dyn SiteAdapter>>,
}

impl ScraperEngine {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    pub fn add_adapter(&mut self, adapter: Box<dyn SiteAdapter>) {
        self.adapters.push(adapter);
    }

    /// Run discover_manga for all adapters and upsert into DB.
    pub async fn scan_all(&self, driver: &WebDriver, db: &Connection) -> anyhow::Result<()> {
        for adapter in &self.adapters {
            let source = adapter.source_name();
            tracing::info!("scanning {source} for manga...");

            let listings = adapter.discover_manga(driver).await?;
            tracing::info!("found {} manga on {source}", listings.len());

            for listing in listings {
                tracing::debug!("saving manga: \"{}\" ({})", listing.title, listing.detail_url);
                let new_manga = NewManga {
                    source: source.to_string(),
                    source_id: listing.detail_url.clone(),
                    title: listing.title.clone(),
                    author: None,
                    cover_url: listing.cover_url,
                    description: None,
                    detail_url: listing.detail_url,
                    series_id: None,
                };
                let id = db::insert_manga(db, &new_manga)?;
                // Auto-link to a series by normalized title
                db::ensure_manga_in_series(db, &listing.title, id)?;
                tracing::debug!("saved manga id={} \"{}\"", id, new_manga.title);
            }
        }
        Ok(())
    }

    /// For a given manga ID, visit its detail page and upsert chapters.
    pub async fn sync_manga(&self, driver: &WebDriver, db: &Connection, manga_id: i64) -> anyhow::Result<()> {
        let manga = db::get_manga_by_id(db, manga_id)?;

        let adapter = self
            .adapters
            .iter()
            .find(|a| a.source_name() == manga.source)
            .ok_or_else(|| anyhow::anyhow!("no adapter for source {}", manga.source))?;

        tracing::info!("syncing manga: {} ({})", manga.title, manga.detail_url);

        let info = adapter.fetch_manga(driver, &manga.detail_url).await?;

        // Ensure manga is linked to a series (for newly inserted or pre-feature entries)
        let series_id = if let Some(sid) = manga.series_id {
            sid
        } else {
            db::ensure_manga_in_series(db, &info.title, manga_id)?
        };

        // Update manga metadata (preserve series link from DB)
        let updated = NewManga {
            source: manga.source.clone(),
            source_id: manga.source_id.clone(),
            title: info.title.clone(),
            author: info.author,
            cover_url: info.cover_url,
            description: info.description,
            detail_url: info.detail_url,
            series_id: Some(series_id),
        };
        db::insert_manga(db, &updated)?;

        let chapter_count = info.chapters.len();
        for ch in info.chapters {
            tracing::debug!("saving chapter: ch.{} \"{}\" ({})", ch.number, ch.title.as_deref().unwrap_or(""), ch.url);
            let new_chapter = NewChapter {
                manga_id,
                source_id: ch.url.clone(),
                number: ch.number,
                title: ch.title,
                url: ch.url,
            };
            let ch_id = db::insert_chapter(db, &new_chapter)?;
            tracing::debug!("saved chapter id={} ch.{}", ch_id, new_chapter.number);
        }

        // Mark when chapters were last fetched
        db::set_chapters_synced(db, manga_id)?;

        // Update the series title with the clean detail-page title
        db::update_series_title(db, series_id, &info.title)?;

        tracing::info!("synced {chapter_count} chapters for manga {manga_id}",);
        Ok(())
    }

    /// For a given chapter ID, visit its page and upsert page image URLs.
    pub async fn sync_chapter(&self, driver: &WebDriver, db: &Connection, chapter_id: i64) -> anyhow::Result<()> {
        let chapter = db::get_chapter_by_id(db, chapter_id)?;
        let manga = db::get_manga_by_id(db, chapter.manga_id)?;

        let adapter = self
            .adapters
            .iter()
            .find(|a| a.source_name() == manga.source)
            .ok_or_else(|| anyhow::anyhow!("no adapter for source {}", manga.source))?;

        tracing::info!(
            "syncing chapter: {} ch.{} ({})",
            manga.title,
            chapter.number,
            chapter.url
        );

        let page_urls = adapter.fetch_pages(driver, &chapter.url).await?;
        tracing::info!("fetched {} page URLs for chapter {}", page_urls.len(), chapter_id);

        for (i, url) in page_urls.iter().enumerate() {
            tracing::debug!("saving page {}: {url}", i + 1);
            let new_page = NewPage {
                chapter_id,
                index: (i + 1) as i32,
                image_url: url.clone(),
            };
            db::insert_page(db, &new_page)?;
        }

        tracing::info!(
            "synced {} pages for chapter {}",
            page_urls.len(),
            chapter_id
        );
        Ok(())
    }
}
