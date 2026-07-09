use async_trait::async_trait;
use mangaread_core::{ChapterListing, MangaInfo, MangaListing};
use selectrs::{Html, Selector};
use thirtyfour::prelude::*;
use tracing;

use crate::adapter::SiteAdapter;
use crate::adapters::helpers;

pub struct ManhuaplusAdapter;

#[async_trait]
impl SiteAdapter for ManhuaplusAdapter {
    fn source_name(&self) -> &str {
        "manhuaplus"
    }

    fn base_url(&self) -> &str {
        "https://manhuaplus.top"
    }

    async fn discover_manga(&self, driver: &WebDriver) -> anyhow::Result<Vec<MangaListing>> {
        tracing::info!("navigating to listing page: https://manhuaplus.top/all-manga/1");
        driver.goto("https://manhuaplus.top/all-manga/1").await?;
        helpers::wait_for_page(driver, "manhuaplus listing").await;

        let page_source = driver.source().await?;
        tracing::info!("page source fetched ({} bytes), parsing HTML...", page_source.len());
        let document = Html::parse_document(&page_source);

        // Manga entries are in items with a[href*='/manga/']
        let link_selector = Selector::parse("a[href*='/manga/']").unwrap();
        let img_selector = Selector::parse("img").unwrap();

        let mut manga = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for element in document.select(&link_selector) {
            let href = element.value().attr("href").unwrap_or("");
            // Only capture detail-page links (not chapter links, not nav links)
            let path = href.strip_prefix("https://manhuaplus.top").or_else(|| Some(href)).unwrap();
            // A manga detail link looks like /manga/{slug} with no further segments
            if !path.starts_with("/manga/") || path.matches('/').count() > 2 {
                continue;
            }

            let detail_url = if href.starts_with("http") {
                href.to_string()
            } else {
                helpers::make_absolute(self.base_url(), href)
            };

            if !seen.insert(detail_url.clone()) {
                continue;
            }

            let title = element
                .value()
                .attr("title")
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    let t = element.text().collect::<String>().trim().to_string();
                    if t.is_empty() { None } else { Some(t) }
                })
                .unwrap_or_else(|| "Unknown".to_string());

            // Try to find cover from a child img or parent wrapper
            let cover_url = element
                .select(&img_selector)
                .next()
                .and_then(|img| {
                    img.value()
                        .attr("data-original")
                        .or_else(|| img.value().attr("src"))
                })
                .filter(|s| !s.starts_with("data:"))
                .map(|s| {
                    if s.starts_with("http") {
                        s.to_string()
                    } else {
                        helpers::make_absolute(self.base_url(), s)
                    }
                });

            tracing::debug!(
                "found manga: title=\"{}\" url={} cover={}",
                title,
                detail_url,
                cover_url.as_deref().unwrap_or("none"),
            );

            manga.push(MangaListing {
                title,
                detail_url,
                cover_url,
            });
        }

        tracing::info!("discovery complete: {} manga found", manga.len());
        Ok(manga)
    }

    async fn fetch_manga(
        &self,
        driver: &WebDriver,
        detail_url: &str,
    ) -> anyhow::Result<MangaInfo> {
        tracing::info!("navigating to manga detail page: {detail_url}");
        driver.goto(detail_url).await?;
        helpers::wait_for_page(driver, "manhuaplus").await;

        let page_source = driver.source().await?;
        tracing::info!("page source fetched ({} bytes), parsing HTML...", page_source.len());
        let document = Html::parse_document(&page_source);

        // Title
        let title_sel = Selector::parse("h1.title-detail").unwrap();
        let title = document
            .select(&title_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        // Description — try meta description
        let desc_sel = Selector::parse("meta[name='description']").unwrap();
        let description = document
            .select(&desc_sel)
            .next()
            .and_then(|e| e.value().attr("content"))
            .map(|s| s.to_string());

        // Author
        let author_sel = Selector::parse(".author, .artist, [class*='author']").unwrap();
        let author = document
            .select(&author_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        // Cover — try the detail-page img first
        let cover_sel = Selector::parse(
            ".detail-info img, img[class*='cover'], meta[itemprop='image']",
        )
        .unwrap();
        let cover_url = document
            .select(&cover_sel)
            .next()
            .and_then(|e| {
                if e.value().name() == "meta" {
                    e.value().attr("content")
                } else {
                    e.value().attr("src")
                }
            })
            .map(|s| {
                if s.starts_with("http") {
                    s.to_string()
                } else {
                    helpers::make_absolute(self.base_url(), s)
                }
            });

        // Chapter list
        let ch_sel = Selector::parse(
            "#nt_listchapter a[href*='/manga/'], .list-chapter a[href*='chapter']",
        )
        .unwrap();
        let chapters: Vec<ChapterListing> = document
            .select(&ch_sel)
            .filter_map(|e| {
                let href = e.value().attr("href")?;
                let text = e.text().collect::<String>().trim().to_string();
                let url = if href.starts_with("http") {
                    href.to_string()
                } else {
                    helpers::make_absolute(self.base_url(), href)
                };

                let number = helpers::extract_chapter_number(&text)
                    .or_else(|| helpers::extract_chapter_number(&url))
                    .unwrap_or(0.0);

                let title = if text.is_empty() || text == format!("Chapter {}", number as u64) {
                    None
                } else {
                    Some(text)
                };

                Some(ChapterListing {
                    title,
                    number,
                    url,
                })
            })
            .collect();

        tracing::info!(
            "parsed manga: title=\"{}\" author={} description={} cover={} chapters={}",
            title,
            author.as_deref().unwrap_or("none"),
            if description.is_some() { "found" } else { "none" },
            cover_url.as_deref().unwrap_or("none"),
            chapters.len(),
        );

        Ok(MangaInfo {
            source: self.source_name().to_string(),
            source_id: detail_url.to_string(),
            title,
            author,
            description,
            cover_url,
            detail_url: detail_url.to_string(),
            chapters,
        })
    }

    async fn fetch_pages(
        &self,
        driver: &WebDriver,
        chapter_url: &str,
    ) -> anyhow::Result<Vec<String>> {
        driver.goto(chapter_url).await?;
        helpers::wait_for_page(driver, "manhuaplus chapter").await;

        let page_source = driver.source().await?;
        tracing::info!("page source fetched ({} bytes), parsing HTML...", page_source.len());
        let document = Html::parse_document(&page_source);

        // Page images are in div.page-chapter img
        let img_sel = Selector::parse("div.page-chapter img").unwrap();
        let mut pages: Vec<String> = Vec::new();
        for img in document.select(&img_sel) {
            let src = img
                .value()
                .attr("src")
                .or_else(|| img.value().attr("data-src"))
                .or_else(|| img.value().attr("data-original"));
            if let Some(url) = src {
                let full_url = if url.starts_with("http") {
                    url.to_string()
                } else {
                    helpers::make_absolute(self.base_url(), url)
                };
                tracing::debug!("found page image: {full_url}");
                pages.push(full_url);
            }
        }

        tracing::info!(
            "page extraction complete: {} pages found{}",
            pages.len(),
            if pages.is_empty() { " (no images detected!)" } else { "" }
        );
        Ok(pages)
    }
}

