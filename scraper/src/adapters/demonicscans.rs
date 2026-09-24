use async_trait::async_trait;
use mangaread_core::{ChapterListing, MangaInfo, MangaListing};
use selectrs::{Html, Selector};
use thirtyfour::prelude::*;
use tracing;

use crate::adapter::SiteAdapter;
use crate::adapters::helpers;

/// Adapter for [Demonicscans.org](https://demonicscans.org).
///
/// - **Listing page**: `/index.php` — finds `<a>` links whose `href` contains `/manga/`.
/// - **Detail page**: extracts the `<h1>` title, meta description, author selectors,
///   cover image, and chapter links from `#chapters-list`.
/// - **Chapter page**: matches page images against multiple selector patterns and falls
///   back to all `<img>` tags (filtering out logos/icons/avatars).
pub struct DemonicscansAdapter;

#[async_trait]
impl SiteAdapter for DemonicscansAdapter {
    fn source_name(&self) -> &str {
        "demonicscans"
    }

    fn base_url(&self) -> &str {
        "https://demonicscans.org"
    }

    async fn discover_manga(&self, driver: &WebDriver) -> anyhow::Result<Vec<MangaListing>> {
        driver.goto("https://demonicscans.org/index.php").await?;
        helpers::wait_for_page(driver, "demonicscans index").await;

        let page_source = driver.source().await?;
        tracing::info!(
            "page source fetched ({} bytes), parsing HTML...",
            page_source.len()
        );
        let document = Html::parse_document(&page_source);

        // Find manga entry links
        let link_selector = Selector::parse("a[href*='/manga/']").unwrap();
        let img_selector = Selector::parse("img").unwrap();

        let mut manga = Vec::new();
        let mut skipped = 0u32;
        for element in document.select(&link_selector) {
            let href = element.value().attr("href").unwrap_or("");
            // Skip non-manga links
            if href.len() < 10 || href == "/" || !href.contains("/manga/") {
                skipped += 1;
                continue;
            }

            // Make full URL
            let detail_url = if href.starts_with("http") {
                href.to_string()
            } else {
                format!("https://demonicscans.org{}", href)
            };

            // Try to get title from element text or alt text of child img
            let title = element.text().collect::<String>().trim().to_string();
            let title = if title.is_empty() {
                element
                    .select(&img_selector)
                    .next()
                    .and_then(|img| img.value().attr("alt"))
                    .unwrap_or("Unknown")
                    .to_string()
            } else {
                title
            };

            let cover_url = element
                .select(&img_selector)
                .next()
                .and_then(|img| img.value().attr("src"))
                .map(|s| {
                    if s.starts_with("http") {
                        s.to_string()
                    } else {
                        format!("https://demonicscans.org{}", s)
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

        tracing::info!(
            "discovery complete: {} manga found ({} links skipped)",
            manga.len(),
            skipped
        );
        Ok(manga)
    }

    async fn fetch_manga(&self, driver: &WebDriver, detail_url: &str) -> anyhow::Result<MangaInfo> {
        driver.goto(detail_url).await?;
        helpers::wait_for_page(driver, &format!("demonicscans detail")).await;

        let page_source = driver.source().await?;
        tracing::info!(
            "page source fetched ({} bytes), parsing HTML...",
            page_source.len()
        );
        let document = Html::parse_document(&page_source);

        // Extract title from the page
        let title_sel = Selector::parse("h1").unwrap();
        let title = document
            .select(&title_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        // Extract description
        let desc_sel = Selector::parse("meta[name='description']").unwrap();
        let description = document
            .select(&desc_sel)
            .next()
            .and_then(|e| e.value().attr("content"))
            .map(|s| s.to_string());

        // Extract author
        let author_sel = Selector::parse(".author, .artist, [class*='author']").unwrap();
        let author = document
            .select(&author_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        // Extract cover image
        let cover_sel = Selector::parse(
            "img[class*='cover'], .manga-cover img, img[class*='thumbnail'], #manga-page img",
        )
        .unwrap();
        let cover_url = document
            .select(&cover_sel)
            .next()
            .and_then(|e| e.value().attr("src"))
            .map(|s| helpers::make_absolute(self.base_url(), s));

        // Extract chapter links
        let ch_sel = Selector::parse("#chapters-list a, a.chplinks, a[href*='chapter=']").unwrap();
        let chapters: Vec<ChapterListing> = document
            .select(&ch_sel)
            .filter_map(|e| {
                let href = e.value().attr("href")?;
                let text = e.text().collect::<String>().trim().to_string();
                let url = helpers::make_absolute(self.base_url(), href);

                let number = helpers::extract_chapter_number(&text)
                    .or_else(|| helpers::extract_chapter_number(&url))
                    .unwrap_or(0.0);

                Some(ChapterListing {
                    title: if text.is_empty() { None } else { Some(text) },
                    number,
                    url,
                })
            })
            .collect();

        tracing::info!(
            "parsed manga: title=\"{title}\" author={} description={} cover={} chapters={}",
            author.as_deref().unwrap_or("none"),
            if description.is_some() {
                "found"
            } else {
                "none"
            },
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
        helpers::wait_for_page(driver, "demonicscans chapter").await;

        let page_source = driver.source().await?;
        tracing::info!(
            "page source fetched ({} bytes), parsing HTML...",
            page_source.len()
        );
        let document = Html::parse_document(&page_source);

        // Try multiple selector patterns for chapter images
        let img_sel = Selector::parse(
            ".imgholder, img[class*='page-image'], .reader-img img, #reader img, .chapter-content img, img[alt*='page']"
        ).unwrap();

        let mut pages: Vec<String> = Vec::new();
        for img in document.select(&img_sel) {
            if let Some(src) = img
                .value()
                .attr("src")
                .or_else(|| img.value().attr("data-src"))
            {
                let url = helpers::make_absolute(self.base_url(), src);
                tracing::debug!("found page image via primary selector: {url}");
                pages.push(url);
            }
        }

        // Fallback: try all img tags
        if pages.is_empty() {
            tracing::warn!("primary selectors found no images, trying fallback (all img tags)");
            let all_img = Selector::parse("img").unwrap();
            for img in document.select(&all_img) {
                if let Some(src) = img.value().attr("src") {
                    if src.contains("logo") || src.contains("icon") || src.contains("avatar") {
                        tracing::debug!("skipping non-page image: {src}");
                        continue;
                    }
                    let url = helpers::make_absolute(self.base_url(), src);
                    tracing::debug!("found page image via fallback: {url}");
                    pages.push(url);
                }
            }
        }

        tracing::info!(
            "page extraction complete: {} pages found{}",
            pages.len(),
            if pages.is_empty() {
                " (no images detected!)"
            } else {
                ""
            }
        );
        Ok(pages)
    }
}
