/// Utilities for cleaning and normalising manga/series titles across sources.

/// Collapse internal whitespace (newlines → space, multiple → single).
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut sp = false;
    for c in s.chars() {
        if c.is_whitespace() || c == '\n' || c == '\r' {
            if !sp { out.push(' '); sp = true; }
        } else { out.push(c); sp = false; }
    }
    out.trim().to_string()
}

/// Clean a title for display: strip "...", ";;" concatenations, trailing
/// source IDs, parenthetical suffixes, and excessive whitespace.
/// Preserves original capitalisation.
pub fn clean_display_title(title: &str) -> String {
    let t = collapse_whitespace(title);
    // Strip everything after "..." (truncated listing titles)
    let t = t.find("...").map(|p| t[..p].trim()).unwrap_or(&t);
    // Strip alternate titles separated by ;;
    let t = t.find(";;").map(|p| t[..p].trim()).unwrap_or(t);
    let t = t.trim();
    // Strip trailing numeric-only segments (source IDs on listing pages)
    let t = t.trim_end_matches(|c: char| c.is_ascii_digit());
    let t = t.trim();
    // Strip parenthetical suffixes at end
    let t = if let Some(pos) = t.rfind('(') {
        let before = t[..pos].trim_end();
        let after = t[pos..].trim();
        if after.ends_with(')') && !after[1..after.len()-1].contains('(') {
            before
        } else { t }
    } else { t };
    let t = t.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
    t.to_string()
}

/// Normalize a title for matching across sources (aggressive: lowercase + strip).
pub fn normalize_title(title: &str) -> String {
    clean_display_title(title).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_basic() {
        assert_eq!(clean_display_title("Martial Peak"), "Martial Peak");
    }

    #[test]
    fn test_clean_truncated() {
        assert_eq!(clean_display_title("Martial Peak...\n36850575"), "Martial Peak");
    }

    #[test]
    fn test_clean_alt_titles() {
        assert_eq!(clean_display_title("Nano Machine;;나노마신"), "Nano Machine");
    }

    #[test]
    fn test_normalize() {
        assert_eq!(normalize_title("Martial Peak"), "martial peak");
        assert_eq!(normalize_title("Martial Peak;;36850575"), "martial peak");
    }
}
