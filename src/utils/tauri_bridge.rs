use js_sys;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    pub async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    pub async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;
}

pub fn make_args() -> js_sys::Object {
    js_sys::Object::new()
}

pub fn set_arg(args: &js_sys::Object, key: &str, value: JsValue) {
    let _ = js_sys::Reflect::set(args, &key.into(), &value);
}

pub fn args_with(key: &str, value: &str) -> JsValue {
    let args = make_args();
    set_arg(&args, key, JsValue::from(value.to_string()));
    args.into()
}
