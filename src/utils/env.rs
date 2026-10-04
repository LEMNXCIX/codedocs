use wasm_bindgen::prelude::*;

/// `true` when running inside the Tauri webview rather than a plain browser.
///
/// The compiled-in check is an inline JS expression so there is no JS file to
/// keep in sync, and it is cheap enough to call per use.
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        js_name = "(function() { return typeof window !== 'undefined' && typeof window.__TAURI__ !== 'undefined'; })"
    )]
    fn check_is_tauri() -> bool;
}

#[cfg(target_arch = "wasm32")]
pub fn is_tauri() -> bool {
    check_is_tauri()
}

/// Off-target (tests, native builds): pretend to be the desktop app, so the
/// browser-only demo path does not shadow logic under test.
#[cfg(not(target_arch = "wasm32"))]
pub fn is_tauri() -> bool {
    true
}
