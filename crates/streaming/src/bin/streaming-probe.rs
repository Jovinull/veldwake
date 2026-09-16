use std::time::{Duration, Instant};

use veldwake_streaming::{StreamingConfig, StreamingRuntime};
use veldwake_voxel::ChunkCoord;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let mut runtime = StreamingRuntime::new(StreamingConfig::default(), ChunkCoord::default())?;
    let deadline = started + Duration::from_secs(10);
    while !runtime.is_idle() && Instant::now() < deadline {
        runtime.poll()?;
        std::thread::yield_now();
    }
    if !runtime.is_idle() {
        return Err(
            std::io::Error::other("streaming probe did not become idle in 10 seconds").into(),
        );
    }

    let metrics = runtime.metrics();
    println!("center=(0,0,0)");
    println!(
        "demand render={} dependency={} retention={}",
        runtime.demand().render.len(),
        runtime.demand().dependency.len(),
        runtime.demand().retention.len()
    );
    println!(
        "tracked={} resident={} resident_bytes={} ready_meshes={}",
        runtime.tracked_count(),
        runtime.resident_payload_count(),
        runtime.logical_resident_bytes(),
        runtime.ready_mesh_count()
    );
    println!(
        "loads={} meshes={} stale_loads={} stale_meshes={}",
        metrics.load_jobs_dispatched,
        metrics.mesh_jobs_dispatched,
        metrics.stale_load_results,
        metrics.stale_mesh_results
    );
    println!(
        "snapshot_bytes_dispatched={} elapsed_us={}",
        metrics.snapshot_bytes_dispatched,
        started.elapsed().as_micros()
    );
    Ok(())
}
