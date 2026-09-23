mod app;
mod arena;
mod audio;
mod camera;
mod character;
mod debug;
mod diagnostics;
mod encounter;
mod initiative;
mod input;
// Mostly evidence machinery: the binary draws landmarks through the ordinary
// chunk path and only asks this module for a named capture pose, while the
// tests ask it whether what is drawn can actually be seen.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the framing oracle is evidence; only the poses are called"
    )
)]
mod landmark;
mod lighting;
mod readout;
mod renderer;
mod streaming;
mod synth;
mod traversal;
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
