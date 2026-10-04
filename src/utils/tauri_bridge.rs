use crate::types::FileEntry;
use js_sys::{Object, Promise, Reflect};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// Result of invoking a Tauri command, normalised for the UI.
///
/// The `Err` variant carries a message fit for display, so no call site has to
/// remember to unwrap a raw `JsValue`.
pub type CommandResult<T> = Result<T, String>;

/// Tauri v2's JS API.
///
/// `js_namespace` says only *where* to look; the property name defaults to the
/// Rust function name. Every binding therefore needs an explicit `js_name`,
/// because these Rust-side names carry a `raw_` prefix to avoid colliding with
/// this module's own wrappers. Without it the glue emits
/// `__TAURI__.core.raw_invoke`, which does not exist, so the call resolves to
/// `undefined` and the generated promise glue throws
/// "undefined is not an object (evaluating 'arg0.then')".
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        js_namespace = ["window", "__TAURI__", "core"],
        js_name = invoke,
        catch
    )]
    async fn raw_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen)]
    fn raw_listen(event: &str, handler: &js_sys::Function) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = unlisten)]
    fn raw_unlisten(event: &str, event_id: i32) -> JsValue;
}

/// Build the argument object for `invoke` from `(key, value)` pairs.
pub fn args<const N: usize>(pairs: [(&str, String); N]) -> JsValue {
    let obj = Object::new();
    for (key, value) in pairs {
        let _ = Reflect::set(&obj, &JsValue::from_str(key), &JsValue::from_str(&value));
    }
    obj.into()
}

/// Invoke a Tauri command and deserialize its result.
///
/// Fails early with a clear message outside Tauri, rather than surfacing an
/// opaque `JsValue` error later.
pub async fn invoke<T>(cmd: &str, args: JsValue) -> CommandResult<T>
where
    T: serde::de::DeserializeOwned,
{
    if !crate::utils::env::is_tauri() {
        return Err(format!(
            "'{cmd}' no está disponible fuera de la app de escritorio"
        ));
    }
    let value = raw_invoke(cmd, args)
        .await
        .map_err(|e| describe_error(cmd, &e))?;
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| format!("No se pudo interpretar la respuesta de '{cmd}': {e}"))
}

/// Subscribe to a Tauri event, resolving to the id [`unlisten`] needs.
///
/// Keeping the id lets the caller release the handler on cleanup; without it a
/// listener keeps firing for the lifetime of the webview, which is how the
/// previous watcher ended up reloading the open file after it had gone.
pub async fn listen(event: &str, handler: &js_sys::Function) -> Option<i32> {
    if !crate::utils::env::is_tauri() {
        return None;
    }
    // `__TAURI__.event.listen` returns a Promise resolving to the numeric id.
    // It must be awaited as-is; treating it as a callable and re-wrapping the
    // result in a Promise is what produced the "arg0.then" TypeError.
    let resolved = JsFuture::from(Promise::from(raw_listen(event, handler)))
        .await
        .ok()?;
    resolved.as_f64().map(|id| id as i32)
}

pub fn unlisten(event: &str, event_id: i32) {
    if crate::utils::env::is_tauri() {
        raw_unlisten(event, event_id);
    }
}

fn describe_error(cmd: &str, err: &JsValue) -> String {
    if let Some(message) = err.as_string() {
        return message;
    }
    // `catch` yields an `Error` object rather than a string for thrown errors.
    Reflect::get(err, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .unwrap_or_else(|| format!("fallo en '{cmd}'"))
}

// ---------------------------------------------------------------------------
// Typed command wrappers.
//
// One function per backend command, so argument names and deserialisation live
// in exactly one place. A rename on the Rust side becomes a compile error here
// instead of a silent runtime failure.
// ---------------------------------------------------------------------------

pub async fn open_project_folder() -> CommandResult<String> {
    invoke("open_project_folder", JsValue::NULL).await
}

pub async fn list_markdown_files(folder_path: &str) -> CommandResult<FileTree> {
    invoke(
        "list_markdown_files",
        args([("folderPath", folder_path.to_string())]),
    )
    .await
}

/// The result of walking a project folder.
///
/// The backend also sends a `truncated` flag when the walk hit its budget. It is
/// deliberately not modelled here: the tree is shown either way, so carrying the
/// flag would only be dead weight. Serde ignores unknown fields, so the IPC
/// contract can grow the flag back without touching this struct.
#[derive(Debug, Clone, Deserialize)]
pub struct FileTree {
    #[serde(default)]
    pub entries: Vec<FileEntry>,
}

pub async fn read_file(path: &str) -> CommandResult<String> {
    invoke("read_file", args([("pathStr", path.to_string())])).await
}

pub async fn save_file(path: &str, content: &str) -> CommandResult<()> {
    invoke(
        "save_file",
        args([
            ("pathStr", path.to_string()),
            ("content", content.to_string()),
        ]),
    )
    .await
}

pub async fn delete_file(path: &str) -> CommandResult<()> {
    invoke("delete_file", args([("pathStr", path.to_string())])).await
}

pub async fn rename_file(old_path: &str, new_name: &str) -> CommandResult<()> {
    invoke(
        "rename_file",
        args([
            ("oldPath", old_path.to_string()),
            ("newName", new_name.to_string()),
        ]),
    )
    .await
}

pub async fn create_file(folder_path: &str, name: &str) -> CommandResult<String> {
    invoke(
        "create_file",
        args([
            ("folderPath", folder_path.to_string()),
            ("name", name.to_string()),
        ]),
    )
    .await
}

pub async fn stop_watching() -> CommandResult<()> {
    invoke("stop_watching", JsValue::NULL).await
}

pub async fn watch_folder(folder_path: &str) -> CommandResult<()> {
    invoke(
        "watch_folder",
        args([("folderPath", folder_path.to_string())]),
    )
    .await
}

/// Extract the changed paths from a watcher event payload.
///
/// The backend emits a JSON array of strings. Parsing lives here so callers
/// only deal with `Vec<String>`, and an unexpected payload shape degrades to
/// `None` instead of panicking.
pub fn parse_fs_change(event: &JsValue) -> Vec<String> {
    let Ok(payload) = Reflect::get(event, &JsValue::from_str("payload")) else {
        return Vec::new();
    };
    if let Ok(list) = serde_wasm_bindgen::from_value::<Vec<String>>(payload.clone()) {
        return list;
    }
    // Tolerate a bare string payload.
    payload.as_string().into_iter().collect()
}
