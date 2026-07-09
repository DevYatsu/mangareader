use thirtyfour::WebDriver;
use tracing;

/// Wait a fixed 2-second pause for dynamic page content to finish rendering.
///
/// All scraped sites use JavaScript to populate their DOM after navigation; a simple
/// sleep is coarse but sufficient for current adapters.
///
/// # Arguments
/// * `_driver` — WebDriver handle (unused, but reserved for future smarter waits).
/// * `label` — Human-readable label logged for diagnostics (e.g. `"demonicscans index"`).
pub async fn wait_for_page(_driver: &WebDriver, label: &str) {
    tracing::info!("{label}: page loaded, waiting 2s for dynamic content...");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
}

/// Resolve a possibly-relative URL against a given base origin.
///
/// Handles three cases:
/// - Already-absolute URLs (`http://...` / `https://...`) are returned as-is.
/// - Protocol-relative URLs (`//host/path`) get `https:` prepended.
/// - Absolute paths (`/path`) are joined to the base origin.
/// - Relative paths (`path`) are joined with a `/` separator.
///
/// # Arguments
/// * `base_origin` — Scheme + host (e.g. `"https://example.com"`).
/// * `href` — The raw href value to resolve.
///
/// # Returns
/// A fully-qualified URL string.
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

/// Extract a chapter number from text or a URL segment.
///
/// Supports patterns such as `"Chapter 124"`, `"Ch. 5"`, `"Chapter 522.6"`,
/// `"/chapter-124"`, and `"/chapter-522-6"` (decoded as `522.6`).
///
/// # Arguments
/// * `s` — The text or URL to scan.
///
/// # Returns
/// The extracted number, or [`None`] if no chapter-like pattern is found.
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
