//! Math (`$...$` / `$$...$$`) detection.
//!
//! This runs on *parsed* markdown text events, never on the raw source, so
//! code spans and code blocks are structurally out of reach: those arrive as
//! [`pulldown_cmark::Event::Code`] and [`pulldown_cmark::Tag::CodeBlock`],
//! never as `Event::Text`.

/// One piece of a text node after math splitting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathPiece {
    Text(String),
    /// `$x$`
    Inline(String),
    /// `$$x$$`
    Display(String),
}

impl MathPiece {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MathPiece::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// Split a markdown text node into plain text and math spans.
///
/// Deliberately conservative: a candidate is only accepted when it is
/// unambiguous, so prose that merely contains a dollar sign (`$5`, `$HOME`,
/// `costs $3 and $4`) is left untouched.
pub fn split_math(src: &str) -> Vec<MathPiece> {
    let chars: Vec<char> = src.chars().collect();
    let mut out: Vec<MathPiece> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            // Display math: $$ ... $$
            if chars.get(i + 1) == Some(&'$') && can_open_display(&chars, i) {
                if let Some((tex, next)) = find_close(&chars, i + 2, true) {
                    if is_wellformed(&tex, true) {
                        flush(&mut out, &mut buf);
                        out.push(MathPiece::Display(tex));
                        i = next;
                        continue;
                    }
                }
            }
            // Inline math: $ ... $
            if can_open(&chars, i) {
                if let Some((tex, next)) = find_close(&chars, i + 1, false) {
                    if is_wellformed(&tex, false) {
                        flush(&mut out, &mut buf);
                        out.push(MathPiece::Inline(tex));
                        i = next;
                        continue;
                    }
                }
            }
        }
        buf.push(chars[i]);
        i += 1;
    }

    flush(&mut out, &mut buf);
    out
}

fn flush(out: &mut Vec<MathPiece>, buf: &mut String) {
    if !buf.is_empty() {
        out.push(MathPiece::Text(std::mem::take(buf)));
    }
}

/// A `$` only opens math when it is not glued to a preceding word, so `USD$`
/// and `a$b` stay literal. A second `$` never opens inline math (`$$` is
/// handled separately as display math).
fn can_open(chars: &[char], i: usize) -> bool {
    if chars.get(i + 1) == Some(&'$') {
        return false;
    }
    match i.checked_sub(1).map(|p| chars[p]) {
        None => true,
        Some(prev) => !(prev.is_alphanumeric() || prev == '$' || prev == '_'),
    }
}

/// Same word-boundary rule as [`can_open`], for the `$$` opener.
fn can_open_display(chars: &[char], i: usize) -> bool {
    match i.checked_sub(1).map(|p| chars[p]) {
        None => true,
        Some(prev) => !(prev.is_alphanumeric() || prev == '$' || prev == '_'),
    }
}

/// Find the closing delimiter, returning the inner text and the index just past
/// the closer.
fn find_close(chars: &[char], from: usize, display: bool) -> Option<(String, usize)> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == '$' {
            if display {
                if chars.get(i + 1) == Some(&'$') {
                    let tex: String = chars[from..i].iter().collect();
                    return Some((tex, i + 2));
                }
            } else {
                let tex: String = chars[from..i].iter().collect();
                return Some((tex, i + 1));
            }
        }
        i += 1;
    }
    None
}

fn is_wellformed(tex: &str, display: bool) -> bool {
    let trimmed = tex.trim();
    if trimmed.is_empty() || trimmed.contains('$') {
        return false;
    }
    if !display {
        // Reject `$5 and ` — an unclosed-looking run almost always means the
        // author meant currency, not math.
        if tex.starts_with(char::is_whitespace) || tex.ends_with(char::is_whitespace) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(src: &str) -> Vec<MathPiece> {
        split_math(src)
    }

    fn text_of(src: &str) -> String {
        pieces(src)
            .into_iter()
            .filter_map(|p| match p {
                MathPiece::Text(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn plain_text_is_untouched() {
        assert_eq!(
            pieces("hello world"),
            vec![MathPiece::Text("hello world".into())]
        );
    }

    #[test]
    fn inline_math_is_extracted() {
        assert_eq!(
            pieces("see $E = mc^2$ ok"),
            vec![
                MathPiece::Text("see ".into()),
                MathPiece::Inline("E = mc^2".into()),
                MathPiece::Text(" ok".into()),
            ]
        );
    }

    #[test]
    fn display_math_is_extracted() {
        assert_eq!(pieces("$$x^2$$"), vec![MathPiece::Display("x^2".into())]);
    }

    #[test]
    fn display_math_may_span_lines() {
        let got = pieces("$$\n\\frac{a}{b}\n$$");
        assert_eq!(got, vec![MathPiece::Display("\n\\frac{a}{b}\n".into())]);
    }

    #[test]
    fn currency_is_not_math() {
        // The most important negative case: prose with prices must survive.
        assert_eq!(text_of("costs $5 and $10 total"), "costs $5 and $10 total");
        assert_eq!(text_of("price: $30"), "price: $30");
    }

    #[test]
    fn dollar_glued_to_word_is_not_math() {
        assert_eq!(text_of("USD$100"), "USD$100");
    }

    #[test]
    fn empty_math_is_not_math() {
        assert_eq!(text_of("a $ b $ c"), "a $ b $ c");
    }

    #[test]
    fn mixed_content_keeps_order() {
        let got = pieces("$a$ mid $$b$$ end");
        assert_eq!(
            got,
            vec![
                MathPiece::Inline("a".into()),
                MathPiece::Text(" mid ".into()),
                MathPiece::Display("b".into()),
                MathPiece::Text(" end".into()),
            ]
        );
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(pieces("").is_empty());
    }
}
