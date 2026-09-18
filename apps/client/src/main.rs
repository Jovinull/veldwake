mod app;
mod arena;
mod camera;
mod character;
mod debug;
mod diagnostics;
mod encounter;
mod input;
mod lighting;
mod readout;
mod renderer;
mod streaming;
mod vfx;
mod world;

use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(error) = diagnostics::init() {
        eprintln!("Veldwake diagnostics failed: {error}");
        return ExitCode::FAILURE;
    }
    if let Err(error) = app::run() {
        eprintln!("Veldwake client failed: {error}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
