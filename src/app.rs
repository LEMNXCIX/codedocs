use crate::components::layout::Layout;
use leptos::logging::log;
use leptos::prelude::*;

pub fn validar_contador(count: i32) -> String {
    if count % 2 == 0 {
        log!("El contador es par");
        return "Par".to_string();
    } else {
        log!("El contador es impar");
        return "Impar".to_string();
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Definimos un Signal (Estado Reactivo)
    // Concepto Simple: Una variable que avisa cuando cambia.
    // Concepto Avanzado: Un getter/setter reactivo con suscripciones automáticas.
    let (_name, _set_name) = signal("Mundo".to_string());
    let (count, set_count) = signal(0);
    let (base, set_base) = signal(validar_contador(count.get()));
    view! {
       <Layout>
        <div class="max-w-2xl mx-auto">
            <h1 class="text-3xl font-bold mb-4">"Panel de Control"</h1>
            <p class="text-slate-400 mb-8">
                "Bienvenido al editor CodeDocs. Aquí podrás gestionar tus documentos Markdown."
            </p>
             <div class="p-6 bg-white dark:bg-slate-800/50 border border-slate-200 dark:border-slate-700 rounded-lg shadow-sm">
                <p class="mb-6 text-slate-600 dark:text-slate-400 font-medium">
                    "Clicks acumulados: "
                    <span class="text-slate-900 dark:text-white font-mono font-bold text-lg ml-1">
                        {count} <span class="text-xs text-slate-400 font-normal ml-2">"("{base}")"</span>
                    </span>
                </p>
                <button
                    on:click={move |_| {
                        set_count.update(|n| *n += 1);
                        set_base.update(|b| *b = validar_contador(count.get()));
                    }}
                    class="px-4 py-2 bg-slate-900 hover:bg-slate-800 dark:bg-white dark:text-slate-900 dark:hover:bg-slate-200 text-white rounded-md text-sm font-medium transition-colors"
                >
                    "Incrementar Contador"
                </button>
            </div>
        </div>
       </Layout>
    }
}
