#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

/// Trunk entrypoint: delegates to `lanternleaf-egui`'s WASM thin client.
#[wasm_bindgen]
pub fn start(canvas_id: &str) -> Result<(), JsValue> {
    lanternleaf_egui::start(canvas_id)
}

