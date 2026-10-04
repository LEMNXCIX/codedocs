//! Adversarial probes: tag balance under nested link/image suppression, and
//! agreement between the outline anchors and the ids the renderer emits.
//!
//! Both were found by fuzzing nesting combinations; kept because each pins a
//! real defect that is easy to reintroduce.

use std::collections::HashSet;

use codedocs_core::{extract_headings, render_markdown};

/// Whether the tags in `html` are balanced.
///
/// Only checks the tags the renderer emits itself; document content is escaped
/// and therefore cannot produce an unbalanced tag on its own.
fn balanced(html: &str) -> bool {
    let mut stack: Vec<String> = Vec::new();
    let bytes: Vec<char> = html.chars().collect();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != '<' {
            i += 1;
            continue;
        }
        let rest: String = bytes[i..].iter().collect();
        let Some(end) = rest.find('>') else {
            return false;
        };
        let tag = &rest[1..end];
        if let Some(closing) = tag.strip_prefix('/') {
            if stack.pop().as_deref() != Some(closing.trim()) {
                return false;
            }
        } else if !tag.ends_with('/') && !tag.starts_with('!') {
            let name = tag.split_whitespace().next().unwrap_or_default();
            if !name.is_empty() {
                stack.push(name.to_string());
            }
        }
        i += end + 1;
    }

    stack.is_empty()
}

/// An unsafe URL must never reach an attribute, whatever it is nested in.
fn assert_no_unsafe_url(md: &str) {
    let html = render_markdown(md);
    assert!(
        !html.contains("href=\"javascript:") && !html.contains("src=\"javascript:"),
        "unsafe URL in an attribute for {md:?}: {html}"
    );
    assert!(
        !html.to_lowercase().contains("<script"),
        "script leaked for {md:?}: {html}"
    );
}

#[test]
fn nested_link_and_image_suppression_stays_balanced() {
    // A single suppression counter used to swallow the inner `</a>` when an
    // unsafe image wrapped a safe link, leaving a dangling anchor behind.
    let inners = [
        "[a](https://ok)",
        "[a](javascript:alert(1))",
        "![a](https://ok/a.png)",
        "![a](javascript:alert(1))",
    ];
    let outers = [
        "[{i}](https://ok)",
        "[{i}](javascript:alert(1))",
        "![{i}](https://ok/a.png)",
        "![{i}](javascript:alert(1))",
    ];

    for outer in outers {
        for inner in inners {
            let md = outer.replace("{i}", inner);
            assert_no_unsafe_url(&md);
            let html = render_markdown(&md);
            assert!(balanced(&html), "unbalanced tags for {md:?}: {html}");
        }
    }
}

#[test]
fn images_inside_images_stay_balanced() {
    for md in [
        "![a ![b](https://ok/b.png)](https://ok/a.png)\n",
        "![a ![b](javascript:x)](https://ok/a.png)\n",
        "[![a](https://ok/a.png) text](javascript:x)\n",
        "[![a](javascript:x) text](https://ok)\n",
    ] {
        assert_no_unsafe_url(md);
        let html = render_markdown(md);
        assert!(balanced(&html), "unbalanced tags for {md:?}: {html}");
    }
}

#[test]
fn outline_anchors_are_ids_that_exist() {
    // The outline scrolls by anchor; a recomputed slug that does not match the
    // renderer's id makes the click silently do nothing.
    for md in [
        "# Hello World\n",
        "# Notes\n\n# notes\n\n# Notes\n",
        "# !!! ???\n\n# ***\n",
        "# Ünïcödé 中文 🎉\n",
        "```\n# not a heading\n```\n\n# real\n",
        "> # Quoted\n",
        "- # In list\n",
        "## Hello **world** and `code`\n",
    ] {
        let html = render_markdown(md);
        for heading in extract_headings(md) {
            assert!(
                html.contains(&format!(r#"id="{}""#, heading.anchor)),
                "outline anchor {:?} for {:?} has no matching id in {html}",
                heading.anchor,
                heading.text
            );
        }
    }
}

#[test]
fn duplicate_headings_get_distinct_anchors() {
    // Otherwise every outline entry scrolls to the first heading.
    let headings = extract_headings("# Notes\n\n# notes\n\n# Notes\n");
    let anchors: Vec<&str> = headings.iter().map(|h| h.anchor.as_str()).collect();
    let unique: HashSet<&&str> = anchors.iter().collect();
    assert_eq!(
        anchors.len(),
        unique.len(),
        "duplicate anchors: {anchors:?}"
    );
}

#[test]
fn headings_inside_nested_containers_are_still_anchored() {
    for (md, expected) in [
        ("> # Quoted\n", "quoted"),
        ("- # In list\n", "in-list"),
        ("1. # Numbered\n", "numbered"),
        ("> > # Deep\n", "deep"),
    ] {
        let headings = extract_headings(md);
        assert_eq!(headings.len(), 1, "{md:?}");
        assert_eq!(headings[0].anchor, expected, "{md:?}");
    }
}

#[test]
fn heading_inside_a_fenced_block_is_not_a_heading() {
    let md = "```md\n# Not a heading\n```\n\n# Real\n";
    let headings = extract_headings(md);
    assert_eq!(headings.len(), 1, "{md:?}");
    assert_eq!(headings[0].text, "Real");
    assert!(!render_markdown(md).contains(r#"id="not-a-heading""#));
}
