pub mod shell;
pub mod app;
pub mod constants;
pub mod effects;
pub mod helpers;
pub mod os;
pub mod pdf;
pub mod pdf_renderer;
pub mod pdf_subsystem;
pub mod pretty;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn start(canvas_id: &str) -> Result<(), JsValue> {
    app::run_wasm(canvas_id)
}
