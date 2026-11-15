//! WebAssembly entry point: starts the app in `<canvas id="app">`.
//! Built by `web/build.sh`.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        match katasymbol_app::run_web("app").await {
            // Running: remove the loading notice.
            Ok(()) => {
                let _ = js_sys_eval("document.getElementById('loading')?.remove()");
            }
            // E.g. no WebGL: show why on the page.
            Err(e) => {
                let msg = format!("{e:?}").replace('\\', "").replace('`', "'");
                web_sys_log(&format!("Katasymbol konnte nicht starten: {msg}"));
                let _ = js_sys_eval(&format!(
                    "(()=>{{const p=document.getElementById('problem');p.textContent=`Die App konnte nicht starten: {msg}`;p.style.display='block';}})()"
                ));
            }
        }
    });
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn web_sys_log(s: &str);
    #[wasm_bindgen(js_name = eval, catch)]
    fn js_sys_eval(code: &str) -> Result<JsValue, JsValue>;
}
