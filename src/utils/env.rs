use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        js_name = "(function() { return typeof window !== 'undefined' && typeof window.__TAURI__ !== 'undefined'; })"
    )]
    fn check_is_tauri() -> bool;
}

pub fn is_tauri() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        check_is_tauri()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        true
    }
}
