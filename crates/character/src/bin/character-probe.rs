//! Headless inspection of a compiled character.
//!
//! The character compiler has no GPU, no window, and no frame loop, so
//! everything it produces can be looked at from a terminal. This probe is how
//! a person or an agent answers "what is actually there?" without launching the
//! client, and how the character measurements in the milestone document are
//! taken.
//!
//! ```text
//! character-probe body
//! character-probe parts
//! character-probe skeleton
//! character-probe collision
//! character-probe palette
//! character-probe poses
//! character-probe contact
//! character-probe signature
//! character-probe bench [iterations]
//! ```
//!
//! `--fixture golden|sturdy|varied` selects which character to report on.

use std::{env, process::ExitCode, time::Instant};

use veldwake_character::{
    CharacterCompiler, CharacterDescriptor, CharacterError, CompiledCharacter,
    collision::CollisionRepresentation,
    compiler::behaviour_signature,
    descriptor::CHARACTER_VOXEL_SIZE,
    fixture::{
        NAMED_POSES, golden_descriptor, locked_values, measured_values, pose_fingerprint,
        state_for, sturdy_descriptor, varied_descriptor,
    },
    ground::{FlatGround, GroundSampler, RampGround, StepGround, SteppedRamp},
    locomotion::{LEFT, RIGHT},
    material::{ALL_MATERIALS, CompiledPalette, luminance, saturation},
    pose::{CharacterState, pose},
    skeleton::ALL_BONES,
};

/// Bytes in one part uniform: a `mat4x4<f32>` model matrix and a `vec4<f32>`
/// of shading parameters, matching `renderer::PartUniform`.
const PART_UNIFORM_BYTES: usize = 80;

fn main() -> ExitCode {
    let mut arguments: Vec<String> = env::args().skip(1).collect();
    let fixture = match take_fixture(&mut arguments) {
        Ok(fixture) => fixture,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let descriptor = match fixture {
        Fixture::Golden => golden_descriptor(),
        Fixture::Sturdy => sturdy_descriptor(),
        Fixture::Varied => varied_descriptor(),
    };
    let mut compiler = CharacterCompiler::new();
    let character = match compiler.compile_descriptor(&descriptor) {
        Ok(character) => character,
        Err(error) => {
            eprintln!("the {} fixture does not compile: {error}", fixture.name());
            return ExitCode::from(2);
        }
    };

    println!(
        "fixture {}  identity {:#018x}  geometry {:#018x}  skeleton {:#018x}  collision {:#018x}",
        fixture.name(),
        character.fingerprint(),
        character.geometry_fingerprint(),
        character.skeleton_fingerprint(),
        character.collision_fingerprint()
    );

    let command = arguments.first().map_or("body", String::as_str);
    let rest: Vec<&str> = arguments.iter().skip(1).map(String::as_str).collect();
    let outcome = match command {
        "body" => body(&character),
        "parts" => parts(&character),
        "skeleton" => skeleton(&character),
        "collision" => collision(&character),
        "palette" => palette(character.palette()),
        "poses" => poses(&character),
        "contact" => contact(&character),
        "signature" => signature(),
        "bench" => bench(&descriptor, rest.first().copied()),
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

#[derive(Clone, Copy)]
enum Fixture {
    Golden,
    Sturdy,
    Varied,
}

impl Fixture {
    const fn name(self) -> &'static str {
        match self {
            Self::Golden => "golden",
            Self::Sturdy => "sturdy",
            Self::Varied => "varied",
        }
    }
}

fn take_fixture(arguments: &mut Vec<String>) -> Result<Fixture, String> {
    let Some(position) = arguments
        .iter()
        .position(|argument| argument == "--fixture")
    else {
        return Ok(Fixture::Golden);
    };
    let Some(raw) = arguments.get(position + 1).cloned() else {
        return Err("--fixture needs a value".to_owned());
    };
    arguments.drain(position..=position + 1);
    match raw.as_str() {
        "golden" => Ok(Fixture::Golden),
        "sturdy" => Ok(Fixture::Sturdy),
        "varied" => Ok(Fixture::Varied),
        other => Err(format!("unknown fixture `{other}`")),
    }
}

fn body(character: &CompiledCharacter) -> Result<(), String> {
    let body = character.body();
    println!();
    println!("body, in character voxels (one voxel is {CHARACTER_VOXEL_SIZE} world units)");
    let rows: [(&str, i32); 22] = [
        ("height", body.height),
        ("head height", body.head_height),
        ("head width", body.head_width),
        ("head depth", body.head_depth),
        ("leg length", body.leg_length),
        ("thigh length", body.thigh_length),
        ("foot height", body.foot_height),
        ("foot length", body.foot_length),
        ("arm length", body.arm_length),
        ("upper arm length", body.upper_arm_length),
        ("forearm length", body.forearm_length),
        ("hand length", body.hand_length),
        ("hand depth", body.hand_depth),
        ("limb thickness", body.limb_thickness),
        ("shoulder span", body.shoulder_span),
        ("hip width", body.hip_width),
        ("waist width", body.waist_width),
        ("torso depth", body.torso_depth),
        ("ankle y", body.ankle_y),
        ("knee y", body.knee_y),
        ("hip y", body.hip_y),
        ("shoulder y", body.shoulder_y),
    ];
    for (name, value) in rows {
        println!("  {name:<20} {value:>4}");
    }
    println!(
        "  {:<20} {:>6.3} world units",
        "total height",
        body.height_units()
    );
    println!();
    println!("style ratios");
    let height = f32::from(i16::try_from(body.height).unwrap_or(1));
    println!(
        "  head / height        {:.3}",
        body.head_height as f32 / height
    );
    println!(
        "  leg / height         {:.3}",
        body.leg_length as f32 / height
    );
    println!(
        "  arm / height         {:.3}",
        body.arm_length as f32 / height
    );
    println!(
        "  shoulder / head      {:.3}",
        body.shoulder_span as f32 / body.head_width as f32
    );
    println!(
        "  hip / shoulder       {:.3}",
        body.hip_width as f32 / body.shoulder_span as f32
    );
    println!(
        "  waist / hip          {:.3}",
        body.waist_width as f32 / body.hip_width as f32
    );
    println!(
        "  limb / height        {:.3}",
        body.limb_thickness as f32 / height
    );
    println!("  leg gap              {:>4}", 2 * body.leg_inner);
    Ok(())
}

fn parts(character: &CompiledCharacter) -> Result<(), String> {
    println!();
    println!(
        "{:<12} {:>10} {:>8} {:>7} {:>8} {:>10}  materials",
        "part", "dims", "voxels", "quads", "vertices", "bytes"
    );
    let mut total_quads = 0;
    let mut total_bytes = 0;
    for part in character.parts() {
        let mesh = part.mesh();
        total_quads += mesh.quad_count();
        total_bytes += mesh.payload_bytes();
        let names: Vec<&str> = part.materials().into_iter().map(|m| m.name()).collect();
        println!(
            "{:<12} {:>3}x{:>2}x{:>2} {:>8} {:>7} {:>8} {:>10}  {}",
            part.bone().name(),
            part.volume().dims[0],
            part.volume().dims[1],
            part.volume().dims[2],
            part.solid_voxels(),
            mesh.quad_count(),
            mesh.vertices().len(),
            mesh.payload_bytes(),
            names.join(", ")
        );
    }
    println!();
    println!(
        "total: {} parts, {} voxels, {total_quads} quads, {total_bytes} CPU mesh bytes",
        character.parts().len(),
        character.solid_voxels()
    );
    // Forty bytes of position, normal, colour, and specular per vertex, four
    // bytes per index, plus one eighty-byte part uniform: a four-by-four model
    // matrix and a four-float parameter vector. Exactly what the client
    // uploads, and the client logs the same figure, so the two can be compared
    // rather than assumed.
    let vertices: usize = character
        .parts()
        .iter()
        .map(|part| part.mesh().vertices().len())
        .sum();
    let indices: usize = character
        .parts()
        .iter()
        .map(|part| part.mesh().indices().len())
        .sum();
    println!(
        "gpu geometry: {} vertex bytes + {} index bytes + {} uniform bytes = {}",
        vertices * 40,
        indices * 4,
        character.parts().len() * PART_UNIFORM_BYTES,
        vertices * 40 + indices * 4 + character.parts().len() * PART_UNIFORM_BYTES
    );
    println!(
        "draw calls with the character enabled: {} world + {} shadow",
        character.parts().len(),
        character.parts().len()
    );
    Ok(())
}

fn skeleton(character: &CompiledCharacter) -> Result<(), String> {
    let skeleton = character.skeleton();
    let world = skeleton.rest_world();
    println!();
    println!(
        "{:<3} {:<12} {:<12} {:>26} {:>26}",
        "id", "bone", "parent", "rest local (voxels)", "rest world (voxels)"
    );
    for bone in ALL_BONES {
        let description = skeleton.bone(bone);
        let local = description.rest_local.translation;
        let global = world[bone.index()].translation;
        println!(
            "{:<3} {:<12} {:<12} {:>8.2}{:>9.2}{:>9.2} {:>8.2}{:>9.2}{:>9.2}",
            bone.index(),
            bone.name(),
            description.parent.map_or("-", |parent| parent.name()),
            local.x,
            local.y,
            local.z,
            global.x,
            global.y,
            global.z
        );
    }
    println!();
    println!(
        "{} bones, {} bytes",
        ALL_BONES.len(),
        character.skeleton_payload_bytes()
    );
    Ok(())
}

fn collision(character: &CompiledCharacter) -> Result<(), String> {
    let collision: &CollisionRepresentation = character.collision();
    let capsule = collision.capsule();
    println!();
    println!("body capsule, world units, relative to the stand point");
    println!("  radius          {:.4}", capsule.radius);
    println!("  segment height  {:.4}", capsule.segment_height);
    println!("  base height     {:.4}", capsule.base_height);
    println!("  total height    {:.4}", capsule.total_height());
    println!();
    println!(
        "{:<12} {:>24} {:>24}",
        "part box", "centre (voxels)", "half extents (voxels)"
    );
    for part in collision.boxes() {
        println!(
            "{:<12} {:>7.2}{:>8.2}{:>8.2} {:>7.2}{:>8.2}{:>8.2}",
            part.bone.name(),
            part.centre.x,
            part.centre.y,
            part.centre.z,
            part.half_extents.x,
            part.half_extents.y,
            part.half_extents.z
        );
    }
    println!();
    println!("collision bytes: {}", collision.payload_bytes());
    Ok(())
}

fn palette(palette: &CompiledPalette) -> Result<(), String> {
    println!();
    println!(
        "{:<16} {:>22} {:>9} {:>11}",
        "material", "linear rgb", "luminance", "saturation"
    );
    for material in ALL_MATERIALS {
        let albedo = palette.albedo(material);
        println!(
            "{:<16} {:>6.3}{:>8.3}{:>8.3} {:>9.3} {:>11.3}",
            material.name(),
            albedo[0],
            albedo[1],
            albedo[2],
            luminance(albedo),
            saturation(albedo)
        );
    }
    Ok(())
}

fn poses(character: &CompiledCharacter) -> Result<(), String> {
    println!();
    for named in NAMED_POSES {
        let state = state_for(named);
        let posed = pose(character, &state, None);
        let angles = posed.angles();
        println!(
            "{:<14} speed {:>4.1}  phase {:>5.3}  blend {:.2}/{:.2}  fingerprint {:#018x}",
            named.name,
            named.speed,
            named.phase,
            posed.blend().moving,
            posed.blend().run,
            pose_fingerprint(&posed)
        );
        println!(
            "               hip {:>7.1}/{:<7.1} knee {:>6.1}/{:<6.1} shoulder {:>6.1}/{:<6.1} elbow {:>5.1}/{:<5.1}",
            angles.hip_pitch[LEFT].to_degrees(),
            angles.hip_pitch[RIGHT].to_degrees(),
            angles.knee_flex[LEFT].to_degrees(),
            angles.knee_flex[RIGHT].to_degrees(),
            angles.shoulder_pitch[LEFT].to_degrees(),
            angles.shoulder_pitch[RIGHT].to_degrees(),
            angles.elbow_flex[LEFT].to_degrees(),
            angles.elbow_flex[RIGHT].to_degrees(),
        );
        println!(
            "               pelvis rise {:>6.2} sway {:>6.2} yaw {:>6.1}  chest yaw {:>6.1}  within limits {}",
            angles.pelvis_rise,
            angles.pelvis_sway,
            angles.pelvis_yaw.to_degrees(),
            angles.chest_yaw.to_degrees(),
            angles.within_limits()
        );
        println!("               {}", named.intent);
    }
    Ok(())
}

fn contact(character: &CompiledCharacter) -> Result<(), String> {
    let grounds: [(&str, Box<dyn GroundSampler>); 4] = [
        ("flat", Box::new(FlatGround::at(18.0))),
        (
            "ramp 0.35",
            Box::new(RampGround {
                slope: 0.35,
                height_at_origin: 18.0,
            }),
        ),
        ("terraced 0.18", Box::new(SteppedRamp::terrain(0.18, 18.0))),
        (
            "step",
            Box::new(StepGround {
                edge_x: 0.0,
                low: 18.0,
                high: 19.0,
            }),
        ),
    ];

    println!();
    for (name, ground) in &grounds {
        let mut state = CharacterState::standing(
            -6.0,
            0.0,
            std::f32::consts::FRAC_PI_2,
            Some(ground.as_ref()),
        );
        state.speed = character.gait().speed_for(1.5);
        let mut worst_stance = 0.0_f32;
        let mut worst_any = 0.0_f32;
        let mut samples = 0_u32;
        let mut unreached = 0_u32;
        let started = Instant::now();
        for _ in 0..1_200 {
            state.walk_forward(1.0 / 120.0, character, Some(ground.as_ref()));
            let posed = pose(character, &state, Some(ground.as_ref()));
            for side in [LEFT, RIGHT] {
                let foot = posed.contacts()[side];
                if !foot.grounded {
                    continue;
                }
                samples += 1;
                if !foot.reached {
                    unreached += 1;
                }
                worst_any = worst_any.max(foot.clearance().abs());
                if foot.stance > 0.9 {
                    worst_stance = worst_stance.max(foot.clearance().abs());
                }
            }
        }
        let elapsed = started.elapsed();
        println!(
            "{name:<14} samples {samples:>5}  worst planted clearance {:>7.4}  worst any {:>7.4}  \
             unreached {unreached:>4}  pose+ik {:>7.1} us/frame",
            worst_stance,
            worst_any,
            elapsed.as_secs_f64() * 1.0e6 / 1_200.0
        );
        println!(
            "               tolerance is one character voxel = {CHARACTER_VOXEL_SIZE:.4} world units"
        );
    }
    Ok(())
}

fn signature() -> Result<(), String> {
    let measured = measured_values().map_err(|error: CharacterError| error.to_string())?;
    let locked = locked_values();
    println!();
    println!(
        "{:<20} {:>20} {:>20}  status",
        "value", "locked", "measured"
    );
    let mut drifted = false;
    for (index, (name, value)) in measured.into_iter().enumerate() {
        let (_, expected) = locked[index];
        let mark = if value == expected {
            "ok"
        } else {
            drifted = true;
            "CHANGED"
        };
        println!("{name:<20} {expected:>#20x} {value:>#20x}  {mark}");
    }
    if drifted {
        return Err("the compiled characters no longer match their locked fixtures".to_owned());
    }
    Ok(())
}

fn bench(descriptor: &CharacterDescriptor, iterations: Option<&str>) -> Result<(), String> {
    let count: usize = match iterations {
        Some(raw) => raw
            .parse()
            .map_err(|_| format!("`{raw}` is not an iteration count"))?,
        None => 64,
    };
    if count == 0 {
        return Err("an iteration count of zero measures nothing".to_owned());
    }

    let mut compiler = CharacterCompiler::new();
    let mut timings = Vec::with_capacity(count);
    let mut character = None;
    for _ in 0..count {
        let started = Instant::now();
        let compiled = compiler
            .compile_descriptor(descriptor)
            .map_err(|error| error.to_string())?;
        timings.push(started.elapsed().as_secs_f64() * 1.0e6);
        character = Some(compiled);
    }
    let Some(character) = character else {
        return Err("no character was compiled".to_owned());
    };
    timings.sort_by(f64::total_cmp);
    let percentile = |fraction: f64| {
        let index = ((timings.len() as f64 - 1.0) * fraction).round() as usize;
        timings[index.min(timings.len() - 1)]
    };
    let mean = timings.iter().sum::<f64>() / timings.len() as f64;

    println!();
    println!("compile, {count} iterations, microseconds");
    println!("  min    {:>10.1}", timings[0]);
    println!("  median {:>10.1}", percentile(0.5));
    println!("  p95    {:>10.1}", percentile(0.95));
    println!("  max    {:>10.1}", timings[timings.len() - 1]);
    println!("  mean   {:>10.1}", mean);
    println!();
    println!("memory");
    println!(
        "  scratch grid (transient peak) {:>10} bytes",
        compiler.scratch_bytes()
    );
    println!(
        "  cpu mesh payload              {:>10} bytes",
        character.mesh_payload_bytes()
    );
    println!(
        "  skeleton                      {:>10} bytes",
        character.skeleton_payload_bytes()
    );
    println!(
        "  collision                     {:>10} bytes",
        character.collision().payload_bytes()
    );
    let vertices: usize = character
        .parts()
        .iter()
        .map(|part| part.mesh().vertices().len())
        .sum();
    let indices: usize = character
        .parts()
        .iter()
        .map(|part| part.mesh().indices().len())
        .sum();
    println!(
        "  gpu geometry                  {:>10} bytes",
        vertices * 40 + indices * 4 + character.parts().len() * PART_UNIFORM_BYTES
    );
    println!(
        "  dynamic upload per frame      {:>10} bytes",
        character.parts().len() * PART_UNIFORM_BYTES
    );

    // Pose cost, split so the animation and the contact work are separable.
    let ground = FlatGround::at(18.0);
    let mut state = CharacterState::standing(0.0, 0.0, 0.0, Some(&ground));
    state.speed = character.gait().speed_for(1.6);
    let frames = 4_000;
    let started = Instant::now();
    for _ in 0..frames {
        state.advance(1.0 / 120.0, &character, Some(&ground));
        let posed = pose(&character, &state, None);
        std::hint::black_box(posed.part_matrices()[0]);
    }
    let animation_only = started.elapsed().as_secs_f64() * 1.0e6 / f64::from(frames);
    let started = Instant::now();
    for _ in 0..frames {
        state.advance(1.0 / 120.0, &character, Some(&ground));
        let posed = pose(&character, &state, Some(&ground));
        std::hint::black_box(posed.part_matrices()[0]);
    }
    let with_contact = started.elapsed().as_secs_f64() * 1.0e6 / f64::from(frames);
    println!();
    println!("pose, microseconds per frame, {frames} frames");
    println!("  animation + matrices {:>8.2}", animation_only);
    println!("  with contact and ik  {:>8.2}", with_contact);
    println!(
        "  contact and ik alone {:>8.2}",
        with_contact - animation_only
    );
    println!();
    println!("signature {:#018x}", behaviour_signature(&character));
    Ok(())
}
