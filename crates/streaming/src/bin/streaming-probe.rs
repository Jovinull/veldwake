use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use veldwake_streaming::{
    CacheConfig, CacheMetrics, ChunkCache, ChunkSource, DiagnosticChunkSource, PayloadEncoding,
    StreamingConfig, StreamingRuntime, TerrainChunkSource, TimingStat,
};
use veldwake_voxel::ChunkCoord;

/// One streaming input, with the centre its measurements are taken around.
///
/// The diagnostic corridor and the procedural region are different worlds, so a
/// cache measurement of one says nothing about the other. Both are measured
/// here, under the same phases, so the M3D conclusions can be re-tested against
/// content that costs something to generate.
struct SourceSpec {
    name: &'static str,
    make: fn() -> Box<dyn ChunkSource>,
    centre: ChunkCoord,
}

/// The finite M3 corridor, centred where every M3 measurement was taken.
const DIAGNOSTIC: SourceSpec = SourceSpec {
    name: "diagnostic",
    make: || Box::new(DiagnosticChunkSource),
    centre: ChunkCoord::new(0, 0, 0),
};

/// The M4 golden region, centred on its middle layer so the demand sphere sits
/// inside the region rather than half outside it.
const TERRAIN: SourceSpec = SourceSpec {
    name: "terrain",
    make: || Box::new(TerrainChunkSource::golden()),
    centre: ChunkCoord::new(0, 1, 0),
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let selection: Vec<String> = std::env::args().skip(1).collect();
    let wanted = |name: &str| selection.is_empty() || selection.iter().any(|value| value == name);

    if wanted("m3b-default") {
        run("m3b-default", StreamingConfig::default(), &DIAGNOSTIC)?;
    }
    if wanted("m3c-banded") {
        run("m3c-banded", StreamingConfig::m3c_diagnostic(), &DIAGNOSTIC)?;
    }
    if wanted("m4-terrain-near") {
        run("m4-terrain-near", StreamingConfig::default(), &TERRAIN)?;
    }
    if wanted("m4-golden") {
        run("m4-golden", StreamingConfig::m4_golden(), &TERRAIN)?;
    }
    if wanted("m4-golden-banded") {
        run(
            "m4-golden-banded",
            StreamingConfig::m4_golden_banded(),
            &TERRAIN,
        )?;
    }
    for spec in [&DIAGNOSTIC, &TERRAIN] {
        if !wanted(spec.name) {
            continue;
        }
        for encoding in [PayloadEncoding::Raw, PayloadEncoding::Rle] {
            cache_experiment(spec, encoding)?;
        }
    }
    Ok(())
}

/// The M3D disk-cache experiment: one identical settle run with the cache off,
/// then cold, then warm twice, so cold-versus-warm and warm stability are both
/// visible. Entries live under the system temporary directory; the repository
/// working tree is never written to.
fn cache_experiment(
    spec: &SourceSpec,
    encoding: PayloadEncoding,
) -> Result<(), Box<dyn std::error::Error>> {
    let label = format!("{}-{}", spec.name, encoding.name());
    let config = StreamingConfig::default();
    let centre = spec.centre;
    let cache_config = CacheConfig::new(cache_root(&label)).with_payload_encoding(encoding);
    let fingerprint = (spec.make)().fingerprint();

    // Phase 1: no cache at all. The baseline every other phase is compared to.
    let started = Instant::now();
    let mut runtime = StreamingRuntime::with_source(config, centre, (spec.make)())?;
    settle(&mut runtime, started)?;
    cache_report(&label, "off", &runtime, started, None);

    // Phase 2: cold. The cache is emptied first so the run is reproducible.
    let (cache, _) = ChunkCache::open(&cache_config, fingerprint)?;
    cache.clear()?;
    for phase in ["cold", "warm", "warm-again"] {
        let (cache, open_report) = ChunkCache::open(&cache_config, fingerprint)?;
        if open_report.temporaries_removed > 0 {
            println!(
                "cache encoding={label} phase={phase} swept_temporaries={}",
                open_report.temporaries_removed
            );
        }
        let started = Instant::now();
        let mut runtime =
            StreamingRuntime::with_source_and_cache(config, centre, (spec.make)(), cache)?;
        settle(&mut runtime, started)?;
        let footprint = ChunkCache::open(&cache_config, fingerprint)?
            .0
            .footprint()?;
        cache_report(&label, phase, &runtime, started, Some(footprint));
    }
    Ok(())
}

fn cache_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("veldwake-probe-cache-{label}"))
}

fn cache_report(
    encoding: &str,
    phase: &str,
    runtime: &StreamingRuntime,
    started: Instant,
    footprint: Option<veldwake_streaming::CacheFootprint>,
) {
    let metrics = runtime.metrics();
    let cache: &CacheMetrics = &metrics.cache;
    let (entries, disk_bytes) = footprint.map_or((0, 0), |f| (f.entries, f.bytes));
    println!(
        "cache encoding={encoding} phase={phase} time_to_idle_us={} source_loads={} tracked={} resident={} resident_bytes={} cpu_mesh_bytes={}",
        started.elapsed().as_micros(),
        metrics.load_jobs_dispatched,
        runtime.tracked_count(),
        runtime.resident_payload_count(),
        runtime.logical_resident_bytes(),
        runtime.summary().cpu_mesh_bytes
    );
    println!(
        "cache encoding={encoding} phase={phase} lookups={} hits_present={} hits_absent={} misses={} stale={} corrupt={} read_failures={} rejected_removed={} rejected_delete_failures={} source_fallbacks={}",
        cache.lookups,
        cache.hits_present,
        cache.hits_absent,
        cache.misses,
        cache.stale_rejects,
        cache.corrupt_rejects,
        cache.read_failures,
        cache.rejected_entries_removed,
        cache.rejected_entry_delete_failures,
        cache.source_fallbacks
    );
    println!(
        "cache encoding={encoding} phase={phase} write_attempts={} writes={} writes_skipped={} write_failures={} bytes_read={} bytes_written={} disk_entries={entries} disk_bytes={disk_bytes}",
        cache.write_attempts,
        cache.writes,
        cache.writes_skipped,
        cache.write_failures,
        cache.bytes_read,
        cache.bytes_written
    );
    println!(
        "cache encoding={encoding} phase={phase} encode={} decode={} stale_loads={} stale_meshes={} hard_cap_blocks={}",
        timing(&cache.encode),
        timing(&cache.decode),
        metrics.stale_load_results,
        metrics.stale_mesh_results,
        metrics.hard_cap_blocks
    );
}

fn run(
    profile: &str,
    config: StreamingConfig,
    spec: &SourceSpec,
) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let centre = spec.centre;
    let stepped = ChunkCoord::new(centre.x + 1, centre.y, centre.z);
    let mut runtime = StreamingRuntime::with_source(config, centre, (spec.make)())?;
    settle(&mut runtime, started)?;
    report(profile, "settled", &runtime, started);

    // One boundary crossing and its return: the band must not ping-pong.
    let swaps_before = runtime.metrics().lod_swaps;
    runtime.set_demand_center(stepped)?;
    settle(&mut runtime, started)?;
    runtime.set_demand_center(centre)?;
    settle(&mut runtime, started)?;
    let first_cycle = runtime.metrics().lod_swaps - swaps_before;
    runtime.set_demand_center(stepped)?;
    settle(&mut runtime, started)?;
    runtime.set_demand_center(centre)?;
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
    println!(
        "profile={profile} phase={phase} snapshot_build={} lod1_derivation={} worker_mesh_lod0={} worker_mesh_lod1={}",
        timing(&metrics.snapshot_build),
        timing(&metrics.lod1_derivation),
        timing(&metrics.worker_mesh_lod0),
        timing(&metrics.worker_mesh_lod1)
    );
}

fn timing(stat: &TimingStat) -> String {
    format!(
        "count:{}/total_us:{}/mean_us:{}/max_us:{}",
        stat.count,
        stat.total_us,
        stat.mean_us(),
        stat.max_us
    )
}
