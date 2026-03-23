mod app;
mod constants;
mod effects;
mod helpers;
mod pdf;
mod pdf_renderer;
mod pdf_subsystem;
mod shell;

pub(crate) use constants::*;

fn main() {
    app::run();
}
