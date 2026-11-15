//! WebAssembly entry point: starts the app in `<canvas id="app">`.
//! Built by `web/build.sh`.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        if let Err(e) = katasymbol_app::run_web("app").await {
            web_sys_log(&format!("Katasymbol konnte nicht starten: {e:?}"));
        }
        // Hide the loading notice once the app runs (or failed).
        let _ = js_sys_eval("document.getElementById('loading')?.remove()");
    });
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn web_sys_log(s: &str);
    #[wasm_bindgen(js_name = eval, catch)]
    fn js_sys_eval(code: &str) -> Result<JsValue, JsValue>;
}
