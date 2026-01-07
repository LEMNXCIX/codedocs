use leptos::children::Children;
use leptos::prelude::ClassAttribute;
use leptos::prelude::ElementChild;
use leptos::*;

#[component]
pub fn Layout(children: Children) -> impl IntoView {
    view! {
        <div class="flex flex-row h-screen bg-slate-50 dark:bg-brand-dark text-slate-900 dark:text-slate-200 font-sans transition-colors duration-300">
            <aside class="w-64 border-r border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900/50 p-4 overflow-y-auto z-10">
                <h2 class="text-xs font-bold uppercase tracking-widest text-brand-blue mb-6">
                    Explorador
                </h2>
                <div class="space-y-2">
                    <div class="p-2 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer transition-colors text-sm font-medium text-slate-700 dark:text-slate-300 hover:text-brand-orange">
                        Documentos
                    </div>
                </div>
            </aside>
            <main class="flex-1 p-8 overflow-y-auto bg-slate-50 dark:bg-brand-dark">
                {children()}
            </main>
        </div>
    }
}
