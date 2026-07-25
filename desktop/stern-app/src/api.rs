use wasm_bindgen::prelude::*;

#[allow(dead_code)]
#[allow(clippy::future_not_send)]
pub async fn invoke(cmd: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    let args_js = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;

    let window = web_sys::window().ok_or("no window")?;
    let tauri: JsValue = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"))
        .map_err(|e| format!("no __TAURI__: {e:?}"))?;
    let invoke_fn = js_sys::Reflect::get(&tauri, &JsValue::from_str("invoke"))
        .map_err(|e| format!("no invoke: {e:?}"))?;
    let invoke_fn: js_sys::Function = invoke_fn
        .dyn_into()
        .map_err(|_| "invoke is not a function")?;

    let result = invoke_fn
        .call2(&tauri, &JsValue::from_str(cmd), &args_js)
        .map_err(|e| format!("invoke error: {e:?}"))?;

    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&result))
        .await
        .map_err(|e| format!("promise error: {e:?}"))?;

    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}
