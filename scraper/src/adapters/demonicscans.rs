use async_trait::async_trait;
use mangaread_core::models::{ChapterListing, MangaInfo, MangaListing};
use selectrs::{Html, Selector};
use thirtyfour::prelude::*;
use tracing;

use crate::adapter::SiteAdapter;

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
        tracing::info!("navigating to index page: https://demonicscans.org/index.php");
        driver.goto("https://demonicscans.org/index.php").await?;
        tracing::info!("page loaded, waiting 2s for dynamic content...");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let page_source = driver.source().await?;
        tracing::info!("page source fetched ({} bytes), parsing HTML...", page_source.len());
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

    async fn fetch_manga(
        &self,
        driver: &WebDriver,
        detail_url: &str,
    ) -> anyhow::Result<MangaInfo> {
        tracing::info!("navigating to manga detail page: {detail_url}");
        driver.goto(detail_url).await?;
        tracing::info!("page loaded, waiting 2s for dynamic content...");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let page_source = driver.source().await?;
        tracing::info!("page source fetched ({} bytes), parsing HTML...", page_source.len());
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
        let cover_sel =
            Selector::parse("img[class*='cover'], .manga-cover img, img[class*='thumbnail']")
                .unwrap();
        let cover_url = document
            .select(&cover_sel)
            .next()
            .and_then(|e| e.value().attr("src"))
            .map(|s| {
                if s.starts_with("http") {
                    s.to_string()
                } else {
                    format!("https://demonicscans.org{}", s)
                }
            });

        // Extract chapter links
        let ch_sel = Selector::parse(
            "a[href*='/manga/'][href*='chapter'], .chapter-list a, a[href*='capitulo']",
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
                    format!("https://demonicscans.org{}", href)
                };

                let number = extract_chapter_number(&text)
                    .or_else(|| extract_chapter_number(&url))
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
        tracing::info!("navigating to chapter page: {chapter_url}");
        driver.goto(chapter_url).await?;
        tracing::info!("page loaded, waiting 2s for dynamic content...");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let page_source = driver.source().await?;
        tracing::info!("page source fetched ({} bytes), parsing HTML...", page_source.len());
        let document = Html::parse_document(&page_source);

        // Try multiple selector patterns for chapter images
        let img_sel = Selector::parse(
            "img[class*='page-image'], .reader-img img, #reader img, .chapter-content img, img[alt*='page']"
        ).unwrap();

        let mut pages: Vec<String> = Vec::new();
        for img in document.select(&img_sel) {
            if let Some(src) = img
                .value()
                .attr("src")
                .or_else(|| img.value().attr("data-src"))
            {
                let url = if src.starts_with("http") {
                    src.to_string()
                } else {
                    format!("https://demonicscans.org{}", src)
                };
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
                    let url = if src.starts_with("http") {
                        src.to_string()
                    } else {
                        format!("https://demonicscans.org{}", src)
                    };
                    tracing::debug!("found page image via fallback: {url}");
                    pages.push(url);
                }
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

fn extract_chapter_number(s: &str) -> Option<f64> {
    let re = regex_lite::Regex::new(r"(?i)(?:chapter|ch\.?|capitulo)\s*(\d+(?:\.\d+)?)").ok()?;
    if let Some(caps) = re.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().parse::<f64>().ok();
        }
    }
    // Fallback: try to find standalone number at end of URL segment
    let re2 = regex_lite::Regex::new(r"[_-](\d+(?:\.\d+)?)$").ok()?;
    if let Some(caps) = re2.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().parse::<f64>().ok();
        }
    }
    None
}
