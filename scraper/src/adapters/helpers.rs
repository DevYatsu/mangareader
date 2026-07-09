use thirtyfour::WebDriver;
use tracing;

/// Wait a fixed duration for dynamic page content to load.
///
/// All scraped sites need a brief pause after navigation while
/// JavaScript renders the DOM.  The constant 2-second sleep is
/// coarse but works for all current adapters.
pub async fn wait_for_page(_driver: &WebDriver, label: &str) {
    tracing::info!("{label}: page loaded, waiting 2s for dynamic content...");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
}

/// Resolve a possibly-relative URL against a base origin.
pub fn make_absolute(base_origin: &str, href: &str) -> String {
    if href.starts_with("http") || href.starts_with("//") {
        // already absolute, or protocol-relative
        if href.starts_with("//") {
            // protocol-relative — prepend https:
            format!("https:{}", href)
        } else {
            href.to_string()
        }
    } else if href.starts_with('/') {
        // absolute path
        format!("{}{}", base_origin.trim_end_matches('/'), href)
    } else {
        // relative path
        format!("{}/{}", base_origin.trim_end_matches('/'), href.trim_start_matches('/'))
    }
}

/// Extract a chapter number from text like "Chapter 124" or URLs like
/// ".../chapter-124" or ".../chapter-522-6" (for 522.6).
pub fn extract_chapter_number(s: &str) -> Option<f64> {
    // Primary: match "Chapter 124", "Ch. 124", "Chapter 522.6", "capitulo 124"
    let re = regex_lite::Regex::new(r"(?i)(?:chapter|ch\.?|capitulo)\s*(\d+(?:\.\d+)?)").ok()?;
    if let Some(caps) = re.captures(s) {
        if let Some(m) = caps.get(1) {
            if let Ok(n) = m.as_str().parse::<f64>() {
                return Some(n);
            }
        }
    }
    // Fallback: hyphen or underscore before number at end of slug segment
    // e.g. "/chapter-124" → 124, "/chapter-522-6" → 522.6
    let re2 = regex_lite::Regex::new(r"[_-](\d+)-(\d+)$").ok()?;
    if let Some(caps) = re2.captures(s) {
        let whole = caps.get(1)?.as_str();
        let frac = caps.get(2)?.as_str();
        if let (Ok(w), Ok(f)) = (whole.parse::<f64>(), frac.parse::<f64>()) {
            return Some(w + f / 10.0_f64.powf(frac.len() as f64));
        }
    }
    // Last resort: standalone number at end of URL segment
    let re3 = regex_lite::Regex::new(r"[_-](\d+(?:\.\d+)?)$").ok()?;
    if let Some(caps) = re3.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().parse::<f64>().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_standard() {
        assert_eq!(extract_chapter_number("Chapter 124"), Some(124.0));
        assert_eq!(extract_chapter_number("Ch. 5"), Some(5.0));
        assert_eq!(extract_chapter_number("Chapter 522.6"), Some(522.6));
    }

    #[test]
    fn test_extract_from_url() {
        assert_eq!(extract_chapter_number("/manga/slug/chapter-124"), Some(124.0));
        assert_eq!(extract_chapter_number("https://site.com/manga/x/chapter-522-6"), Some(522.6));
    }

    #[test]
    fn test_make_absolute() {
        assert_eq!(make_absolute("https://example.com", "/path"), "https://example.com/path");
        assert_eq!(make_absolute("https://example.com", "https://other.com/x"), "https://other.com/x");
        assert_eq!(make_absolute("https://example.com", "relative"), "https://example.com/relative");
    }
}
