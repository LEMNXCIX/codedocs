mod outline;

use leptos::prelude::*;
use leptos::reactive::spawn_local;

use crate::actions;
use crate::state::EditorState;
use crate::types::FileEntry;
use crate::utils::env::is_tauri;
use crate::utils::tauri_bridge;
use crate::watch::watch_workspace;

/// Which sidebar tab is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SidebarTab {
    #[default]
    Files,
    Outline,
}

/// Project sidebar: logo, tab switch, and either the file tree or the outline.
#[component]
pub fn Sidebar(
    state: EditorState,
    on_delete: Callback<String>,
    on_rename: Callback<String>,
    create_new_file: Callback<()>,
) -> impl IntoView {
    let (active_tab, set_active_tab) = signal(SidebarTab::Files);

    view! {
        <aside
            class="border-r border-base-200 dark:border-base-800 bg-base-100 dark:bg-base-900 \
                   flex flex-col flex-shrink-0 overflow-hidden"
            style:width=move || format!("{}px", state.sidebar_width.get())
        >
            <div class="p-6 border-b border-base-200 dark:border-base-800">
                <Logo state />
                <div class="mb-6"><ModeBadge /></div>
                <TabSwitch active_tab set_active_tab />
                {move || match active_tab.get() {
                    SidebarTab::Files => view! {
                        <div class="flex flex-col gap-2 mt-2">
                            <OpenFolderButton state />
                            <Show when=move || is_tauri()>
                                <button
                                    class="flex items-center gap-2 px-3 py-1.5 bg-base-100 \
                                           hover:bg-base-200 dark:bg-base-800 \
                                           dark:hover:bg-base-700 text-base-700 dark:text-base-300 \
                                           rounded-md text-xs font-medium transition-all"
                                    on:click=move |_| create_new_file.run(())
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14"
                                        viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                        stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M12 5v14M5 12h14"/></svg>
                                    "Nuevo Archivo"
                                </button>
                            </Show>
                        </div>
                    }
                    .into_any(),
                    SidebarTab::Outline => ().into_any(),
                }}
            </div>

            <div class="flex-1 overflow-y-auto p-4 custom-scrollbar">
                {move || match active_tab.get() {
                    SidebarTab::Files => view! {
                        <div>
                            <p class="text-[11px] font-mono text-base-400 dark:text-base-600 \
                                      truncate bg-base-100 dark:bg-base-900/50 p-2 rounded \
                                      border border-base-200 dark:border-base-800 mb-4"
                               title=move || state.path.get().unwrap_or_default()>
                                {move || state.path.get().unwrap_or_else(|| "Sin carpeta".into())}
                            </p>
                            <FileTree items=state.files state on_delete on_rename />
                        </div>
                    }
                    .into_any(),
                    SidebarTab::Outline => view! {
                        <outline::OutlinePanel state />
                    }
                    .into_any(),
                }}
            </div>
        </aside>
    }
}

/// Wordmark. Double-click toggles the colour scheme.
#[component]
fn Logo(state: EditorState) -> impl IntoView {
    view! {
        <div
            class="group flex items-center gap-3 cursor-pointer select-none mb-6"
            title="Doble clic para cambiar entre claro y oscuro"
            on:dblclick=move |_| {
                let is_dark = !state.is_dark.get_untracked();
                state.is_dark.set(is_dark);
                // The `dark` class drives every Tailwind `dark:` variant, so it
                // has to move with the signal. `class_list().toggle` only takes
                // a token, so the class is added or removed explicitly.
                if let Some(el) = leptos::prelude::document().document_element() {
                    let classes = el.class_list();
                    if is_dark {
                        let _ = classes.add_1("dark");
                    } else {
                        let _ = classes.remove_1("dark");
                    }
                }
            }
        >
            <svg width="32" height="32" viewBox="0 0 1000 1000" fill="none" xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 opacity-90 group-hover:opacity-100 transition-opacity flex-shrink-0">
                <g transform="matrix(0.994487,-0.104858,0.104858,0.994487,-94.013334,48.040307)">
                    <path d="M923.276,183.448L923.276,860.431C923.276,904.775 887.275,940.776 842.931,940.776L200.172,940.776C155.829,940.776 119.828,904.775 119.828,860.431L119.828,183.448C119.828,139.105 155.829,103.103 200.172,103.103L842.931,103.103C887.275,103.103 923.276,139.105 923.276,183.448Z" style="fill:rgb(252,255,255);stroke:rgb(142,142,142);stroke-opacity:0.24;stroke-width:4.17px;"/>
                </g>
                <g transform="matrix(1,0,0,1,24.002123,7.817214)">
                    <g transform="matrix(1,0,0,1,-2.643528,-61.595632)">
                        <path d="M923.276,183.448L923.276,860.431C923.276,904.775 887.275,940.776 842.931,940.776L200.172,940.776C155.829,940.776 119.828,904.775 119.828,860.431L119.828,183.448C119.828,139.105 155.829,103.103 200.172,103.103L842.931,103.103C887.275,103.103 923.276,139.105 923.276,183.448Z" style="fill:rgb(252,255,255);stroke:rgb(141,141,141);stroke-opacity:0.24;stroke-width:4.17px;"/>
                    </g>
                    <g transform="matrix(1,0,0,0.884146,1.390949,121.707249)">
                        <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                    </g>
                    <g transform="matrix(1,0,0,0.884146,1.390949,246.83121)">
                        <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                    </g>
                    <g transform="matrix(1,0,0,0.884146,1.390949,361.141567)">
                        <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                    </g>
                    <g transform="matrix(1,0,0,0.884146,1.390949,594.189815)">
                        <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                    </g>
                    <g transform="matrix(1,0,0,0.884146,1.390949,483.624274)">
                        <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(243,243,243);"/>
                    </g>
                    <g transform="matrix(1,0,0,0.884146,1.390949,7.412977)">
                        <path d="M860.69,164.448C860.69,176.933 851.728,187.069 840.69,187.069L194.345,187.069C183.307,187.069 174.345,176.933 174.345,164.448C174.345,151.964 183.307,141.828 194.345,141.828L840.69,141.828C851.728,141.828 860.69,151.964 860.69,164.448Z" style="fill:rgb(242,242,242);"/>
                    </g>
                    <g transform="matrix(0.544512,0,0,0.544512,-18.662162,377.007267)">
                        <text x="362.069px" y="450.431px" style="font-family:'UnifrakturMaguntia', sans-serif;font-size:833.416px;fill:rgb(47,47,47);">CD</text>
                    </g>
                </g>
            </svg>
            <h1 class="text-xl tracking-tight text-base-900 dark:text-base-100 font-unifraktur">
                "CodeDocs"
            </h1>
        </div>
    }
}

/// Native vs browser-demo indicator.
#[component]
fn ModeBadge() -> impl IntoView {
    let native = is_tauri();
    view! {
        <div class="flex items-center gap-2 px-3 py-1 bg-base-100 dark:bg-base-800 rounded-full \
                    border border-base-200 dark:border-base-700 w-fit">
            <span class=move || format!(
                "w-2 h-2 rounded-full {}",
                if native { "bg-base-900" } else { "bg-brand-orange" }
            )></span>
            <span class="text-[10px] font-bold text-base-500 dark:text-base-400 uppercase \
                         tracking-tighter">
                {if native { "Escritorio (Nativo)" } else { "Web (Demo Mode)" }}
            </span>
        </div>
    }
}

#[component]
fn TabSwitch(
    active_tab: ReadSignal<SidebarTab>,
    set_active_tab: WriteSignal<SidebarTab>,
) -> impl IntoView {
    let tabs = [
        (SidebarTab::Files, "Archivos"),
        (SidebarTab::Outline, "Contenido"),
    ];

    view! {
        <div class="flex flex-col gap-2">
            <div class="flex gap-1 bg-base-100 dark:bg-base-800/50 rounded-md p-0.5">
                {tabs.into_iter().map(|(tab, label)| {
                    view! {
                        <button
                            class=move || format!(
                                "flex-1 text-[10px] font-bold uppercase tracking-wider py-1.5 \
                                 rounded transition-colors {}",
                                if active_tab.get() == tab {
                                    "bg-base-50 dark:bg-base-700 text-base-900 dark:text-base-50 shadow-sm"
                                } else {
                                    "text-base-500 dark:text-base-400 hover:text-base-700 dark:hover:text-base-300"
                                }
                            )
                            on:click=move |_| set_active_tab.set(tab)
                        >
                            {label}
                        </button>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

/// Pick a project folder, then load its file tree and start watching.
#[component]
fn OpenFolderButton(state: EditorState) -> impl IntoView {
    let on_click = move |_: leptos::ev::MouseEvent| {
        spawn_local(async move {
            if !is_tauri() {
                load_demo_folder(state);
                return;
            }
            match tauri_bridge::open_project_folder().await {
                Ok(path) => {
                    state.path.set(Some(path));
                    actions::refresh_files(state);
                    watch_workspace(state);
                }
                Err(err) => {
                    // The user closing the picker is not an error worth showing.
                    if !err.contains("cancelo") && !err.contains("cancel") {
                        leptos::logging::error!("No se pudo abrir la carpeta: {err}");
                        state.notify(format!("No se pudo abrir la carpeta: {err}"));
                    }
                }
            }
        });
    };

    view! {
        <button
            class="px-4 py-2 bg-base-900 hover:bg-base-700 dark:bg-base-50 dark:hover:bg-base-200 \
                   text-base-50 dark:text-base-900 rounded-md text-sm font-medium transition-all \
                   shadow-sm active:scale-95"
            on:click=on_click
        >
            "Abrir carpeta del proyecto"
        </button>
    }
}

/// Populate the tree with demo data in the browser build.
fn load_demo_folder(state: EditorState) {
    const DEMO_ROOT: &str = "C:\\Demo\\Documents";
    state.path.set(Some(DEMO_ROOT.to_string()));
    state.files.set(vec![
        FileEntry::file("Bienvenido.md", format!("{DEMO_ROOT}\\Bienvenido.md")),
        FileEntry::file("Guía_Rápida.md", format!("{DEMO_ROOT}\\Guía_Rápida.md")),
        FileEntry::dir(
            "Proyectos",
            format!("{DEMO_ROOT}\\Proyectos"),
            vec![FileEntry::file(
                "Demo.md",
                format!("{DEMO_ROOT}\\Proyectos\\Demo.md"),
            )],
        ),
    ]);
}

/// Recursive markdown file tree.
#[component]
fn FileTree(
    items: RwSignal<Vec<FileEntry>>,
    state: EditorState,
    on_delete: Callback<String>,
    on_rename: Callback<String>,
) -> impl IntoView {
    view! {
        <ul class="space-y-1">
            {move || {
                items
                    .get()
                    .into_iter()
                    .map(|item| view! {
                        <TreeItem item state on_delete on_rename />
                    })
                    .collect_view()
            }}
        </ul>
    }
}

/// One row of the file tree, plus its children when expanded.
#[component]
fn TreeItem(
    item: FileEntry,
    state: EditorState,
    on_delete: Callback<String>,
    on_rename: Callback<String>,
) -> impl IntoView {
    let (is_expanded, set_is_expanded) = signal(item.is_dir);

    let is_dir = item.is_dir;
    // Handlers clone from the captured values instead of moving them, which
    // keeps the closures `Fn` so they survive re-renders.
    let toggle_state = state;
    let toggle_path = item.path.clone();
    let on_toggle = move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        if is_dir {
            set_is_expanded.update(|open| *open = !*open);
        } else {
            actions::open_file(toggle_state, toggle_path.clone());
        }
    };

    let selected = state.selected_file;
    let row_path = item.path.clone();
    // Separate owned copies: two `move` closures cannot both capture the same
    // binding, and `Callback::new` requires `Fn`.
    let delete_path = item.path.clone();
    let rename_path = item.path.clone();
    let delete_cb = on_delete;
    let rename_cb = on_rename;

    // Built outside the `view!` macro: a `Callback` is `Fn`, and defining it
    // here keeps the captured path clones owned by one closure each.
    let on_delete_click = Callback::new(move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        delete_cb.run(delete_path.clone());
    });
    let on_rename_click = Callback::new(move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        rename_cb.run(rename_path.clone());
    });

    let children = item.children.clone();

    view! {
        <li class="select-none group">
            <div
                class="flex items-center gap-2 py-1 px-2 rounded-md cursor-pointer \
                       transition-colors hover:bg-base-200 dark:hover:bg-base-800 text-sm"
                on:click=on_toggle
            >
                <span class="w-4 flex justify-center text-base-400">
                    {move || if is_dir {
                        if is_expanded.get() { "▾" } else { "▸" }
                    } else {
                        ""
                    }}
                </span>
                <TreeIcon is_dir />
                <span
                    class="truncate text-base-700 dark:text-base-300 flex-1"
                    class:font-semibold=move || {
                        selected.get().as_deref() == Some(row_path.as_str())
                    }
                >
                    {item.name.clone()}
                </span>
                <Show when=move || !is_dir && is_tauri()>
                    <IconButton
                        title="Eliminar"
                        on_click=on_delete_click
                        path="M3 6h18M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"
                    />
                    <IconButton
                        title="Renombrar"
                        on_click=on_rename_click
                        path="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"
                    />
                </Show>
            </div>
            {move || {
                if is_expanded.get() && is_dir {
                    view! {
                        <ul class="ml-4 border-l border-base-200 dark:border-base-800 mt-1 space-y-0.5">
                            {children
                                .clone()
                                .into_iter()
                                .map(|child| view! {
                                    <TreeItem item=child state on_delete on_rename />
                                })
                                .collect_view()}
                        </ul>
                    }
                    .into_any()
                } else {
                    ().into_any()
                }
            }}
        </li>
    }
}

/// Folder / document indicator.
///
/// Inline SVG rather than the emoji the tree used before: emoji render
/// differently per platform and looked inconsistent next to the SVG icons.
#[component]
fn TreeIcon(is_dir: bool) -> impl IntoView {
    view! {
        <span class="flex-shrink-0 text-base-400">
            {if is_dir {
                // Folder
                view! {
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"
                        fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
                        stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
                }
                    .into_any()
            } else {
                // Document
                view! {
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"
                        fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
                        stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                        <polyline points="14 2 14 8 20 8"/></svg>
                }
                    .into_any()
            }}
        </span>
    }
}

/// Small icon-only button revealed on row hover.
#[component]
fn IconButton(
    title: &'static str,
    path: &'static str,
    on_click: Callback<leptos::ev::MouseEvent>,
) -> impl IntoView {
    view! {
        <button
            class="opacity-0 group-hover:opacity-100 focus:opacity-100 p-1 text-base-400 \
                   hover:text-brand-orange transition-all"
            title=title
            aria-label=title
            on:click=move |ev| on_click.run(ev)
        >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24"
                fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
                stroke-linejoin="round"><path d=path /></svg>
        </button>
    }
}
