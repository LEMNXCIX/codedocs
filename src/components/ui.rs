use leptos::prelude::*;

#[component]
pub fn Button(
    #[prop(into)] on_click: Callback<leptos::ev::MouseEvent>,
    children: Children,
) -> impl IntoView {
    view! {
        <button class="px-4 py-2 mt-2 bg-slate-900 hover:bg-slate-800 dark:bg-white dark:hover:bg-slate-200 text-white dark:text-slate-900 rounded-md text-sm font-medium transition-all shadow-sm active:scale-95"
            on:click=move |ev| on_click.run(ev)>
            {children()}
        </button>
    }
}

#[component]
pub fn List(
    items: ReadSignal<Vec<String>>,
    on_click: Callback<String>,
) -> impl IntoView {
    view! {
        <ul class="list-disc list-inside space-y-1">
            {move || {
                items.get().into_iter().map(|item| {
                    let item_clone1 = item.clone();
                    let item_clone2 = item.clone();
                    view! {
                        <li
                            class="text-slate-700 dark:text-slate-300 cursor-pointer hover:bg-slate-100 dark:hover:bg-slate-700 p-1 rounded"
                            on:click=move |_| {
                                on_click.run(item_clone1.clone());
                            }
                        >
                            {move || { let item = item_clone2.clone(); if item.contains('/') { format!("📁 {}", item) } else { item } }}
                        </li>
                    }
                }).collect::<Vec<_>>()
            }}
        </ul>
    }
}
