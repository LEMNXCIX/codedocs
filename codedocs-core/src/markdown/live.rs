//! Live-format spans for the editing surface.
//!
//! The editor shows formatted markdown while the source stays the source: no
//! serialization, no rewriting, the document is never touched. To hide the
//! syntax characters (the `**`, `#`, `](url)` …) the frontend needs to know
//! *where* they are, as ranges it can decorate. That is what this module
//! computes: a flat list of `(from, to, tag)` spans over the document.
//!
//! # Coordinate system
//!
//! `pulldown-cmark` reports ranges in **bytes**. CodeMirror counts columns in
//! **UTF-16 code units**. A `ñ` is 2 bytes but 1 unit, an emoji is 4 bytes but
//! 2 units, so passing byte offsets through untouched silently shifts every
//! decoration after the first non-ASCII character. [`Utf16Index`] translates
//! between the two; getting this wrong is the easiest way to break the
//! editor, which is why the `offsets_are_utf16_not_bytes` test exists.
//!
//! # Marker rule
//!
//! For a construct like `**a**`, the parser reports the whole `0..5` on the
//! `Start(Strong)` event and `2..3` on the inner text. The *marker* — the part
//! to hide — is whatever the children do not cover: `0..2` and `3..5`. The
//! styled part is the children themselves. Block constructs work the same
//! way: in `> quote` the `Start(BlockQuote)` range covers `> ` plus the
//! paragraph, so the marker is the `> ` prefix. A construct with no children
//! (`#` alone, an unclosed `~~~`) has nowhere to put a marker and yields no
//! spans rather than a guess.
//!
//! # Overlaps
//!
//! Marker spans of nested constructs cannot overlap by construction (an outer
//! marker is a gap *between* its children, and inner spans live inside those
//! children). The dedup in [`live_spans`] is a backstop anyway: when two
//! `Hide` spans do overlap, the outer one wins and the inner is discarded, as
//! the spec requires. Spans with different tags (e.g. `Quote` over the same
//! `> ` as its `Hide`) are allowed to coincide.

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

use super::math::split_math;
use super::parser_options;

/// What a span of source text means to the editor.
///
/// Discriminants are explicit because they cross the IPC boundary to
/// JavaScript; renumbering silently recolors every decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SpanTag {
    Hide = 0,
    Strong = 1,
    Emphasis = 2,
    Strikethrough = 3,
    InlineCode = 4,
    LinkText = 5,
    ImageAlt = 6,
    Heading1 = 7,
    Heading2 = 8,
    Heading3 = 9,
    Heading4 = 10,
    Heading5 = 11,
    Heading6 = 12,
    Quote = 13,
    ListMarker = 14,
    Fence = 15,
    TablePipe = 16,
    TableDelimiter = 17,
    MathInline = 18,
    MathDisplay = 19,
    MermaidBlock = 20,
}

/// Flat span list over a document, in UTF-16 code units.
///
/// `ranges` holds `[from0, to0, from1, to1, …]` and `tags[i]` labels
/// `ranges[2*i]..ranges[2*i+1]`. Sorted by `from`, bounded by the document's
/// UTF-16 length, with no overlapping `Hide` pair.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct LiveSpans {
    pub ranges: Vec<u32>,
    pub tags: Vec<SpanTag>,
}

/// Inicios de línea en índices de byte y en unidades UTF-16, para traducir los
/// offsets de `pulldown-cmark` a los que cuenta CodeMirror.
pub(crate) struct Utf16Index<'a> {
    text: &'a str,
    /// Byte offset where each line starts; always begins with 0.
    line_byte: Vec<usize>,
    /// UTF-16 units counted at each line start.
    line_utf16: Vec<u32>,
}

impl<'a> Utf16Index<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let mut line_byte = vec![0];
        let mut line_utf16 = vec![0];
        let mut units: u32 = 0;
        for (i, ch) in text.char_indices() {
            units += ch.len_utf16() as u32;
            if ch == '\n' {
                line_byte.push(i + ch.len_utf8());
                line_utf16.push(units);
            }
        }
        Self {
            text,
            line_byte,
            line_utf16,
        }
    }

    /// Byte offset → UTF-16 units. `None` when the offset is outside the
    /// document or lands in the middle of a character.
    pub(crate) fn to_utf16(&self, byte_offset: usize) -> Option<u32> {
        if byte_offset > self.text.len() {
            return None;
        }
        // Binary search for the line holding the offset; the line-start tables
        // always contain 0, so the index below cannot underflow.
        let line = self
            .line_byte
            .partition_point(|&s| s <= byte_offset)
            .saturating_sub(1);
        let mut pos = self.line_byte[line];
        let mut units = self.line_utf16[line];
        for ch in self.text[pos..].chars() {
            if pos == byte_offset {
                return Some(units);
            }
            if pos > byte_offset {
                return None;
            }
            pos += ch.len_utf8();
            units += ch.len_utf16() as u32;
        }
        if pos == byte_offset {
            Some(units)
        } else {
            None
        }
    }
}

/// An open construct on the parser stack, accumulating its children's ranges
/// so the marker gaps can be derived when it closes.
struct Frame<'a> {
    tag: Tag<'a>,
    start: usize,
    end: usize,
    children: Vec<(usize, usize)>,
}

/// Byte-range spans before UTF-16 conversion: `(from, to, tag)`.
type ByteSpans = Vec<(usize, usize, SpanTag)>;

/// Compute the live-format spans of `content`.
///
/// Never panics on degenerate input (`**`, `#`, `~~~`, `$`, …): constructs
/// without children yield no spans, out-of-range offsets are skipped, and
/// every emitted span satisfies `from <= to`.
pub fn live_spans(content: &str) -> LiveSpans {
    let index = Utf16Index::new(content);
    let mut spans: ByteSpans = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();

    for (event, range) in Parser::new_ext(content, parser_options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Some(parent) = stack.last_mut() {
                    push_child(parent, range.start, range.end);
                }
                stack.push(Frame {
                    tag,
                    start: range.start,
                    end: range.end,
                    children: Vec::new(),
                });
            }
            Event::End(_) => {
                if let Some(frame) = stack.pop() {
                    emit_node(content, &frame, &mut spans);
                }
            }
            Event::Code(_) => {
                if let Some(parent) = stack.last_mut() {
                    push_child(parent, range.start, range.end);
                }
                emit_code_span(content, range.start, range.end, &mut spans);
            }
            Event::Text(_) => {
                if let Some(parent) = stack.last_mut() {
                    push_child(parent, range.start, range.end);
                }
                // Math lives in text nodes, but code blocks keep their dollars
                // literally (as the preview does: it never splits math inside
                // fenced code), and headings render their `$x$` as plain text
                // too (the heading buffer bypasses `emit_math`). Marking math
                // where the preview shows literal dollars would hide syntax
                // the reader still sees, so both contexts are skipped.
                let in_verbatim = stack.iter().any(|f| matches!(f.tag, Tag::CodeBlock(_)))
                    || stack.iter().any(|f| matches!(f.tag, Tag::Heading { .. }));
                if !in_verbatim {
                    emit_math_spans(content, range.start, range.end, &mut spans);
                }
            }
            _ => {
                // Leaf events without their own spans (soft breaks, task
                // markers, …) still occupy source, so they count as children
                // for the marker gaps of whatever contains them.
                if let Some(parent) = stack.last_mut() {
                    push_child(parent, range.start, range.end);
                }
            }
        }
    }

    resolve(&index, spans)
}

/// Record a child range, clamped inside its parent. Parser ranges should
/// already nest, but clamping keeps a surprising range from producing a span
/// with `from > to` or escaping the document.
fn push_child(frame: &mut Frame, start: usize, end: usize) {
    let start = start.clamp(frame.start, frame.end);
    let end = end.clamp(frame.start, frame.end);
    // Zero-width children (e.g. an empty node) still split the gap they sit
    // in, so they are kept as zero-width separators.
    if start <= end {
        frame.children.push((start, end));
    } else {
        frame.children.push((start, start));
    }
}

/// Emit the spans of one closed construct, following the marker rule: every
/// part of its range not covered by a child is syntax, plus a style span
/// whose shape depends on the tag (see the table in the task brief).
fn emit_node(content: &str, frame: &Frame, spans: &mut ByteSpans) {
    if frame.children.is_empty() {
        // Nowhere to attach a marker: `#`, `~~~`, `[]()` and friends yield
        // nothing rather than a guess.
        return;
    }
    let mut children = frame.children.clone();
    children.sort();
    let first_start = children[0].0;
    let last_end = children
        .iter()
        .map(|&(_, e)| e)
        .max()
        .unwrap_or(first_start);

    // Gaps between `frame.start..frame.end` that no child covers.
    let mut gaps: Vec<(usize, usize)> = Vec::new();
    let mut cursor = frame.start;
    for &(s, e) in &children {
        if s > cursor {
            gaps.push((cursor, s));
        }
        cursor = cursor.max(e);
    }
    if frame.end > cursor {
        gaps.push((cursor, frame.end));
    }

    // Style span over the children union, e.g. the `a` in `**a**`.
    let inner = || (first_start, last_end);
    match &frame.tag {
        Tag::Heading { level, .. } => {
            // Styled over the text, not `start..end`: the marker (`# `, the
            // `{#attr}` tail) is hidden, and highlighting it as a heading on
            // top of hiding it would be redundant.
            let (s, e) = inner();
            push_span(content, spans, s, e, heading_tag(*level));
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Hide);
            }
        }
        Tag::Strong => {
            let (s, e) = inner();
            push_span(content, spans, s, e, SpanTag::Strong);
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Hide);
            }
        }
        Tag::Emphasis => {
            let (s, e) = inner();
            push_span(content, spans, s, e, SpanTag::Emphasis);
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Hide);
            }
        }
        Tag::Strikethrough => {
            let (s, e) = inner();
            push_span(content, spans, s, e, SpanTag::Strikethrough);
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Hide);
            }
        }
        Tag::Link { .. } => {
            let (s, e) = inner();
            push_span(content, spans, s, e, SpanTag::LinkText);
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Hide);
            }
        }
        Tag::Image { .. } => {
            let (s, e) = inner();
            push_span(content, spans, s, e, SpanTag::ImageAlt);
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Hide);
            }
        }
        Tag::BlockQuote(..) => {
            push_span(content, spans, frame.start, first_start, SpanTag::Quote);
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Hide);
            }
        }
        Tag::Item => {
            push_span(
                content,
                spans,
                frame.start,
                first_start,
                SpanTag::ListMarker,
            );
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Hide);
            }
        }
        Tag::CodeBlock(kind) => {
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::Fence);
            }
            if is_mermaid(kind) {
                push_span(
                    content,
                    spans,
                    frame.start,
                    frame.end,
                    SpanTag::MermaidBlock,
                );
            }
        }
        Tag::Table(..) => {
            // The only gap with no child events is the delimiter row
            // (`|---|`): it is syntax through and through.
            for (s, e) in gaps {
                push_span(content, spans, s, e, SpanTag::TableDelimiter);
            }
        }
        Tag::TableHead | Tag::TableRow => {
            // Cell gaps hold the `|` separators (plus padding spaces, which
            // stay visible). One span per pipe, so no span touches a space.
            for (s, e) in gaps {
                if let Some(slice) = content.get(s..e) {
                    for (i, ch) in slice.char_indices() {
                        if ch == '|' {
                            push_span(content, spans, s + i, s + i + 1, SpanTag::TablePipe);
                        }
                    }
                }
            }
        }
        // `TableCell` padding (the spaces around `a` in `| a |`) is spacing,
        // not syntax: no spans. `Paragraph`, `List`, footnotes and the rest
        // carry no live formatting either.
        _ => {}
    }
}

fn heading_tag(level: pulldown_cmark::HeadingLevel) -> SpanTag {
    match level as u8 {
        1 => SpanTag::Heading1,
        2 => SpanTag::Heading2,
        3 => SpanTag::Heading3,
        4 => SpanTag::Heading4,
        5 => SpanTag::Heading5,
        _ => SpanTag::Heading6,
    }
}

/// Whether a fenced block is a mermaid diagram, using the same rule as the
/// preview (`mod.rs` lowercases the first info word and compares exactly), so
/// the editor and the preview agree on what is a diagram.
fn is_mermaid(kind: &CodeBlockKind) -> bool {
    match kind {
        CodeBlockKind::Fenced(info) => info
            .split_whitespace()
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("mermaid")),
        CodeBlockKind::Indented => false,
    }
}

/// An inline code span covers its backticks: count the opening run, mirror it
/// at the end, style what is inside.
fn emit_code_span(content: &str, start: usize, end: usize, spans: &mut ByteSpans) {
    let Some(slice) = content.get(start..end) else {
        return;
    };
    let open = slice.bytes().take_while(|&b| b == b'`').count();
    let close = slice.bytes().rev().take_while(|&b| b == b'`').count();
    if open == 0 || open + close >= slice.len() {
        return;
    }
    push_span(content, spans, start, start + open, SpanTag::Hide);
    push_span(
        content,
        spans,
        start + open,
        end - close,
        SpanTag::InlineCode,
    );
    push_span(content, spans, end - close, end, SpanTag::Hide);
}

/// Math spans inside one text node. `split_math` partitions its input exactly
/// (text is copied verbatim, math keeps its inner TeX), so replaying its
/// pieces in order against the source slice recovers each `$…$` / `$$…$$`
/// with delimiters included — the delimiters are what gets hidden.
fn emit_math_spans(content: &str, start: usize, end: usize, spans: &mut ByteSpans) {
    let Some(slice) = content.get(start..end) else {
        return;
    };
    let mut cursor = 0usize;
    for piece in split_math(slice) {
        match piece {
            super::math::MathPiece::Text(t) => {
                cursor += t.len();
            }
            super::math::MathPiece::Inline(tex) => {
                let len = tex.len() + 2;
                push_span(
                    content,
                    spans,
                    start + cursor,
                    start + cursor + len,
                    SpanTag::MathInline,
                );
                cursor += len;
            }
            super::math::MathPiece::Display(tex) => {
                let len = tex.len() + 4;
                push_span(
                    content,
                    spans,
                    start + cursor,
                    start + cursor + len,
                    SpanTag::MathDisplay,
                );
                cursor += len;
            }
        }
    }
}

/// Push one byte-range span, trimmed of line breaks (a decoration over `\n`
/// joins lines visually in CodeMirror) and dropped when empty or outside the
/// document. `\r`/`\n` are single bytes, so trimming bytes cannot split a
/// character.
fn push_span(content: &str, spans: &mut ByteSpans, mut start: usize, mut end: usize, tag: SpanTag) {
    let bytes = content.as_bytes();
    while start < end && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    while start < end && (bytes[start] == b'\n' || bytes[start] == b'\r') {
        start += 1;
    }
    if start >= end || end > content.len() {
        return;
    }
    if content.get(start..end).is_none() {
        return;
    }
    spans.push((start, end, tag));
}

/// Sort, convert to UTF-16, and resolve `Hide` overlaps (outer wins).
///
/// Both dedup scans are O(1) per span, not O(kept): spans arrive sorted by
/// `(from, to)`, so exact duplicates are adjacent (one lookback suffices)
/// and kept `Hide`s are non-overlapping and in `from` order (a newcomer
/// overlaps iff it starts before the last kept end). The previous
/// `kept_hides.iter().any(...)` + full-output scan were O(n²) — invisible
/// on small notes, ~60 ms medidos por pulsación on a 5 000-line
/// document, where the live editor recomputes spans on every change.
fn resolve(index: &Utf16Index, mut spans: ByteSpans) -> LiveSpans {
    // Sort by `(from, to)`: an outer span starts at or before any span it
    // contains, so the outer sorts first and — kept first — wins, as the
    // spec requires. The sort is stable, so reparse yields identical output.
    spans.sort_by_key(|a| (a.0, a.1));
    let mut out = LiveSpans::default();
    // End of the last kept `Hide`. Kept hides never overlap and arrive in
    // `from` order, so `from < hide_end` is exactly "overlaps a kept hide".
    // Starts at 0, which no span can be under (`from >= 0`).
    let mut hide_end: u32 = 0;
    // Last emitted span: sorted input puts exact duplicates adjacently.
    let mut last: Option<(u32, u32, SpanTag)> = None;
    for (start, end, tag) in spans {
        let (Some(from), Some(to)) = (index.to_utf16(start), index.to_utf16(end)) else {
            continue;
        };
        if from > to {
            continue;
        }
        if from == to {
            continue;
        }
        if tag == SpanTag::Hide {
            // Outer wins: whatever is already kept takes precedence, so an
            // overlapping newcomer is discarded, as the spec requires.
            if from < hide_end {
                continue;
            }
            // `to > from >= hide_end`, so this also advances the end.
            hide_end = to;
        }
        // Exact duplicates cannot arise from nested-or-disjoint gaps, but a
        // second guard costs nothing and keeps the JS side from decorating
        // the same range twice with the same tag.
        if last == Some((from, to, tag)) {
            continue;
        }
        last = Some((from, to, tag));
        out.ranges.push(from);
        out.ranges.push(to);
        out.tags.push(tag);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flatten a [`LiveSpans`] for readable assertions.
    fn spans(src: &str) -> Vec<(u32, u32, SpanTag)> {
        let live = live_spans(src);
        live.ranges
            .chunks_exact(2)
            .zip(live.tags.iter())
            .map(|(r, &t)| (r[0], r[1], t))
            .collect()
    }

    #[test]
    fn heading_marker_is_hidden() {
        // "# Hola" -> un Hide en 0..2 y un Heading1 en 2..6
        assert_eq!(
            spans("# Hola"),
            vec![(0, 2, SpanTag::Hide), (2, 6, SpanTag::Heading1)]
        );
    }

    #[test]
    fn strong_hides_both_delimiters() {
        // `**a**` is 5 bytes: `**` + `a` + `**`. The brief's 0..1/1..2/2..3
        // sketch counts one delimiter char; the parser reports both, so the
        // actual spans hide 0..2 and 3..5 around the styled 2..3.
        assert_eq!(
            spans("**a**"),
            vec![
                (0, 2, SpanTag::Hide),
                (2, 3, SpanTag::Strong),
                (3, 5, SpanTag::Hide),
            ]
        );
    }

    #[test]
    fn offsets_are_utf16_not_bytes() {
        // "# ñ😀": el tramo oculto mide 0..2 en UTF-16, aunque en bytes sea
        // 0..2 aquí también — lo que cambia es el texto: en bytes sería
        // 2..8 (`ñ` = 2 bytes, `😀` = 4), en UTF-16 es 2..5 (1 + 2 unidades).
        // Con bytes, el tramo marcado mediría el doble y el editor partiría
        // el emoji. Este es el test que más importa.
        assert_eq!(
            spans("# ñ😀"),
            vec![(0, 2, SpanTag::Hide), (2, 5, SpanTag::Heading1)]
        );
    }

    #[test]
    fn link_hides_bracket_and_url() {
        // "[t](u)" -> Hide 0..1 (el "["), LinkText 1..2, Hide 2..6 ("](u)").
        // El brief anota 2..5; el cierre ocupa 2..6 (`]`, `(`, `u`, `)`).
        assert_eq!(
            spans("[t](u)"),
            vec![
                (0, 1, SpanTag::Hide),
                (1, 2, SpanTag::LinkText),
                (2, 6, SpanTag::Hide),
            ]
        );
    }

    #[test]
    fn degenerate_input_does_not_panic() {
        // "**", "[]()", "####### x", "#", "~~~", "|---|", "$", "$$", "- [ ]"
        // Ninguno entra en panic y ninguno devuelve un tramo con from > to.
        for src in [
            "**",
            "[]()",
            "####### x",
            "#",
            "~~~",
            "|---|",
            "$",
            "$$",
            "- [ ]",
        ] {
            let got = spans(src);
            for &(from, to, _) in &got {
                assert!(from <= to, "inverted span in {src:?}: {got:?}");
            }
        }
    }

    #[test]
    fn nested_emphasis_has_no_overlapping_hides() {
        // "**a *b* c**": ningún par de Hide se solapa.
        let hides: Vec<(u32, u32)> = spans("**a *b* c**")
            .into_iter()
            .filter(|&(_, _, t)| t == SpanTag::Hide)
            .map(|(s, e, _)| (s, e))
            .collect();
        assert!(!hides.is_empty());
        for (i, &a) in hides.iter().enumerate() {
            for &b in &hides[i + 1..] {
                assert!(
                    !(a.0 < b.1 && b.0 < a.1),
                    "overlapping Hides {a:?} and {b:?}"
                );
            }
        }
    }

    #[test]
    fn utf16_index_counts_multibyte_chars() {
        let index = Utf16Index::new("aé😀b");
        // a=1, é=1, 😀=2, b=1 → 5 units over 8 bytes.
        assert_eq!(index.to_utf16(8), Some(5));
        assert_eq!(index.to_utf16(0), Some(0));
        assert_eq!(index.to_utf16(1), Some(1));
        assert_eq!(index.to_utf16(3), Some(2));
        assert_eq!(index.to_utf16(7), Some(4));
        assert_eq!(index.to_utf16(8), Some(5));
        assert_eq!(index.to_utf16(9), None);
        // Mid-character offsets are not representable.
        assert_eq!(index.to_utf16(2), None);
        assert_eq!(index.to_utf16(5), None);
    }

    #[test]
    fn utf16_index_handles_multiple_lines() {
        let index = Utf16Index::new("é\n😀x");
        assert_eq!(index.to_utf16(0), Some(0));
        assert_eq!(index.to_utf16(3), Some(2)); // start of line 2
        assert_eq!(index.to_utf16(7), Some(4)); // the `x`
        assert_eq!(index.to_utf16(8), Some(5)); // end of document
        assert_eq!(index.to_utf16(9), None);
    }

    #[test]
    fn image_marks_alt_and_hides_syntax() {
        assert_eq!(
            spans("![alt](img.png)"),
            vec![
                (0, 2, SpanTag::Hide),
                (2, 5, SpanTag::ImageAlt),
                (5, 15, SpanTag::Hide),
            ]
        );
    }

    #[test]
    fn emphasis_and_strikethrough_styles() {
        assert!(spans("*em*").contains(&(1, 3, SpanTag::Emphasis)));
        assert!(spans("~~s~~").contains(&(2, 3, SpanTag::Strikethrough)));
    }

    #[test]
    fn inline_code_hides_backticks() {
        assert_eq!(
            spans("`code`"),
            vec![
                (0, 1, SpanTag::Hide),
                (1, 5, SpanTag::InlineCode),
                (5, 6, SpanTag::Hide),
            ]
        );
    }

    #[test]
    fn blockquote_marks_prefix() {
        let got = spans("> quote");
        assert!(got.contains(&(0, 2, SpanTag::Hide)), "{got:?}");
        assert!(got.contains(&(0, 2, SpanTag::Quote)), "{got:?}");
    }

    #[test]
    fn list_item_marks_bullet() {
        let got = spans("- item");
        assert!(got.contains(&(0, 2, SpanTag::Hide)), "{got:?}");
        assert!(got.contains(&(0, 2, SpanTag::ListMarker)), "{got:?}");
    }

    #[test]
    fn fenced_block_marks_fences() {
        let got = spans("```rust\ncode\n```");
        assert!(got.contains(&(0, 7, SpanTag::Fence)), "{got:?}");
        assert!(got.contains(&(13, 16, SpanTag::Fence)), "{got:?}");
        assert!(!got.iter().any(|&(_, _, t)| t == SpanTag::Hide), "{got:?}");
    }

    #[test]
    fn mermaid_block_is_marked_whole() {
        let src = "```mermaid\ngraph TD;\n```";
        let got = spans(src);
        let total = src.encode_utf16().count() as u32;
        assert!(got.contains(&(0, total, SpanTag::MermaidBlock)), "{got:?}");
        assert!(got.iter().any(|&(_, _, t)| t == SpanTag::Fence), "{got:?}");
    }

    #[test]
    fn non_mermaid_block_has_no_mermaid_span() {
        let got = spans("```rust\ncode\n```");
        assert!(
            !got.iter().any(|&(_, _, t)| t == SpanTag::MermaidBlock),
            "{got:?}"
        );
    }

    #[test]
    fn table_pipes_and_delimiter_are_marked() {
        let got = spans("| a | b |\n|---|---|\n| 1 | 2 |\n");
        let pipes: usize = got
            .iter()
            .filter(|&&(_, _, t)| t == SpanTag::TablePipe)
            .count();
        // Header row and body row contribute 3 pipes each.
        assert_eq!(pipes, 6, "{got:?}");
        assert!(
            got.iter().any(|&(_, _, t)| t == SpanTag::TableDelimiter),
            "{got:?}"
        );
    }

    #[test]
    fn inline_math_covers_delimiters() {
        // MathInline es el rango completo entre `$` y `$`, delimiters incluidos.
        let got = spans("see $x$ ok");
        assert!(got.contains(&(4, 7, SpanTag::MathInline)), "{got:?}");
    }

    #[test]
    fn display_math_covers_delimiters() {
        let got = spans("$$a+b$$");
        assert!(got.contains(&(0, 7, SpanTag::MathDisplay)), "{got:?}");
    }

    #[test]
    fn currency_is_not_math() {
        let got = spans("It costs $5 and $10.");
        assert!(
            !got.iter().any(|&(_, _, t)| t == SpanTag::MathInline),
            "{got:?}"
        );
    }

    #[test]
    fn math_inside_code_is_literal() {
        let got = spans("```sh\necho $HOME\n```");
        assert!(
            !got.iter().any(|&(_, _, t)| t == SpanTag::MathInline),
            "{got:?}"
        );
    }

    #[test]
    fn heading_levels_map() {
        assert!(spans("## H2")
            .iter()
            .any(|&(_, _, t)| t == SpanTag::Heading2));
        assert!(spans("###### H6")
            .iter()
            .any(|&(_, _, t)| t == SpanTag::Heading6));
    }

    #[test]
    fn empty_heading_has_no_spans() {
        assert!(spans("#").is_empty());
    }

    #[test]
    fn live_spans_shape_matches_contract() {
        let live = live_spans("# Hola");
        assert_eq!(live.ranges.len(), live.tags.len() * 2);
        assert_eq!(live.ranges, vec![0, 2, 2, 6]);
    }
}
