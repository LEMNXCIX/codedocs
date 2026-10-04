use leptos::prelude::*;

use crate::components::editor::codemirror::set_cursor;
use crate::state::EditorState;
use crate::utils::markdown::Heading;

/// Panel listing the current document's headings.
///
/// Each entry is a button that moves the editor cursor to that heading. The
/// preview this used to scroll is gone — the live editor owns the document
/// — so jumping the cursor (and scrolling it into view) is what a click
/// does now.
#[component]
pub fn OutlinePanel(state: EditorState) -> impl IntoView {
    view! {
        <nav class="flex flex-col gap-1" aria-label="Contenido del documento">
            <h2 class="text-[10px] font-bold uppercase tracking-widest text-base-400 \
                      dark:text-base-500 px-1 mb-2">
                "Contenido"
            </h2>
            {move || {
                let headings = state.headings.get();
                if headings.is_empty() {
                    view! {
                        <p class="text-xs text-base-400 dark:text-base-600 italic px-1">
                            "Sin encabezados"
                        </p>
                    }
                    .into_any()
                } else {
                    headings
                        .into_iter()
                        .map(|heading| view! { <OutlineLink heading /> })
                        .collect_view()
                        .into_any()
                }
            }}
        </nav>
    }
}

#[component]
fn OutlineLink(heading: Heading) -> impl IntoView {
    // `saturating_sub` guards the cast: a malformed level of 0 would otherwise
    // wrap to a huge `usize` and blow up the padding.
    let indent = heading.level.saturating_sub(1) as usize * 12;
    // The jump target is `offset` (UTF-16 units, as CodeMirror counts them):
    // with no preview pane left there is no element with the anchor id in
    // the DOM, so scrolling by id silently did nothing.
    let label = heading.text;
    let offset = heading.offset as u32;

    view! {
        <button
            class="block w-full text-left text-xs text-base-600 dark:text-base-400 \
                   hover:text-brand-orange dark:hover:text-brand-orange \
                   hover:bg-base-100 dark:hover:bg-base-800/50 px-2 py-1 rounded \
                   transition-colors truncate"
            style:padding-left=format!("{}px", indent + 4)
            title=label.clone()
            on:click=move |_| set_cursor(offset)
        >
            {label.clone()}
        </button>
    }
}

/// Heading slug, shared with the renderer.
///
/// The outline jumps the cursor by `offset` now, not by id, but the renderer
/// still emits these ids and `extract_headings` still carries them, so the
/// agreement is pinned here: two copies of the slug function would drift
/// and silently break anything that looks a heading up by id.
#[cfg(test)]
mod tests {
    #[test]
    fn every_outline_anchor_exists_in_the_rendered_html() {
        // The renderer still emits these ids and `extract_headings` still
        // carries them; anything that looks a heading up by id depends on
        // this agreement holding.
        for md in [
            "# Hello World\n",
            "# Notes\n\n# notes\n\n# Notes\n",
            "# !!! ???\n\n# ***\n",
            "# Ünïcödé 中文 🎉\n",
            "```\n# not a heading\n```\n\n# real\n",
            "> # Quoted\n",
            "## Hello **world** and `code`\n",
        ] {
            let html = codedocs_core::render_markdown(md);
            for heading in codedocs_core::extract_headings(md) {
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
        // Otherwise every outline entry would share one identity.
        let headings = codedocs_core::extract_headings("# Notes\n\n# notes\n\n# Notes\n");
        let anchors: Vec<&str> = headings.iter().map(|h| h.anchor.as_str()).collect();
        let unique: std::collections::HashSet<&&str> = anchors.iter().collect();
        assert_eq!(
            anchors.len(),
            unique.len(),
            "duplicate anchors: {anchors:?}"
        );
    }

    #[test]
    fn indentation_never_wraps() {
        // A level of 0 must not underflow into a huge padding value.
        let indent = 0u8.saturating_sub(1) as usize * 12;
        assert_eq!(indent, 0);
    }
}
