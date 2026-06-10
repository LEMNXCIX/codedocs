use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = __codedocs_createEditor)]
    pub fn cm_create_editor(
        parent: &web_sys::Element,
        initial_content: &str,
        is_dark: bool,
    ) -> JsValue;

    #[wasm_bindgen(js_name = __codedocs_getContent)]
    pub fn cm_get_content() -> String;

    #[wasm_bindgen(js_name = __codedocs_setContent)]
    pub fn cm_set_content(content: &str);

    #[wasm_bindgen(js_name = __codedocs_setTheme)]
    pub fn cm_set_theme(is_dark: bool);

    #[wasm_bindgen(js_name = __codedocs_setOnChange)]
    pub fn cm_set_on_change(callback: &js_sys::Function);

    #[wasm_bindgen(js_name = __codedocs_focus)]
    pub fn cm_focus();

    #[wasm_bindgen(js_name = __codedocs_destroyEditor)]
    pub fn cm_destroy();

    #[wasm_bindgen(js_name = __codedocs_wrap_selection)]
    pub fn cm_wrap_selection(wrapper: &str);

    #[wasm_bindgen(js_name = __codedocs_insert_link)]
    pub fn cm_insert_link();
}

#[component]
pub fn CodeMirrorEditor(
    content: ReadSignal<String>,
    set_content: WriteSignal<String>,
    is_dark: ReadSignal<bool>,
    on_save: Callback<()>,
) -> impl IntoView {
    let container_ref = NodeRef::<leptos::html::Div>::new();
    let is_initialized = RwSignal::new(false);
    let last_known_content = RwSignal::new(String::new());

    Effect::new(move |_| {
        if let Some(el) = container_ref.get() {
            if !is_initialized.get() {
                let initial = content.get();
                let dark = is_dark.get();
                last_known_content.set(initial.clone());

                let set_c = set_content;
                let lkc = last_known_content;
                let closure = Closure::<dyn Fn(String)>::new(move |new_text: String| {
                    lkc.set(new_text.clone());
                    set_c.set(new_text);
                });
                cm_set_on_change(closure.as_ref().unchecked_ref());
                closure.forget();

                let save_cb = on_save;
                let save_closure = Closure::<dyn Fn()>::new(move || {
                    save_cb.run(());
                });
                js_sys::Reflect::set(
                    &js_sys::global(),
                    &JsValue::from_str("__codedocs_save"),
                    save_closure.as_ref(),
                )
                .ok();
                save_closure.forget();

                cm_create_editor(&el, &initial, dark);
                is_initialized.set(true);
            }
        }
    });

    Effect::new(move |_| {
        if is_initialized.get() {
            let new_content = content.get();
            let last = last_known_content.get();
            if new_content != last {
                last_known_content.set(new_content.clone());
                cm_set_content(&new_content);
            }
        }
    });

    Effect::new(move |_| {
        if is_initialized.get() {
            cm_set_theme(is_dark.get());
        }
    });

    on_cleanup(|| {
        cm_destroy();
    });

    view! {
        <div
            node_ref=container_ref
            class="w-full h-full overflow-hidden codemirror-container"
        />
    }
}
