use leptos::prelude::*;

use crate::state::EditorState;
use crate::utils::markdown::Heading;

/// Panel listing the current document's headings.
///
/// Each entry is a button that scrolls the preview to that heading. The previous
/// version rendered buttons with no handler at all — dead UI that looked
/// interactive.
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
    // `anchor` comes from the renderer, not from re-slugifying `text`. That
    // distinction matters: the renderer deduplicates repeated headings and falls
    // back to `seccion` when a heading has no slug-worthy characters, so a
    // recomputed slug would point at an id that does not exist.
    let anchor = heading.anchor;
    let label = heading.text;

    view! {
        <button
            class="block w-full text-left text-xs text-base-600 dark:text-base-400 \
                   hover:text-brand-orange dark:hover:text-brand-orange \
                   hover:bg-base-100 dark:hover:bg-base-800/50 px-2 py-1 rounded \
                   transition-colors truncate"
            style:padding-left=format!("{}px", indent + 4)
            title=label.clone()
            on:click=move |_| scroll_to_heading(&anchor)
        >
            {label.clone()}
        </button>
    }
}

/// Scroll the preview to the heading with this anchor.
///
/// Slug generation must match the renderer's; if the two ever disagree this
/// silently does nothing, so it is centralised here and unit-tested.
fn scroll_to_heading(anchor: &str) {
    let Some(element) = leptos::prelude::document().get_element_by_id(anchor) else {
        return;
    };
    element.scroll_into_view();
}

/// Heading slug, shared with the renderer.
///
/// Not reimplemented here: the outline scrolls by id, and those ids come from
/// the renderer. Two copies of this function would drift and silently break
/// scrolling. The panel itself uses [`Heading::anchor`] — the exact id the
/// renderer emitted — rather than re-slugifying the heading text, because the
/// renderer deduplicates repeated headings and falls back to `seccion` when a
/// heading has no slug-worthy characters.
#[cfg(test)]
pub use codedocs_core::markdown::slugify;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_outline_anchor_exists_in_the_rendered_html() {
        // The contract the scroll behaviour depends on: each anchor the outline
        // looks up must be an id the renderer actually emitted.
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
        // Otherwise every entry in the outline scrolls to the first heading.
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
