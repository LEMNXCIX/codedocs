use leptos::ev::Event;
use leptos::prelude::*;
use leptos::web_sys::HtmlInputElement;
use wasm_bindgen::JsCast;

#[component]
pub fn App() -> impl IntoView {
    // Definimos un Signal (Estado Reactivo)
    // Concepto Simple: Una variable que avisa cuando cambia.
    // Concepto Avanzado: Un getter/setter reactivo con suscripciones automáticas.
    let (name, set_name) = signal("Mundo".to_string());

    view! {
        <main class="flex flex-col items-center justify-center h-screen bg-slate-900 text-white">
            <h1 class="text-5xl font-extrabold mb-4 bg-gradient-to-r from-blue-400 to-purple-500 bg-clip-text text-transparent">
                "¡Hola, " {move || name.get()} "!"
            </h1>
            <input
                type="text"
                class="p-2 border rounded bg-slate-800 text-white border-blue-500 outline-none"
                on:input=move |ev: Event| {
                    let target: Option<HtmlInputElement> = ev.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
                    if let Some(input) = target {
                        set_name.set(input.value());
                    }
                }
                prop:value={name}
            />
        </main>
    }
}
