use mangaread_core::db;
use mangaread_core::models::{NewChapter, NewManga, NewPage};
use thirtyfour::WebDriver;
use tracing;

use crate::adapter::SiteAdapter;

pub struct ScraperEngine {
    adapters: Vec<Box<dyn SiteAdapter>>,
    db: rusqlite::Connection,
}

impl ScraperEngine {
    pub fn new(db: rusqlite::Connection) -> Self {
        Self {
            adapters: Vec::new(),
            db,
        }
    }

    pub fn add_adapter(&mut self, adapter: Box<dyn SiteAdapter>) {
        self.adapters.push(adapter);
    }

    /// Run discover_manga for all adapters and upsert into DB.
    pub async fn scan_all(&mut self, driver: &WebDriver) -> anyhow::Result<()> {
        for adapter in &self.adapters {
            let source = adapter.source_name();
            tracing::info!("scanning {source} for manga...");

            let listings = adapter.discover_manga(driver).await?;
            tracing::info!("found {} manga on {source}", listings.len());

            for listing in listings {
                let new_manga = NewManga {
                    source: source.to_string(),
                    source_id: listing.detail_url.clone(),
                    title: listing.title,
                    author: None,
                    cover_url: listing.cover_url,
                    description: None,
                    detail_url: listing.detail_url,
                };
                db::insert_manga(&self.db, &new_manga)?;
            }
        }
        Ok(())
    }

    /// For a given manga ID, visit its detail page and upsert chapters.
    pub async fn sync_manga(&mut self, driver: &WebDriver, manga_id: i64) -> anyhow::Result<()> {
        let manga = db::get_manga_by_id(&self.db, manga_id)?;

        let adapter = self
            .adapters
            .iter()
            .find(|a| a.source_name() == manga.source)
            .ok_or_else(|| anyhow::anyhow!("no adapter for source {}", manga.source))?;

        tracing::info!("syncing manga: {} ({})", manga.title, manga.detail_url);

        let info = adapter.fetch_manga(driver, &manga.detail_url).await?;

        // Update manga metadata
        let updated = NewManga {
            source: manga.source.clone(),
            source_id: manga.source_id.clone(),
            title: info.title,
            author: info.author,
            cover_url: info.cover_url,
            description: info.description,
            detail_url: info.detail_url,
        };
        db::insert_manga(&self.db, &updated)?;

        let chapter_count = info.chapters.len();
        for ch in info.chapters {
            let new_chapter = NewChapter {
                manga_id,
                source_id: ch.url.clone(),
                number: ch.number,
                title: ch.title,
                url: ch.url,
            };
            db::insert_chapter(&self.db, &new_chapter)?;
        }

        tracing::info!("synced {chapter_count} chapters for manga {manga_id}",);
        Ok(())
    }

    /// For a given chapter ID, visit its page and upsert page image URLs.
    pub async fn sync_chapter(&mut self, driver: &WebDriver, chapter_id: i64) -> anyhow::Result<()> {
        let chapter = db::get_chapter_by_id(&self.db, chapter_id)?;
        let manga = db::get_manga_by_id(&self.db, chapter.manga_id)?;

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

        for (i, url) in page_urls.iter().enumerate() {
            let new_page = NewPage {
                chapter_id,
                index: (i + 1) as i32,
                image_url: url.clone(),
            };
            db::insert_page(&self.db, &new_page)?;
        }

        tracing::info!(
            "synced {} pages for chapter {}",
            page_urls.len(),
            chapter_id
        );
        Ok(())
    }

    pub fn into_db(self) -> rusqlite::Connection {
        self.db
    }
}
