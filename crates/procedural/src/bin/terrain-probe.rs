//! Headless inspection of the generated world.
//!
//! The generator has no GPU, no window, and no frame loop, so everything it
//! produces can be looked at from a terminal. This probe is how a person or an
//! agent answers "what is actually there?" without launching the client, and
//! how the generation measurements in the milestone document are taken.
//!
//! ```text
//! terrain-probe map [landform|zone|height|material]
//! terrain-probe column <x> <z>
//! terrain-probe chunk <x> <y> <z>
//! terrain-probe signature
//! terrain-probe vegetation
//! terrain-probe bench [chunks]
//! ```
//!
//! An optional `--seed <value>` selects a diagnostic world; without it the
//! probe reports on the golden slice.

use std::{env, process::ExitCode, time::Instant};

use veldwake_procedural::{
    TerrainGenerator, WorldSeed,
    generator::material_histogram,
    identity::TerrainConfig,
    region::{GOLDEN_POSES, GOLDEN_PROBES, SIGNATURE_CHUNKS, region_signature},
    terrain::SHORE_RISE,
};
use veldwake_voxel::{CHUNK_EDGE, ChunkCoord, fingerprint, mesh_exposed_faces};

fn main() -> ExitCode {
    let mut arguments: Vec<String> = env::args().skip(1).collect();
    let seed = match take_seed(&mut arguments) {
        Ok(seed) => seed,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let generator = match seed {
        Some(seed) => TerrainGenerator::with_seed(WorldSeed(seed)),
        None => TerrainGenerator::golden(),
    };

    let command = arguments.first().map_or("map", String::as_str);
    let rest: Vec<&str> = arguments.iter().skip(1).map(String::as_str).collect();

    println!(
        "world fingerprint {:#018x}  seed {:#018x}",
        generator.fingerprint(),
        generator.identity().seed.raw()
    );

    let outcome = match command {
        "map" => map(&generator, rest.first().copied().unwrap_or("landform")),
        "column" => column(&generator, &rest),
        "chunk" => chunk(&generator, &rest),
        "signature" => signature(&generator),
        "bench" => bench(&generator, &rest),
        "poses" => poses(),
        "vegetation" => vegetation(&generator),
        other => Err(format!("unknown command `{other}`")),
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Removes `--seed <value>` from the argument list, if present.
fn take_seed(arguments: &mut Vec<String>) -> Result<Option<u64>, String> {
    let Some(position) = arguments.iter().position(|argument| argument == "--seed") else {
        return Ok(None);
    };
    let Some(raw) = arguments.get(position + 1).cloned() else {
        return Err("--seed needs a value".to_owned());
    };
    let value = parse_u64(&raw)?;
    arguments.drain(position..=position + 1);
    Ok(Some(value))
}

fn parse_u64(raw: &str) -> Result<u64, String> {
    let parsed = if let Some(hex) = raw.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        raw.parse()
    };
    parsed.map_err(|error| format!("`{raw}` is not a number: {error}"))
}

fn parse_i64(raw: &str) -> Result<i64, String> {
    raw.parse()
        .map_err(|error| format!("`{raw}` is not a coordinate: {error}"))
}

/// A coarse plan view of the region, one character per sampled column.
fn map(generator: &TerrainGenerator, mode: &str) -> Result<(), String> {
    let extent = TerrainConfig::golden().extent;
    let edge = CHUNK_EDGE as i64;
    let min_x = i64::from(extent.min_chunk_x) * edge;
    let max_x = i64::from(extent.max_chunk_x) * edge + edge - 1;
    let min_z = i64::from(extent.min_chunk_z) * edge;
    let max_z = i64::from(extent.max_chunk_z) * edge + edge - 1;
    // One character per twelve voxels keeps the whole region on one screen.
    let step = 12;

    println!("{mode} map, x {min_x}..{max_x}, z {min_z}..{max_z}, {step} voxels per character");
    let field = generator.field();
    let mut z = min_z;
    while z <= max_z {
        let mut line = String::new();
        let mut x = min_x;
        while x <= max_x {
            let sample = field.sample(x as f64 + 0.5, z as f64 + 0.5);
            line.push(match mode {
                "landform" => match sample.landform {
                    veldwake_procedural::Landform::Channel => '~',
                    veldwake_procedural::Landform::ValleyFloor => '.',
                    veldwake_procedural::Landform::Shoulder => '/',
                    veldwake_procedural::Landform::Highland => '#',
                    veldwake_procedural::Landform::Ridge => 'A',
                },
                "zone" => match sample.zone {
                    veldwake_procedural::BiomeZone::RiverBank => 's',
                    veldwake_procedural::BiomeZone::Meadow => ',',
                    veldwake_procedural::BiomeZone::MeadowWood => 'T',
                    veldwake_procedural::BiomeZone::Slope => '/',
                    veldwake_procedural::BiomeZone::Highland => 'h',
                    veldwake_procedural::BiomeZone::RockyRidge => 'R',
                },
                "height" => height_character(sample.height),
                "material" => material_character(field.surface_material(&sample)),
                other => return Err(format!("unknown map mode `{other}`")),
            });
            x += step;
        }
        println!("{line}");
        z += step;
    }
    Ok(())
}

fn height_character(height: f64) -> char {
    const RAMP: [char; 10] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
    let index = ((height / 10.0).floor().max(0.0) as usize).min(RAMP.len() - 1);
    RAMP[index]
}

fn material_character(material: veldwake_procedural::TerrainMaterial) -> char {
    match material {
        veldwake_procedural::TerrainMaterial::MeadowGrass => 'g',
        veldwake_procedural::TerrainMaterial::HighlandGrass => 'G',
        veldwake_procedural::TerrainMaterial::Sediment => 's',
        veldwake_procedural::TerrainMaterial::Rock => 'R',
        _ => '?',
    }
}

/// Everything the generator knows about one column, plus its named probe if it
/// has one.
fn column(generator: &TerrainGenerator, rest: &[&str]) -> Result<(), String> {
    let (Some(raw_x), Some(raw_z)) = (rest.first(), rest.get(1)) else {
        return Err("column needs an x and a z".to_owned());
    };
    let x = parse_i64(raw_x)?;
    let z = parse_i64(raw_z)?;
    let field = generator.field();
    let sample = field.sample(x as f64 + 0.5, z as f64 + 0.5);

    println!("column ({x}, {z})");
    println!(
        "  height          {:.3}  (surface voxel y={})",
        sample.height,
        sample.surface_y()
    );
    println!(
        "  water surface   {:.3}  (top water voxel y={}, submerged: {})",
        sample.water_surface,
        sample.water_surface_y(),
        sample.is_submerged()
    );
    println!("  axis distance   {:.3}", sample.axis_distance);
    println!("  slope           {:.4}", sample.slope);
    println!("  moisture        {:.4}", sample.moisture);
    println!("  landform        {}", sample.landform.name());
    println!("  zone            {}", sample.zone.name());
    println!(
        "  surface         {}",
        field.surface_material(&sample).name()
    );
    println!(
        "  strata          {}",
        (0..12)
            .map(|depth| {
                field
                    .subsurface_material(&sample, depth, sample.surface_y() - i64::from(depth))
                    .name()
            })
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!(
        "  clear of water  {}",
        sample.height > sample.water_surface + SHORE_RISE
    );
    // How far above the surface a camera has to stand to clear every plant in
    // the column, which is what a usable pose needs.
    let mut clearance = 0;
    while clearance < 24
        && generator
            .vegetation()
            .occupied(field, x, sample.surface_y() + clearance + 1, z)
    {
        clearance += 1;
    }
    println!("  plant clearance {clearance} voxels above the surface");
    Ok(())
}

/// The contents of one chunk.
fn chunk(generator: &TerrainGenerator, rest: &[&str]) -> Result<(), String> {
    let (Some(raw_x), Some(raw_y), Some(raw_z)) = (rest.first(), rest.get(1), rest.get(2)) else {
        return Err("chunk needs an x, a y, and a z".to_owned());
    };
    let coord = ChunkCoord::new(
        i32::try_from(parse_i64(raw_x)?).map_err(|_| "x is out of range".to_owned())?,
        i32::try_from(parse_i64(raw_y)?).map_err(|_| "y is out of range".to_owned())?,
        i32::try_from(parse_i64(raw_z)?).map_err(|_| "z is out of range".to_owned())?,
    );

    let started = Instant::now();
    let generated = generator.generate(coord);
    let elapsed = started.elapsed();

    match generated {
        None => println!("{coord:?} is outside the region: authoritative absence"),
        Some(chunk) => {
            println!(
                "{coord:?} present, {} solid of {}, fingerprint {:#018x}, generated in {:.3} ms",
                chunk.solid_count(),
                veldwake_voxel::CHUNK_VOLUME,
                fingerprint(&chunk),
                elapsed.as_secs_f64() * 1000.0
            );
            for (material, count) in material_histogram(&chunk) {
                println!("  {:<18} {count}", material.name());
            }
            // Meshing cost and the GPU footprint it implies. The renderer
            // converts each vertex to forty bytes (position, normal, colour,
            // specular) and each index to four.
            let started = Instant::now();
            let mesh = mesh_exposed_faces(&chunk);
            let mesh_time = started.elapsed();
            let vertices = mesh.vertices().len();
            let indices = mesh.indices().len();
            println!(
                "  mesh: {} quads, {vertices} vertices, {indices} indices, {} gpu bytes, meshed in {:.3} ms",
                vertices / 4,
                vertices * 40 + indices * 4,
                mesh_time.as_secs_f64() * 1000.0
            );
        }
    }
    Ok(())
}

/// The locked regional signature, recomputed.
fn signature(generator: &TerrainGenerator) -> Result<(), String> {
    let started = Instant::now();
    let value = region_signature(generator);
    println!(
        "regional signature {value:#018x} over {} chunks in {:.1} ms",
        SIGNATURE_CHUNKS.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    println!("named probes:");
    for probe in GOLDEN_PROBES {
        let sample = generator
            .field()
            .sample(probe.x as f64 + 0.5, probe.z as f64 + 0.5);
        println!(
            "  {:<22} ({:>5}, {:>5})  height {:>7.3}  slope {:.3}  {:<12} {:<12} {}",
            probe.name,
            probe.x,
            probe.z,
            sample.height,
            sample.slope,
            sample.landform.name(),
            sample.zone.name(),
            generator.field().surface_material(&sample).name()
        );
    }
    Ok(())
}

/// Generation cost over a traversal of the region.
fn bench(generator: &TerrainGenerator, rest: &[&str]) -> Result<(), String> {
    let requested = match rest.first() {
        Some(raw) => parse_i64(raw)?.max(1),
        None => 128,
    };
    let extent = TerrainConfig::golden().extent;

    let mut coords = Vec::new();
    'outer: for z in extent.min_chunk_z..=extent.max_chunk_z {
        for y in extent.min_chunk_y..=extent.max_chunk_y {
            for x in extent.min_chunk_x..=extent.max_chunk_x {
                coords.push(ChunkCoord::new(x, y, z));
                if coords.len() as i64 >= requested {
                    break 'outer;
                }
            }
        }
    }

    let mut timings = Vec::with_capacity(coords.len());
    let mut mesh_timings = Vec::with_capacity(coords.len());
    let mut solid = 0_usize;
    let mut present = 0_usize;
    let mut gpu_bytes = 0_usize;
    let mut quads = 0_usize;
    for coord in &coords {
        let started = Instant::now();
        let generated = generator.generate(*coord);
        timings.push(started.elapsed().as_secs_f64() * 1000.0);
        if let Some(chunk) = generated {
            present += 1;
            solid += chunk.solid_count();
            let started = Instant::now();
            let mesh = mesh_exposed_faces(&chunk);
            mesh_timings.push(started.elapsed().as_secs_f64() * 1000.0);
            quads += mesh.vertices().len() / 4;
            gpu_bytes += mesh.vertices().len() * 40 + mesh.indices().len() * 4;
        }
    }
    timings.sort_by(f64::total_cmp);
    mesh_timings.sort_by(f64::total_cmp);

    let total: f64 = timings.iter().sum();
    let mesh_total: f64 = mesh_timings.iter().sum();
    println!(
        "generated {} chunks ({present} present, {solid} solid voxels) in {total:.1} ms",
        coords.len()
    );
    println!(
        "meshed {present} chunks ({quads} quads, {gpu_bytes} gpu bytes) in {mesh_total:.1} ms"
    );
    if !mesh_timings.is_empty() {
        println!(
            "  per mesh: median {:.3} ms  p95 {:.3} ms  max {:.3} ms  mean gpu bytes {}",
            mesh_timings[mesh_timings.len() / 2],
            mesh_timings[mesh_timings.len() * 95 / 100],
            mesh_timings.last().copied().unwrap_or(0.0),
            gpu_bytes / present.max(1)
        );
    }
    println!(
        "  per chunk: min {:.3} ms  median {:.3} ms  p95 {:.3} ms  max {:.3} ms  mean {:.3} ms",
        timings.first().copied().unwrap_or(0.0),
        timings[timings.len() / 2],
        timings[timings.len() * 95 / 100],
        timings.last().copied().unwrap_or(0.0),
        total / timings.len() as f64
    );
    Ok(())
}

/// How much of the meadow the canopy actually covers.
///
/// The style bible caps vegetation at a share of the meadow surface, and the
/// only honest way to check that is to count columns rather than to reason
/// about densities.
fn vegetation(generator: &TerrainGenerator) -> Result<(), String> {
    let field = generator.field();
    let system = generator.vegetation();
    let spacing = TerrainConfig::golden().tree_spacing;

    // A band along the valley floor, wide enough to include both meadow zones.
    let (min_x, max_x) = (-360_i64, 360_i64);
    let (min_z, max_z) = (-120_i64, 120_i64);

    let mut covered = std::collections::BTreeSet::new();
    let mut trees = 0_usize;
    let mut heights = [0_usize; 12];
    for cell_z in min_z.div_euclid(spacing) - 1..=max_z.div_euclid(spacing) + 1 {
        for cell_x in min_x.div_euclid(spacing) - 1..=max_x.div_euclid(spacing) + 1 {
            let Some(tree) = system.tree_in_cell(field, cell_x, cell_z) else {
                continue;
            };
            trees += 1;
            if let Some(slot) = heights.get_mut(tree.height as usize) {
                *slot += 1;
            }
            let (low, high) = tree.bounds();
            for z in low[2]..=high[2] {
                for x in low[0]..=high[0] {
                    covered.insert((x, z));
                }
            }
        }
    }

    let mut meadow = 0_usize;
    let mut meadow_covered = 0_usize;
    let mut shrubs = 0_usize;
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let sample = field.sample(x as f64 + 0.5, z as f64 + 0.5);
            if !field.surface_material(&sample).supports_vegetation() {
                continue;
            }
            meadow += 1;
            if covered.contains(&(x, z)) {
                meadow_covered += 1;
            }
        }
    }
    for cell_z in min_z.div_euclid(TerrainConfig::golden().shrub_spacing)
        ..=max_z.div_euclid(TerrainConfig::golden().shrub_spacing)
    {
        for cell_x in min_x.div_euclid(TerrainConfig::golden().shrub_spacing)
            ..=max_x.div_euclid(TerrainConfig::golden().shrub_spacing)
        {
            if system.shrub_in_cell(field, cell_x, cell_z).is_some() {
                shrubs += 1;
            }
        }
    }

    println!(
        "band x {min_x}..{max_x}, z {min_z}..{max_z}: {trees} trees, {shrubs} shrubs, {meadow} grass columns"
    );
    println!(
        "canopy covers {meadow_covered} of {meadow} grass columns ({:.1}%)",
        100.0 * meadow_covered as f64 / meadow.max(1) as f64
    );
    for (height, count) in heights.iter().enumerate() {
        if *count > 0 {
            println!("  height {height:>2}: {count}");
        }
    }
    Ok(())
}

/// The named camera poses the visual evidence is captured from.
fn poses() -> Result<(), String> {
    for pose in GOLDEN_POSES {
        println!(
            "{:<20} position ({:>7.1}, {:>6.1}, {:>7.1})  yaw {:>7.1}  pitch {:>6.1}  {}",
            pose.name,
            pose.position[0],
            pose.position[1],
            pose.position[2],
            pose.yaw_degrees,
            pose.pitch_degrees,
            pose.intent
        );
    }
    Ok(())
}
