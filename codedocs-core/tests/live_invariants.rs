//! Invariants of `live_spans` over arbitrary documents: sorted spans inside
//! the document bounds, deterministic output, and no spans for empty input.
//!
//! These pin the contract the editor relies on: CodeMirror decorations with
//! out-of-range or unsorted offsets misbehave, so any document — including
//! adversarial ones with emoji, tables and unclosed fences — must satisfy
//! them.

use codedocs_core::{live_spans, LiveSpans};

/// Documents exercising every span kind plus the degenerate corners:
/// multibyte text, unclosed constructs, currency that looks like math.
fn corpus() -> Vec<&'static str> {
    vec![
        "",
        "\n",
        "# Hola",
        "# ñ😀 emoji 🎉",
        "## H2 {#custom}",
        "Setext\n=====\n",
        "**a**",
        "**a *b* c**",
        "*em* and ~~gone~~ and `code`",
        "[t](u)",
        "![alt](img.png)",
        "> quote\n> more",
        "> - nested *list*",
        "- item",
        "1. ordered",
        "- [ ] task",
        "- [x] done",
        "```rust\ncode\n```",
        "```mermaid\ngraph TD;\n```",
        "```\nunclosed",
        "~~~",
        "| a | b |\n|---|---|\n| 1 | 2 |\n",
        "|---|",
        "see $x$ ok",
        "$$a+b$$",
        "It costs $5 and $10.",
        "$",
        "$$",
        "**",
        "[]()",
        "####### x",
        "#",
        "Text [with $math$](https://example.com) inside",
        "# Título con *énfasis* y `código`",
        "para one\n\npara two\n",
        "日本語の見出し\n\n- 箇条書き 🎉\n",
    ]
}

fn flattened(live: &LiveSpans) -> Vec<(u32, u32)> {
    live.ranges.chunks_exact(2).map(|r| (r[0], r[1])).collect()
}

fn utf16_len(doc: &str) -> u32 {
    doc.encode_utf16().count() as u32
}

#[test]
fn spans_are_sorted_and_within_bounds() {
    for doc in corpus() {
        let live = live_spans(doc);
        assert_eq!(
            live.ranges.len(),
            live.tags.len() * 2,
            "ranges/tags out of sync for {doc:?}"
        );
        let flat = flattened(&live);
        let mut prev = 0u32;
        for &(from, to) in &flat {
            assert!(from <= to, "inverted span {from}..{to} for {doc:?}");
            assert!(from >= prev, "unsorted spans for {doc:?}: {flat:?}");
            prev = from;
        }
    }
}

#[test]
fn no_span_exceeds_the_utf16_length_of_the_document() {
    for doc in corpus() {
        let live = live_spans(doc);
        let len = utf16_len(doc);
        for &(from, to) in &flattened(&live) {
            assert!(
                to <= len,
                "span {from}..{to} exceeds UTF-16 length {len} for {doc:?}"
            );
        }
    }
}

#[test]
fn empty_document_produces_no_spans() {
    assert_eq!(live_spans(""), LiveSpans::default());
}

#[test]
fn spans_are_stable_under_reparse() {
    for doc in corpus() {
        assert_eq!(live_spans(doc), live_spans(doc), "unstable for {doc:?}");
    }
}
