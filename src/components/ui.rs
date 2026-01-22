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
pub fn TreeItem(item: FileEntry, on_click: Callback<String>) -> impl IntoView {
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
        <li class="select-none">
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
                <span class="truncate text-slate-700 dark:text-slate-300">
                    {item.name}
                </span>
            </div>
            {move || if is_expanded.get() && item.is_dir {
                view! {
                    <ul class="ml-4 border-l border-slate-200 dark:border-slate-800 mt-1 space-y-0.5">
                        {children.clone().into_iter().map(|child| {
                            view! { <TreeItem item=child on_click=on_click /> }
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
pub fn FileTree(items: ReadSignal<Vec<FileEntry>>, on_click: Callback<String>) -> impl IntoView {
    view! {
        <ul class="space-y-1 mt-4">
            {move || {
                items.get().into_iter().map(|item| {
                    view! { <TreeItem item=item on_click=on_click /> }
                }).collect_view()
            }}
        </ul>
    }
}
