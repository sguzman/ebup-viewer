mod tts_worker;

use eframe::egui;

#[derive(Default)]
struct LanternLeafApp {}

impl eframe::App for LanternLeafApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LanternLeaf (egui migration)");
            ui.label("The native egui migration is underway.");
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    if tts_worker::maybe_run_worker() {
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    eframe::run_native(
        "LanternLeaf",
        options,
        Box::new(|_cc| Ok(Box::new(LanternLeafApp::default()))),
    )
}
