//! URL scheme validation for links and images.
//!
//! `pulldown-cmark` happily emits `<a href="javascript:...">` from a markdown
//! document, so the destination must be checked before it reaches the preview.

/// Schemes that are safe to put in an `href`/`src` we render into the app.
const SAFE_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];

/// Returns `true` when `dest` is safe to emit as a link/image destination.
///
/// Accepts absolute URLs with a known-safe scheme plus relative references
/// (`notes/a.md`, `./b.md`, `#anchor`, `/abs/path`).
pub fn is_safe_url(dest: &str) -> bool {
    let d = dest.trim();
    if d.is_empty() {
        return false;
    }
    // Fragment-only and protocol-relative references.
    if d.starts_with('#') || d.starts_with("//") {
        return true;
    }

    let Some(colon) = d.find(':') else {
        // No colon at all: a relative path.
        return true;
    };

    let head = &d[..colon];
    if head.is_empty() {
        return false;
    }

    // A colon appearing after a path separator is not a scheme delimiter
    // (`notes/a:b.md` is a relative path, not the `notes` scheme).
    if head.contains('/') || head.contains('?') || head.contains('#') {
        return true;
    }

    // A valid scheme is ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ).
    let mut chars = head.chars();
    let valid_syntax = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));

    valid_syntax && SAFE_SCHEMES.contains(&head.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_http_and_https() {
        assert!(is_safe_url("https://example.com"));
        assert!(is_safe_url("http://example.com/a?b=c#d"));
        assert!(is_safe_url("HTTPS://EXAMPLE.COM"));
    }

    #[test]
    fn allows_mailto_and_tel() {
        assert!(is_safe_url("mailto:a@b.com"));
        assert!(is_safe_url("tel:+123456"));
    }

    #[test]
    fn allows_relative_references() {
        assert!(is_safe_url("notes/a.md"));
        assert!(is_safe_url("./b.md"));
        assert!(is_safe_url("../c.md"));
        assert!(is_safe_url("/abs/path.md"));
        assert!(is_safe_url("#anchor"));
        assert!(is_safe_url("//cdn.example.com/x.png"));
    }

    #[test]
    fn rejects_javascript_and_data() {
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url("JavaScript:alert(1)"));
        assert!(!is_safe_url("  javascript:alert(1)"));
        assert!(!is_safe_url("data:text/html;base64,PHNjcmlwdD4="));
        assert!(!is_safe_url("vbscript:msgbox"));
        assert!(!is_safe_url("file:///etc/passwd"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_safe_url(""));
        assert!(!is_safe_url("   "));
    }

    #[test]
    fn colon_after_separator_is_a_path() {
        assert!(is_safe_url("notes/a:b.md"));
    }

    #[test]
    fn rejects_malformed_scheme_syntax() {
        // Scheme-looking prefix that is not a valid scheme must not sneak past.
        assert!(!is_safe_url("1abc:payload"));
    }
}
