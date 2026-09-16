use std::time::{Duration, Instant};

use veldwake_streaming::{StreamingConfig, StreamingRuntime};
use veldwake_voxel::ChunkCoord;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run("m3b-default", StreamingConfig::default())?;
    run("m3c-banded", StreamingConfig::m3c_diagnostic())?;
    Ok(())
}

fn run(profile: &str, config: StreamingConfig) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let mut runtime = StreamingRuntime::new(config, ChunkCoord::default())?;
    settle(&mut runtime, started)?;
    report(profile, "settled", &runtime, started);

    // One boundary crossing and its return: the band must not ping-pong.
    let swaps_before = runtime.metrics().lod_swaps;
    runtime.set_demand_center(ChunkCoord::new(1, 0, 0))?;
    settle(&mut runtime, started)?;
    runtime.set_demand_center(ChunkCoord::new(0, 0, 0))?;
    settle(&mut runtime, started)?;
    let first_cycle = runtime.metrics().lod_swaps - swaps_before;
    runtime.set_demand_center(ChunkCoord::new(1, 0, 0))?;
    settle(&mut runtime, started)?;
    runtime.set_demand_center(ChunkCoord::new(0, 0, 0))?;
    settle(&mut runtime, started)?;
    let second_cycle = runtime.metrics().lod_swaps - swaps_before - first_cycle;
    println!(
        "profile={profile} oscillation first_cycle_swaps={first_cycle} second_cycle_swaps={second_cycle}"
    );
    report(profile, "after-oscillation", &runtime, started);
    Ok(())
}

fn settle(
    runtime: &mut StreamingRuntime,
    started: Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = started + Duration::from_secs(60);
    while !runtime.is_idle() && Instant::now() < deadline {
        runtime.poll()?;
        std::thread::yield_now();
    }
    if !runtime.is_idle() {
        return Err(
            std::io::Error::other("streaming probe did not become idle in 60 seconds").into(),
        );
    }
    Ok(())
}

fn report(profile: &str, phase: &str, runtime: &StreamingRuntime, started: Instant) {
    let metrics = runtime.metrics();
    let summary = runtime.summary();
    println!(
        "profile={profile} phase={phase} center={:?} render={} dependency={} retention={}",
        runtime.center(),
        runtime.demand().render.len(),
        runtime.demand().dependency.len(),
        runtime.demand().retention.len()
    );
    println!(
        "profile={profile} phase={phase} tracked={} resident={} resident_bytes={} known_absent={} lod0_desired={} lod1_desired={} lod0_ready={} lod1_ready={} cpu_mesh_bytes={}",
        runtime.tracked_count(),
        runtime.resident_payload_count(),
        runtime.logical_resident_bytes(),
        summary.known_absent,
        summary.lod0_desired,
        summary.lod1_desired,
        summary.lod0_ready,
        summary.lod1_ready,
        summary.cpu_mesh_bytes
    );
    println!(
        "profile={profile} phase={phase} loads={} meshes={} stale_loads={} stale_meshes={} stale_lod={} lod_swaps={} hard_cap_blocks={} snapshot_bytes_dispatched={} elapsed_us={}",
        metrics.load_jobs_dispatched,
        metrics.mesh_jobs_dispatched,
        metrics.stale_load_results,
        metrics.stale_mesh_results,
        metrics.stale_lod_results,
        metrics.lod_swaps,
        metrics.hard_cap_blocks,
        metrics.snapshot_bytes_dispatched,
        started.elapsed().as_micros()
    );
}
