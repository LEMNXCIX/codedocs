//! Markdown → HTML rendering that is safe to inject into the app webview.
//!
//! # Threat model
//!
//! The preview renders *untrusted files the user opened*. Markdown permits raw
//! HTML, and `pulldown-cmark` passes it through verbatim, so rendering a
//! hostile document directly yields script execution inside a Tauri webview
//! that has `__TAURI__` on `window`.
//!
//! # Strategy
//!
//! Rather than sanitising HTML *after* the fact (fragile: requires an HTML
//! parser to be correct), this module builds the output from an allowlist by
//! construction:
//!
//! 1. Every `Event::Html` / `Event::InlineHtml` produced by the *document* is
//!    dropped. Raw HTML from the file never reaches the output.
//! 2. Every `Event::Html` in the output is one this module generated itself
//!    (math and mermaid wrappers).
//! 3. Link and image destinations are scheme-checked; unsafe ones degrade to
//!    their text content.
//!
//! Document text always travels as `Event::Text`, which `push_html` escapes.

mod math;
mod sanitize;

use std::collections::HashMap;

use pulldown_cmark::{html, CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};

pub use math::{split_math, MathPiece};
pub use sanitize::is_safe_url;

/// A heading extracted from a document, used to build the outline panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    /// The `id` the renderer emits for this heading.
    ///
    /// Carried here rather than recomputed by the outline: the renderer
    /// deduplicates repeated slugs and falls back for headings with no
    /// slug-worthy characters, so a caller that re-derives the anchor from
    /// `text` alone produces ids that do not exist and silently fails to
    /// scroll.
    pub anchor: String,
}

fn parser_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options
}

/// Render markdown to HTML that is safe to inject via `inner_html`.
pub fn render_markdown(content: &str) -> String {
    let parser = Parser::new_ext(content, parser_options());
    let mut out = String::new();
    html::push_html(&mut out, transform(parser).into_iter());
    out
}

/// Extract the heading outline of a document.
///
/// Text is accumulated across the whole heading (inline emphasis, code spans
/// and links included) rather than stopping at the first text node, so
/// `## Hello **world**` yields `Hello world`.
pub fn extract_headings(content: &str) -> Vec<Heading> {
    let parser = Parser::new_ext(content, parser_options());
    let mut headings: Vec<Heading> = Vec::new();
    // Same allocator the renderer uses, in the same document order, so the
    // anchors returned here match the ids it emits exactly.
    let mut used_slugs: HashMap<String, usize> = HashMap::new();
    let mut open: Option<(u8, String)> = None;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                open = Some((level as u8, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = open.take() {
                    let text = text.trim();
                    if !text.is_empty() {
                        let anchor = unique_slug(text, &mut used_slugs);
                        headings.push(Heading {
                            level,
                            text: text.to_string(),
                            anchor,
                        });
                    }
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, ref mut acc)) = open {
                    // Separate nodes with a single space only when neither side
                    // already provides whitespace, so `**world**` inside a
                    // heading does not gain a double space.
                    if !acc.is_empty()
                        && !acc.ends_with(char::is_whitespace)
                        && !t.starts_with(char::is_whitespace)
                    {
                        acc.push(' ');
                    }
                    acc.push_str(&t);
                }
            }
            _ => {}
        }
    }

    headings
}

/// What kind of fenced block we are currently inside.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockMode {
    /// Not inside a code block.
    Flow,
    /// A normal code block: pass through untouched, never split math.
    Code,
    /// A ```mermaid block: emit a `<pre>` the JS bridge can pick up.
    Mermaid,
    /// A ```math / ```latex block: emit a display-math span.
    Math,
}

fn fenced_lang(kind: &CodeBlockKind) -> Option<String> {
    match kind {
        CodeBlockKind::Fenced(info) => {
            let first = info.split_whitespace().next().unwrap_or("");
            if first.is_empty() {
                None
            } else {
                Some(first.to_ascii_lowercase())
            }
        }
        CodeBlockKind::Indented => None,
    }
}

/// Rewrite the event stream into a safe one. See the module docs.
fn transform<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::new();
    let mut mode = BlockMode::Flow;
    // Link/image wrappers we dropped because of an unsafe URL. Their children
    // are kept, so the reader still sees the text.
    //
    // A *stack*, not a counter: `![a [b](https://ok)](javascript:x)` suppresses
    // the outer image while keeping the inner link. With a single counter the
    // inner `</a>` would be swallowed as if it closed the suppressed wrapper,
    // leaving a dangling `<a>` in the output. Tracking which tag each entry
    // opened keeps the emitted tags balanced.
    let mut suppressed: Vec<TagEnd> = Vec::new();

    // Heading state. A heading needs an `id` so the outline panel can scroll to
    // it, but `pulldown-cmark` does not emit one, and the id derives from the
    // heading's text — which is only known once the heading closes. So heading
    // content is buffered and re-emitted as a flat run of text. Inline markup
    // inside a heading (`## **bold**`) collapses to plain text, which is the
    // right trade for a label shown in a navigation list.
    let mut heading: Option<HeadingBuffer> = None;
    let mut used_slugs: HashMap<String, usize> = HashMap::new();

    for event in events {
        // While buffering a heading, every event except its terminator only
        // contributes text.
        if let Some(buffer) = heading.as_mut() {
            match &event {
                Event::End(TagEnd::Heading(_)) => {
                    let buffer = heading.take().expect("checked above");
                    emit_heading(&mut out, &buffer, &mut used_slugs);
                }
                Event::Text(t) | Event::Code(t) => buffer.push_text(t),
                // Inline math inside a heading: keep the TeX, drop the wrapper.
                Event::Html(_) | Event::InlineHtml(_) => {}
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(HeadingBuffer {
                    level: level as u8,
                    text: String::new(),
                });
            }

            // ---- Raw HTML from the document: never trusted. ----
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::Start(Tag::HtmlBlock) | Event::End(TagEnd::HtmlBlock) => {}

            // ---- Fenced code blocks. ----
            Event::Start(Tag::CodeBlock(kind)) => {
                mode = match fenced_lang(&kind).as_deref() {
                    Some("mermaid") => {
                        out.push(Event::Html(CowStr::from(r#"<pre class="mermaid-block">"#)));
                        BlockMode::Mermaid
                    }
                    Some("math" | "latex" | "katex") => {
                        out.push(Event::Html(CowStr::from(r#"<span class="math-display">"#)));
                        BlockMode::Math
                    }
                    _ => {
                        out.push(Event::Start(Tag::CodeBlock(kind)));
                        BlockMode::Code
                    }
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                match mode {
                    BlockMode::Mermaid => out.push(Event::Html(CowStr::from("</pre>"))),
                    BlockMode::Math => out.push(Event::Html(CowStr::from("</span>"))),
                    BlockMode::Code => out.push(Event::End(TagEnd::CodeBlock)),
                    BlockMode::Flow => {}
                }
                mode = BlockMode::Flow;
            }

            // ---- Links and images: scheme-check the destination. ----
            Event::Start(tag) => {
                let unsafe_dest = match &tag {
                    Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                        !is_safe_url(dest_url)
                    }
                    _ => false,
                };
                if unsafe_dest {
                    // Keep the children (the visible label) but drop the
                    // wrapper, so no text is lost and no URL is emitted. Record
                    // which tag was opened so its matching end can be skipped.
                    suppressed.push(match &tag {
                        Tag::Image { .. } => TagEnd::Image,
                        _ => TagEnd::Link,
                    });
                } else {
                    out.push(Event::Start(tag));
                }
            }
            Event::End(tag_end @ (TagEnd::Link | TagEnd::Image)) => {
                // Only swallow the end when it closes a wrapper we actually
                // suppressed; otherwise emit it, so nesting stays balanced.
                if suppressed.last() == Some(&tag_end) {
                    suppressed.pop();
                } else {
                    out.push(Event::End(tag_end));
                }
            }

            // ---- Text: expand math only in flowing prose. ----
            Event::Text(text) => match mode {
                BlockMode::Flow => emit_math(&mut out, &text),
                // Code, mermaid and math blocks keep their text verbatim; the
                // JS bridges read it back out of `textContent`.
                _ => out.push(Event::Text(text)),
            },

            other => out.push(other),
        }
    }

    out
}

/// Accumulates a heading's text while it is being parsed.
struct HeadingBuffer {
    level: u8,
    text: String,
}

impl HeadingBuffer {
    fn push_text(&mut self, text: &str) {
        // Separate nodes with a single space only when neither side already
        // provides whitespace, so `**world**` inside a heading does not gain a
        // double space.
        if !self.text.is_empty()
            && !self.text.ends_with(char::is_whitespace)
            && !text.starts_with(char::is_whitespace)
        {
            self.text.push(' ');
        }
        self.text.push_str(text);
    }
}

/// Emit `<hN id="slug">text</hN>`.
///
/// The tag and id are generated here rather than taken from the document; the
/// text goes through `Event::Text`, which `push_html` escapes.
fn emit_heading<'a>(
    out: &mut Vec<Event<'a>>,
    buffer: &HeadingBuffer,
    used_slugs: &mut HashMap<String, usize>,
) {
    let text = buffer.text.trim();
    if text.is_empty() {
        return;
    }
    let level = buffer.level.clamp(1, 6);
    let slug = unique_slug(text, used_slugs);

    out.push(Event::Html(CowStr::from(format!(
        r#"<h{level} id="{slug}">"#
    ))));
    out.push(Event::Text(CowStr::from(text.to_string())));
    out.push(Event::Html(CowStr::from(format!("</h{level}>"))));
}

/// GitHub-compatible slug, deduplicated within the document.
///
/// Two headings with the same text must get different ids or the outline would
/// scroll both entries to the first one.
fn unique_slug(text: &str, used: &mut HashMap<String, usize>) -> String {
    let base = slugify(text);
    let base = if base.is_empty() {
        "seccion".to_string()
    } else {
        base
    };
    match used.get_mut(&base) {
        Some(count) => {
            *count += 1;
            format!("{base}-{count}")
        }
        None => {
            used.insert(base.clone(), 0);
            base
        }
    }
}

/// Lowercase, strip punctuation, spaces become dashes.
pub fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

fn emit_math(out: &mut Vec<Event<'_>>, text: &str) {
    for piece in split_math(text) {
        match piece {
            MathPiece::Text(t) => out.push(Event::Text(CowStr::from(t))),
            MathPiece::Inline(tex) => {
                out.push(Event::Html(CowStr::from(r#"<span class="math-inline">"#)));
                out.push(Event::Text(CowStr::from(tex)));
                out.push(Event::Html(CowStr::from("</span>")));
            }
            MathPiece::Display(tex) => {
                out.push(Event::Html(CowStr::from(r#"<span class="math-display">"#)));
                out.push(Event::Text(CowStr::from(tex)));
                out.push(Event::Html(CowStr::from("</span>")));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Attribute names of a single open tag.
    ///
    /// `tag` is the tag body without the angle brackets, e.g.
    /// `a href="…" title="…"`. Splits on whitespace *outside* quotes and takes
    /// the part before each `=`. A naive split would read `onclick=…` inside a
    /// quoted `title` value as a real attribute, which is precisely the
    /// confusion these tests exist to rule out.
    fn attribute_names(tag: &str) -> Vec<String> {
        let mut tokens: Vec<String> = Vec::new();
        let mut token = String::new();
        let mut quote: Option<char> = None;

        for c in tag.chars() {
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => token.push(c),
                None if c == '"' || c == '\'' => quote = Some(c),
                None if c == '>' => break,
                None if c.is_whitespace() => {
                    if !token.is_empty() {
                        tokens.push(std::mem::take(&mut token));
                    }
                }
                None => token.push(c),
            }
        }

        // The first token is the element name, not an attribute.
        tokens
            .into_iter()
            .skip(1)
            .filter_map(|t| t.split('=').next().map(str::to_string))
            .filter(|n| !n.is_empty())
            .collect()
    }

    #[test]
    fn plain_markdown_renders() {
        let html = render_markdown("# Title\n\nSome *text*.");
        assert!(html.contains(r#"<h1 id="title">Title</h1>"#), "{html}");
        assert!(html.contains("<em>text</em>"), "{html}");
    }

    #[test]
    fn gfm_features_work() {
        let html = render_markdown("| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n\n~~gone~~");
        assert!(html.contains("<table>"));
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("<del>gone</del>"));
    }

    // ---- Security ----

    #[test]
    fn raw_script_tags_are_stripped() {
        let html = render_markdown("<script>alert(1)</script>");
        assert!(!html.contains("<script"), "script tag leaked: {html}");
        assert!(!html.contains("alert(1)"), "script body leaked: {html}");
    }

    #[test]
    fn inline_event_handlers_are_stripped() {
        let html = render_markdown("<img src=x onerror=\"alert(1)\">");
        assert!(!html.contains("onerror"), "event handler leaked: {html}");
        assert!(!html.contains("<img"), "raw img leaked: {html}");
    }

    #[test]
    fn html_blocks_are_stripped() {
        let html = render_markdown("<div onclick=\"steal()\">hi</div>");
        assert!(!html.contains("onclick"), "event handler leaked: {html}");
        assert!(!html.contains("steal()"), "handler body leaked: {html}");
    }

    #[test]
    fn javascript_links_are_neutralised() {
        let html = render_markdown("[click me](javascript:alert(1))");
        assert!(!html.contains("javascript:"), "js url leaked: {html}");
        // The label is preserved so no content is silently lost.
        assert!(html.contains("click me"), "link text was dropped: {html}");
    }

    #[test]
    fn javascript_images_are_neutralised() {
        let html = render_markdown("![alt](javascript:alert(1))");
        assert!(!html.contains("javascript:"), "js url leaked: {html}");
    }

    #[test]
    fn data_urls_are_neutralised() {
        let html = render_markdown("[x](data:text/html;base64,PHNjcmlwdD4=)");
        assert!(!html.contains("data:text/html"), "data url leaked: {html}");
    }

    #[test]
    fn safe_links_survive() {
        let html = render_markdown("[docs](https://example.com/a)");
        assert!(html.contains("href=\"https://example.com/a\""), "{html}");
    }

    #[test]
    fn relative_links_survive() {
        let html = render_markdown("[other](./notes/other.md)");
        assert!(html.contains("href=\"./notes/other.md\""), "{html}");
    }

    // ---- Math ----

    #[test]
    fn inline_math_gets_a_span() {
        let html = render_markdown("value $x^2$ here");
        assert!(html.contains(r#"<span class="math-inline">"#), "{html}");
        assert!(html.contains("x^2"), "{html}");
    }

    #[test]
    fn display_math_gets_a_span() {
        let html = render_markdown("$$a+b$$");
        assert!(html.contains(r#"<span class="math-display">"#), "{html}");
    }

    #[test]
    fn math_wrappers_cannot_be_escaped() {
        // The wrappers this module emits must never let document text close
        // them and open a tag of its own.
        let html = render_markdown("$</span><img src=x onerror=alert(1)>$\n");
        assert!(
            !html.contains("<img"),
            "img escaped the math wrapper: {html}"
        );
        let html = render_markdown("$$</span><script>alert(1)</script>$$\n");
        assert!(
            !html.contains("<script"),
            "script escaped the math wrapper: {html}"
        );
    }

    #[test]
    fn mermaid_source_cannot_close_its_own_block() {
        let html = render_markdown(
            "```mermaid\ngraph TD;\nA[\"</pre><img src=x onerror=alert(1)>\"];\n```\n",
        );
        assert!(
            !html.contains("<img"),
            "img escaped the mermaid block: {html}"
        );
        assert!(
            !html.contains("</pre><"),
            "the block was closed early: {html}"
        );
    }

    #[test]
    fn fenced_info_string_does_not_reach_the_attributes() {
        // A fence label is document text; it must not influence the emitted tag.
        let html = render_markdown("```math {#x\" onload=\"alert(1)}\n\\frac{a}{b}\n```\n");
        assert!(
            !html.contains("onload"),
            "info string leaked into an attribute: {html}"
        );
        let html = render_markdown("```mermaid {#x\" onload=\"alert(1)}\ngraph TD;\nA-->B;\n```\n");
        assert!(
            !html.contains("onload"),
            "info string leaked into an attribute: {html}"
        );
    }

    #[test]
    fn link_and_image_titles_are_escaped() {
        // The attacker's text survives as the *value* of `title`, which is safe:
        // it is inside a quoted attribute and its own quote is entity-encoded.
        // What must never happen is it becoming an attribute of its own.
        for (md, url_attr) in [
            ("[x](https://ok \"a\\\" onclick=alert(1)\")\n", "href"),
            ("![x](https://ok/a.png \"t\\\" onerror=alert(1)\")\n", "src"),
        ] {
            let html = render_markdown(md);
            let tag = html
                .split('<')
                .find(|part| part.starts_with("a ") || part.starts_with("img "))
                .unwrap_or_else(|| panic!("no link/image tag in {html}"))
                .split('>')
                .next()
                .unwrap_or_default();

            let names = attribute_names(tag);
            assert!(
                names
                    .iter()
                    .all(|n| ["href", "src", "title", "alt"].contains(&n.as_str())),
                "unexpected attribute in <{tag}> for {md:?}: {names:?}"
            );
            assert!(
                names.iter().any(|n| n == url_attr),
                "missing {url_attr} in <{tag}>"
            );
        }
    }

    #[test]
    fn heading_text_cannot_add_attributes() {
        // Smart punctuation turns the document's quotes into curly ones, so the
        // hostile text becomes visible content. The invariant is that the
        // heading tag carries only the generated id.
        for md in [
            "# a\" onload=\"alert(1)\n",
            "# Hi {#\"onmouseover=alert(1)}\n",
        ] {
            let html = render_markdown(md);
            let tag = html
                .split('<')
                .find(|part| part.starts_with("h1"))
                .unwrap_or_else(|| panic!("no heading tag in {html}"))
                .split('>')
                .next()
                .unwrap_or_default();
            assert_eq!(
                tag.matches('"').count(),
                2,
                "extra attribute in <{tag}> for {md:?}"
            );
            let id = tag
                .split("id=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .unwrap_or_default();
            assert!(
                id.chars().all(|c| c.is_alphanumeric() || c == '-'),
                "unsafe id {id:?} for {md:?}"
            );
        }
    }

    #[test]
    fn heading_that_is_only_html_emits_nothing() {
        // `# <b>` leaves no text once the raw HTML is stripped, so no tag is
        // emitted at all rather than an empty heading with a stray attribute.
        let html = render_markdown("# <b>\n");
        assert!(
            !html.contains("<h1"),
            "empty heading should be dropped: {html}"
        );
        assert!(!html.contains("<b>"), "raw HTML leaked: {html}");
    }

    #[test]
    fn reference_links_are_scheme_checked_too() {
        let html = render_markdown("[x][r]\n\n[r]: javascript:alert(1)\n");
        assert!(
            !html.contains("javascript:"),
            "unsafe ref link survived: {html}"
        );
        let html = render_markdown("[x][r]\n\n[r]: https://ok.example/\n");
        assert!(
            html.contains("https://ok.example/"),
            "safe ref link dropped: {html}"
        );
    }

    #[test]
    fn raw_html_in_every_container_is_stripped() {
        // The block parser emits raw HTML in tables, lists, blockquotes, task
        // items and footnote bodies; all of them must go too.
        for md in [
            "| a | b |\n|---|---|\n| <script>alert(1)</script> | <img src=x onerror=y> |\n",
            "- [ ] <img src=x onerror=alert(1)>\n",
            "> <script>alert(1)</script>\n",
            "text[^1]\n\n[^1]: <img src=x onerror=alert(1)>\n",
            "*<img src=x onerror=alert(1)>*\n",
            "<details open ontoggle=alert(1)><summary>x</summary>y</details>\n",
            "<form action=javascript:alert(1)><input></form>\n",
            "<style>body{background:url(javascript:alert(1))}</style>\n",
            "<svg><animate onbegin=alert(1) attributeName=x dur=1s>",
            "<img src=\"data:image/svg+xml;base64,PHN2Zz4=\">\n",
        ] {
            let html = render_markdown(md);
            for marker in [
                "<script",
                "<img",
                "<details",
                "<form",
                "<style",
                "<svg",
                "onerror",
                "ontoggle",
                "onbegin",
                "javascript:",
            ] {
                assert!(
                    !html.contains(marker),
                    "{marker} survived in output for {md:?}\n  -> {html}"
                );
            }
        }
    }

    #[test]
    fn entity_and_whitespace_url_obfuscation_is_rejected() {
        // Browsers decode entities and ignore control characters inside URLs
        // before resolving the scheme, so the check must too.
        for md in [
            "[x](java&#115;cript:alert(1))",
            "[x](java\tscript:alert(1))",
            "[x](JaVaScRiPt:alert(1))",
            "[x](   javascript:alert(1))",
        ] {
            let html = render_markdown(md);
            assert!(
                !html.to_lowercase().contains("javascript:"),
                "leaked for {md:?}: {html}"
            );
        }
    }

    #[test]
    fn autolinks_are_scheme_checked() {
        let html = render_markdown("<https://ok.example>\n");
        assert!(
            html.contains("https://ok.example"),
            "safe autolink dropped: {html}"
        );

        // pulldown-cmark does not recognise `javascript:` as an autolink scheme,
        // so this stays inert text rather than becoming a link. The invariant
        // under test is that no anchor is produced at all.
        let html = render_markdown("<javascript:alert(1)>\n");
        assert!(
            !html.contains("<a "),
            "unsafe autolink became a link: {html}"
        );
        assert!(
            !html.contains("<a>"),
            "unsafe autolink became a link: {html}"
        );
    }

    #[test]
    fn math_inside_inline_code_is_literal() {
        let html = render_markdown("run `echo $HOME` now");
        assert!(
            !html.contains("math-inline"),
            "code span was treated as math: {html}"
        );
        assert!(html.contains("$HOME"), "{html}");
    }

    #[test]
    fn math_inside_fenced_code_is_literal() {
        let html = render_markdown("```sh\necho $HOME\n```");
        assert!(
            !html.contains("math-inline"),
            "code block was treated as math: {html}"
        );
        assert!(html.contains("$HOME"), "{html}");
    }

    #[test]
    fn fenced_math_block_is_supported() {
        let html = render_markdown("```math\n\\frac{a}{b}\n```");
        assert!(html.contains(r#"<span class="math-display">"#), "{html}");
    }

    #[test]
    fn currency_survives_rendering() {
        let html = render_markdown("It costs $5 and $10.");
        assert!(
            !html.contains("math-inline"),
            "currency parsed as math: {html}"
        );
        assert!(html.contains("$5 and $10"), "{html}");
    }

    // ---- Mermaid ----

    #[test]
    fn mermaid_block_carries_source_as_text_not_attribute() {
        let html = render_markdown("```mermaid\ngraph TD;\nA-->B;\n```");
        assert!(html.contains(r#"<pre class="mermaid-block">"#), "{html}");
        assert!(html.contains("</pre>"), "{html}");
        // Source must be text content so the JS bridge can read it back without
        // entity decoding games.
        assert!(
            !html.contains("data-mermaid"),
            "attribute form reintroduced: {html}"
        );
        assert!(html.contains("graph TD;"), "{html}");
    }

    #[test]
    fn mermaid_source_is_escaped() {
        let html = render_markdown("```mermaid\ngraph TD;\nA[\"<img onerror=x>\"];\n```");
        assert!(
            !html.contains("<img"),
            "raw html inside mermaid leaked: {html}"
        );
        assert!(html.contains("&lt;img"), "{html}");
    }

    // ---- Headings ----

    #[test]
    fn extract_headings_basic() {
        let hs = extract_headings("# One\n\ntext\n\n## Two\n");
        assert_eq!(
            hs,
            vec![
                Heading {
                    level: 1,
                    text: "One".into(),
                    anchor: "one".into()
                },
                Heading {
                    level: 2,
                    text: "Two".into(),
                    anchor: "two".into()
                },
            ]
        );
    }

    #[test]
    fn extract_headings_includes_emphasis_and_code() {
        let hs = extract_headings("## Hello **world** and `code`\n");
        assert_eq!(hs.len(), 1);
        assert_eq!(hs[0].text, "Hello world and code");
    }

    #[test]
    fn extract_headings_skips_headings_inside_code_fences() {
        let md = "```\n# not a heading\n```\n\n# real\n";
        let hs = extract_headings(md);
        assert_eq!(
            hs,
            vec![Heading {
                level: 1,
                text: "real".into(),
                anchor: "real".into()
            }]
        );
    }

    #[test]
    fn extract_headings_on_empty_document() {
        assert!(extract_headings("").is_empty());
    }

    #[test]
    fn setext_headings_are_found() {
        let hs = extract_headings("Title\n=====\n");
        assert_eq!(
            hs,
            vec![Heading {
                level: 1,
                text: "Title".into(),
                anchor: "title".into()
            }]
        );
    }

    // ---- Heading anchors (used by the outline panel) ----

    #[test]
    fn headings_get_slug_ids() {
        let html = render_markdown("# Hello World\n");
        assert!(html.contains(r#"<h1 id="hello-world">"#), "{html}");
    }

    #[test]
    fn duplicate_headings_get_unique_ids() {
        let html = render_markdown("# Notes\n\n# Notes\n\n# Notes\n");
        assert!(html.contains(r#"id="notes""#), "{html}");
        assert!(html.contains(r#"id="notes-1""#), "{html}");
        assert!(html.contains(r#"id="notes-2""#), "{html}");
    }

    #[test]
    fn heading_ids_are_attribute_safe() {
        // The id is generated from document text. A heading crafted to break out
        // of the attribute must not be able to inject one.
        let html = render_markdown("# a\" onload=\"alert(1)\n");
        let id = html
            .split("id=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("heading should carry an id");
        assert!(
            id.chars().all(|c| c.is_alphanumeric() || c == '-'),
            "id contains unsafe characters: {id:?} in {html}"
        );
        // Exactly one attribute on the tag: the closing `>` must come right
        // after the id.
        let tag = html.split('<').nth(1).unwrap_or_default();
        assert_eq!(
            tag.matches('"').count(),
            2,
            "extra attribute injected: {tag}"
        );
    }

    #[test]
    fn heading_text_is_escaped() {
        let html = render_markdown("# <img src=x onerror=y>\n");
        assert!(!html.contains("<img"), "{html}");
    }

    #[test]
    fn empty_heading_emits_no_tag() {
        let html = render_markdown("#\n");
        assert!(!html.contains("<h1"), "{html}");
    }

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("Hello World"), "hello-world");
        // Punctuation is dropped rather than turned into a separator, matching
        // GitHub's behaviour for apostrophes.
        assert_eq!(slugify("What's new?"), "whats-new");
        assert_eq!(slugify("a   b"), "a-b");
        assert_eq!(slugify("!!!"), "");
    }

    #[test]
    fn slugify_output_is_always_attribute_safe() {
        for input in [
            "a\" onload=\"x",
            "<script>",
            "a&b",
            "'; DROP TABLE--",
            "back\\slash",
        ] {
            let slug = slugify(input);
            assert!(
                slug.chars().all(|c| c.is_alphanumeric() || c == '-'),
                "unsafe slug {slug:?} from {input:?}"
            );
        }
    }

    #[test]
    fn heading_with_math_keeps_tex() {
        let html = render_markdown("# Value $x^2$ here\n");
        assert!(html.contains("x^2"), "{html}");
        assert!(
            !html.contains("math-inline"),
            "heading math should stay plain: {html}"
        );
    }
}
