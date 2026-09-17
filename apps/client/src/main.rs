mod app;
mod camera;
mod debug;
mod diagnostics;
mod input;
mod renderer;
mod streaming;

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
