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

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<FileEntry>,
}

#[component]
pub fn TreeItem(
    item: FileEntry,
    on_click: Callback<String>,
    on_rename: Callback<String>,
    on_delete: Callback<String>,
) -> impl IntoView {
    let (is_expanded, set_is_expanded) = signal(false);

    let item_clone = item.clone();
    let on_item_click = move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        if item_clone.is_dir {
            set_is_expanded.update(|v| *v = !*v);
        } else {
            on_click.run(item_clone.path.clone());
        }
    };

    let children = item.children.clone();

    view! {
        <li class="select-none group">
            <div
                class="flex items-center gap-2 py-1 px-2 rounded-md cursor-pointer transition-colors hover:bg-slate-200 dark:hover:bg-slate-800 text-sm"
                on:click=on_item_click
            >
                <span class="w-4 flex justify-center text-slate-400">
                    {if item.is_dir {
                        if is_expanded.get() { "▾" } else { "▸" }
                    } else {
                        ""
                    }}
                </span>
                <span class="text-lg">
                    {if item.is_dir { "📁" } else { "📄" }}
                </span>
                <span class="truncate text-slate-700 dark:text-slate-300 flex-1">
                    {item.name.clone()}
                </span>
                {if !item.is_dir {
                    let path_del = item.path.clone();
                    let path_ren = item.path.clone();
                    view! {
                        <button
                            class="opacity-0 group-hover:opacity-100 p-1 text-slate-400 hover:text-red-500 transition-all"
                            on:click=move |ev| {
                                ev.stop_propagation();
                                on_delete.run(path_del.clone());
                            }
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
                        </button>
                        <button
                            class="opacity-0 group-hover:opacity-100 p-1 text-slate-400 hover:text-blue-500 transition-all"
                            on:click=move |ev| {
                                ev.stop_propagation();
                                on_rename.run(path_ren.clone());
                            }
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                        </button>
                    }.into_any()
                } else {
                    ().into_any()
                }}
            </div>
            {move || if is_expanded.get() && item.is_dir {
                view! {
                    <ul class="ml-4 border-l border-slate-200 dark:border-slate-800 mt-1 space-y-0.5">
                        {children.clone().into_iter().map(|child| {
                            view! { <TreeItem item=child on_click=on_click on_rename=on_rename on_delete=on_delete /> }
                        }).collect_view()}
                    </ul>
                }.into_any()
            } else {
                ().into_any()
            }}
        </li>
    }
}

#[component]
pub fn FileTree(
    items: ReadSignal<Vec<FileEntry>>,
    on_click: Callback<String>,
    on_rename: Callback<String>,
    on_delete: Callback<String>,
) -> impl IntoView {
    view! {
        <ul class="space-y-1 mt-4">
            {move || {
                items.get().into_iter().map(|item| {
                    view! { <TreeItem item=item on_click=on_click on_rename=on_rename on_delete=on_delete /> }
                }).collect_view()
            }}
        </ul>
    }
}
