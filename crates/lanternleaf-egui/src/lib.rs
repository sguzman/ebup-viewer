#[cfg(not(target_arch = "wasm32"))]
pub mod shell;
#[cfg(not(target_arch = "wasm32"))]
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod constants;
#[cfg(not(target_arch = "wasm32"))]
pub mod effects;
#[cfg(not(target_arch = "wasm32"))]
pub mod helpers;
#[cfg(not(target_arch = "wasm32"))]
pub mod os;
#[cfg(not(target_arch = "wasm32"))]
pub mod pdf;
#[cfg(not(target_arch = "wasm32"))]
pub mod pdf_renderer;
#[cfg(not(target_arch = "wasm32"))]
pub mod pdf_subsystem;
#[cfg(not(target_arch = "wasm32"))]
pub mod pretty;

#[cfg(target_arch = "wasm32")]
pub mod web_client;

#[cfg(target_arch = "wasm32")]
pub fn start(canvas_id: &str) -> Result<(), wasm_bindgen::JsValue> {
    web_client::run_wasm(canvas_id)
}
