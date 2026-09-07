mod tts_worker;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if tts_worker::maybe_run_worker() {
        return Ok(());
    }

    #[cfg(not(target_arch = "wasm32"))]
    lanternleaf_egui::app::run()?;
    
    Ok(())
}
