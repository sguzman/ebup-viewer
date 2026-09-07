#[cfg(not(target_arch = "wasm32"))]
mod app;
#[cfg(not(target_arch = "wasm32"))]
mod constants;
#[cfg(not(target_arch = "wasm32"))]
mod effects;
#[cfg(not(target_arch = "wasm32"))]
mod helpers;
#[cfg(not(target_arch = "wasm32"))]
mod os;
#[cfg(not(target_arch = "wasm32"))]
mod pdf;
#[cfg(not(target_arch = "wasm32"))]
mod pdf_renderer;
#[cfg(not(target_arch = "wasm32"))]
mod pdf_subsystem;
#[cfg(not(target_arch = "wasm32"))]
mod pretty;
#[cfg(not(target_arch = "wasm32"))]
mod shell;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use constants::*;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(err) = app::run() {
        eprintln!("LanternLeaf native startup failed: {err}");
        std::process::exit(1);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
