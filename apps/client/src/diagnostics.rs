use std::{env, error::Error};

use tracing_subscriber::EnvFilter;

pub fn init() -> Result<(), Box<dyn Error + Send + Sync>> {
    let filter = if env::var_os("RUST_LOG").is_some() {
        EnvFilter::try_from_default_env()?
    } else {
        EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init()?;

    Ok(())
}
