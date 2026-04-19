mod app;
mod constants;
mod effects;
mod helpers;
mod os;
mod pdf;
mod pdf_renderer;
mod pdf_subsystem;
mod pretty;
mod shell;

pub(crate) use constants::*;

fn main() {
    app::run();
}
