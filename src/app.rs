use crate::components::layout::Layout;
use leptos::prelude::*;
use leptos::logging::log;

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
             <div class="p-6 bg-white dark:bg-slate-900/50 border border-slate-800 dark:border-slate-800 rounded-xl shadow-2xl">
                <p class="mb-4">"Clicks acumulados: " <span class="text-brand-blue font-mono">{count} - {base}</span></p>
                <button 
                    on:click={move |_| {
                        set_count.update(|n| *n += 1);
                        set_base.update(|b| *b = validar_contador(count.get()));
                    }}
                    class="px-4 py-2 bg-blue-600 hover:bg-blue-500 rounded font-medium transition-all text-brand-orange hover:text-brand-orange/80"
                >
                    "Incrementar Contador"
                </button>
            </div>
        </div>
       </Layout>
    }
}
