//! Legacy binary shim.
//!
//! The desktop GUI now runs through the egui application crate.
//! This binary is retained for two reasons:
//! - It hosts the `--tts-worker` subprocess mode used by the Piper worker pool.
//! - It provides a clear migration message when launched directly.

mod tts_worker;

fn main() {
    if tts_worker::maybe_run_worker() {
        return;
    }
    eprintln!("The legacy iced desktop UI has been decommissioned.");
    eprintln!("Run `cargo run -p lanternleaf-egui` to launch LanternLeaf.");
}
