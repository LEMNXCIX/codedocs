use crate::utils::markdown::Heading;
use leptos::prelude::*;

#[component]
pub fn OutlinePanel(headings: ReadSignal<Vec<Heading>>) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-1">
            <h2 class="text-[10px] font-bold uppercase tracking-widest text-base-400 dark:text-base-500 px-1 mb-2">
                "Contenido"
            </h2>
            {move || {
                let h = headings.get();
                if h.is_empty() {
                    view! {
                        <p class="text-xs text-base-400 dark:text-base-600 italic px-1">
                            "Sin headings"
                        </p>
                    }.into_any()
                } else {
                    h.into_iter().map(|heading| {
                        let indent = (heading.level - 1) as usize * 12;
                        let text = heading.text.clone();
                        let title = text.clone();
                        view! {
                            <button
                                class="block w-full text-left text-xs text-base-600 dark:text-base-400 hover:text-brand-orange dark:hover:text-brand-orange hover:bg-base-100 dark:hover:bg-base-800/50 px-2 py-1 rounded transition-colors truncate"
                                style:padding-left=format!("{}px", indent + 4)
                                title=title
                            >
                                {text}
                            </button>
                        }
                    }).collect_view().into_any()
                }
            }}
        </div>
    }
}
