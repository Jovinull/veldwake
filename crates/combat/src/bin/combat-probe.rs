//! Headless evidence for the M6 combat slice.
//!
//! No GPU, no window, no audio device. Everything the milestone claims about
//! timelines, reach, hits, the adversary, determinism and cost is printed here so
//! it can be read rather than taken on trust, and so a disagreement between this
//! and the client is visible instead of assumed.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use glam::Vec2;

use veldwake_character::CHARACTER_VOXEL_SIZE;
use veldwake_character::descriptor::BodyMetrics;
use veldwake_character::ground::FlatGround;
use veldwake_character::skeleton::BoneId;
use veldwake_combat::combatant::{Intent, SIDES, Side};
use veldwake_combat::encounter::{Encounter, EncounterSetup, WorldContact};
use veldwake_combat::fixture;
use veldwake_combat::hit::{Capsule, Segment, Sweep, closest_points, sweep_capsule};
use veldwake_combat::hurt::narrowing;
use veldwake_combat::material::{ALL_WEAPON_MATERIALS, luminance};
use veldwake_combat::script::{GOLDEN_SCRIPT, NAMED_MOMENTS, ScriptRunner};
use veldwake_combat::tick::{COMBAT_TICK_HZ, CombatClock, seconds_for};
use veldwake_combat::weapon::{WeaponCompiler, WeaponDescriptor};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let command = arguments.first().map_or("weapon", String::as_str);
    let outcome = match command {
        "weapon" => weapon(),
        "spec" => spec(),
        "reach" => reach(),
        "moments" => moments(),
        "script" => script(),
        "partition" => partition(),
        "signature" => signature(),
        "bench" => bench(arguments.get(1).map(String::as_str)),
        "trace" => trace(arguments.get(1).map(String::as_str)),
        "dodge" => dodge(),
        "bodies" => bodies(),
        "aim" => aim(),
        "contact" => contact(
            arguments.get(1).map(String::as_str),
            arguments.get(2).map(String::as_str),
        ),
        other => Err(format!("unknown command `{other}`")),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("combat-probe: {message}");
            ExitCode::FAILURE
        }
    }
}

fn build() -> Result<(Encounter, EncounterSetup, FlatGround), String> {
    let setup = fixture::golden_setup();
    let ground = fixture::golden_ground();
    let encounter =
        Encounter::new(&setup, Some(&ground)).map_err(|error| format!("setup: {error}"))?;
    Ok((encounter, setup, ground))
}

fn weapon() -> Result<(), String> {
    let descriptor = WeaponDescriptor::golden();
    let weapon = WeaponCompiler::new()
        .compile_descriptor(&descriptor)
        .map_err(|error| format!("weapon: {error}"))?;
    let metrics = weapon.metrics();
    println!("weapon  identity {:#018x}", weapon.fingerprint());
    println!("        geometry {:#018x}", weapon.geometry_fingerprint());
    println!(
        "        dims {:?} voxels  origin {:?}  hand row {}",
        metrics.dims, metrics.origin, metrics.hand_y
    );
    println!(
        "        blade y {:?}  guard y {:?}  grip y {:?}  pommel y {:?}",
        (metrics.blade_y.low, metrics.blade_y.high),
        (metrics.guard_y.low, metrics.guard_y.high),
        (metrics.grip_y.low, metrics.grip_y.high),
        (metrics.pommel_y.low, metrics.pommel_y.high)
    );
    println!(
        "        solids {}  quads {}  cpu mesh bytes {}  scratch bytes {}",
        weapon.solid_voxels(),
        weapon.quad_count(),
        weapon.mesh_payload_bytes(),
        WeaponCompiler::new().scratch_bytes()
    );
    let blade = weapon.blade();
    println!(
        "        blade segment base {:?} tip {:?} radius {} voxels",
        blade.base.to_array(),
        blade.tip.to_array(),
        blade.radius
    );
    println!(
        "        blade length {:.4} world units  radius {:.4}  reach from wrist {:.4}",
        blade.world_length(),
        blade.world_radius(),
        weapon.reach_from_hand()
    );
    println!("        scheme {}", descriptor.scheme.name());
    println!();
    println!("material             voxels   luminance  id");
    for material in ALL_WEAPON_MATERIALS {
        let index = material.index() as usize;
        println!(
            "{:<20} {:>6}   {:>9.4}  {}",
            material.name(),
            weapon.material_counts()[index],
            luminance(weapon.palette().albedo(material)),
            material.voxel_id().0
        );
    }
    Ok(())
}

/// The two bodies side by side, which is how their distinguishability is judged.
fn bodies() -> Result<(), String> {
    let (encounter, _, _) = build()?;
    println!(
        "measure               {:>10}  {:>10}",
        Side::Player.name(),
        Side::Adversary.name()
    );
    let bodies: Vec<_> = SIDES
        .into_iter()
        .map(|side| *encounter.character(side).body())
        .collect();
    type Measure = (&'static str, fn(&BodyMetrics) -> i32);
    let rows: [Measure; 10] = [
        ("height voxels", |body| body.height),
        ("head height", |body| body.head_height),
        ("shoulder span", |body| body.shoulder_span),
        ("hip width", |body| body.hip_width),
        ("waist width", |body| body.waist_width),
        ("torso depth", |body| body.torso_depth),
        ("limb thickness", |body| body.limb_thickness),
        ("leg length", |body| body.leg_length),
        ("arm length", |body| body.arm_length),
        ("foot length", |body| body.foot_length),
    ];
    for (name, get) in rows {
        println!(
            "{name:<20} {:>10}  {:>10}",
            get(&bodies[0]),
            get(&bodies[1])
        );
    }
    for side in SIDES {
        let character = encounter.character(side);
        println!(
            "{:<20} height {:.4} world units  capsule radius {:.4}  fingerprint {:#018x}",
            side.name(),
            character.body().height_units(),
            encounter.combatant(side).capsule().radius,
            character.fingerprint()
        );
    }
    println!();
    // The two volumes side by side. The whole-body capsule keeps the bodies out
    // of each other; the hurt volume decides hits. They are different sizes on
    // purpose, and a capture is why: see `crate::hurt`.
    println!("side        volume       radius   band above ground   of body capsule");
    for side in SIDES {
        let body = encounter.combatant(side).capsule();
        let hurt = encounter.combatant(side).hurt();
        let (low, high) = hurt.band();
        println!(
            "{:<11} whole body   {:>6.4}   {:>6.4} to {:>6.4}   {:>15}",
            side.name(),
            body.radius,
            -body.radius + body.base_height,
            body.total_height(),
            "1.0000"
        );
        println!(
            "{:<11} hurt core    {:>6.4}   {:>6.4} to {:>6.4}   {:>15.4}",
            "",
            hurt.radius(),
            low,
            high,
            narrowing(&hurt, body)
        );
    }
    println!();
    // The margin the hurt volume needs: how far the core actually leaves a
    // capsule fitted to the body standing still, over the whole reference fight.
    let ground = fixture::golden_ground();
    let mut fight = Encounter::new(&fixture::golden_setup(), Some(&ground))
        .map_err(|error| format!("setup: {error}"))?;
    fight.arm();
    let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&fight));
    let mut worst = [0.0_f32; 2];
    let mut worst_bone = [BoneId::Root; 2];
    for _ in 0..fixture::GOLDEN_RUN_TICKS {
        let intent = runner.next_intent(&fight);
        let _ = fight.step(intent, WorldContact::ground_only(&ground));
        if runner.finished() {
            runner.restart();
        }
        for side in SIDES {
            let index = side.index();
            let combatant = fight.combatant(side);
            let capsule = combatant.hurt_capsule();
            let collision = fight.character(side).collision();
            let matrices = combatant.posed().part_matrices();
            for bone in veldwake_combat::hurt::HURT_CORE {
                let matrix = matrices[bone.index()];
                for corner in collision.part_box(bone).corners() {
                    let point = matrix.transform_point3(corner);
                    let (_, _, distance) = closest_points(capsule.axis, Segment::new(point, point));
                    let overshoot = distance - capsule.radius;
                    if overshoot > worst[index] {
                        worst[index] = overshoot;
                        worst_bone[index] = bone;
                    }
                }
            }
        }
    }
    println!("side         worst core overshoot   bone     margin");
    for side in SIDES {
        let index = side.index();
        println!(
            "{:<11} {:>20.4}   {:<8} {:>6.4}",
            side.name(),
            worst[index],
            worst_bone[index].name(),
            veldwake_combat::hurt::HURT_MARGIN
        );
    }
    Ok(())
}

fn spec() -> Result<(), String> {
    let setup = fixture::golden_setup();
    let tuning = fixture::tuning()
        .compile()
        .map_err(|error| format!("tuning: {error}"))?;
    println!(
        "tick rate {COMBAT_TICK_HZ} Hz  ({:.5} s per tick)",
        1.0 / f64::from(COMBAT_TICK_HZ)
    );
    println!();
    println!(
        "attack            windup  active  recovery  total   stagger  hitstop  damage  step-in  knockback"
    );
    for (name, attack) in [
        ("player", tuning.player_attack()),
        ("adversary", tuning.adversary_attack()),
    ] {
        println!(
            "{name:<16} {:>6}  {:>6}  {:>8}  {:>5}   {:>7}  {:>7}  {:>6}  {:>7.2}  {:>9.2}",
            attack.windup(),
            attack.active(),
            attack.recovery(),
            attack.total(),
            attack.stagger(),
            attack.hitstop(),
            attack.damage(),
            attack.step_in(),
            attack.knockback()
        );
        println!(
            "{:<16} {:>6.3}  {:>6.3}  {:>8.3}  {:>5.3}   seconds",
            "",
            seconds_for(attack.windup()),
            seconds_for(attack.active()),
            seconds_for(attack.recovery()),
            seconds_for(attack.total())
        );
        println!(
            "{:<16} active window is ticks [{}, {}) of the swing",
            "",
            attack.active_start(),
            attack.active_end()
        );
    }
    println!();
    let dodge = tuning.dodge();
    println!(
        "dodge    {} ticks ({:.3} s)  cooldown {} ticks  distance {:.2}  speed {:.2} u/s",
        dodge.duration(),
        seconds_for(dodge.duration()),
        dodge.cooldown(),
        dodge.distance(),
        dodge.speed()
    );
    let movement = tuning.movement();
    println!(
        "movement speed {:.2} u/s  turn {:.2} rad/s  step up {:.2}  drop {:.2}  recovery scale {:.2}",
        movement.speed(),
        movement.turn_rate(),
        movement.max_step_up(),
        movement.max_drop(),
        movement.recovery_speed_scale()
    );
    let adversary = tuning.adversary();
    println!(
        "adversary aggro {:.2}  strike {:.2}  min {:.2}  approach {:.2} u/s  reposition {:.2} u/s  recover {} ticks  reposition {} ticks",
        adversary.aggro_radius(),
        adversary.strike_range(),
        adversary.min_range(),
        adversary.approach_speed(),
        adversary.reposition_speed(),
        adversary.recover(),
        adversary.reposition()
    );
    match setup.arena {
        Some(arena) => println!(
            "arena centre {:?} radius {:.2}  health player {} adversary {}  defeat hold {} ticks",
            arena.centre().to_array(),
            arena.radius(),
            tuning.player_health(),
            tuning.adversary_health(),
            tuning.defeat_hold()
        ),
        None => println!(
            "arena none (bounded by the region)  health player {} adversary {}  defeat hold {} ticks",
            tuning.player_health(),
            tuning.adversary_health(),
            tuning.defeat_hold()
        ),
    }
    println!(
        "starts player {:?}  adversary {:?}",
        setup.starts[Side::Player.index()].to_array(),
        setup.starts[Side::Adversary.index()].to_array()
    );
    Ok(())
}

/// The swing envelope: where the blade actually is through a whole attack.
///
/// This is what turns "the reach is about two and a half units" into a
/// measurement, and it is the number the strike range and the dodge distance are
/// judged against.
fn reach() -> Result<(), String> {
    let (mut encounter, _, ground) = build()?;
    encounter.arm();
    let spec = *encounter.attack_spec(Side::Player);
    let capsule = encounter.combatant(Side::Adversary).hurt_capsule();
    let radius = encounter.combatant(Side::Adversary).capsule().radius;
    println!(
        "hurt capsule radius {:.4}  axis {:.4} to {:.4} above ground  player capsule radius {:.4}",
        capsule.radius,
        capsule.axis.base.y,
        capsule.axis.tip.y,
        encounter.combatant(Side::Player).capsule().radius
    );
    println!();
    println!("tick  phase      tip reach  tip height  base reach  base height");
    // Drive the player's own swing with nothing else moving.
    let mut tick = 0;
    let mut max_reach = 0.0_f32;
    let mut active_low = f32::INFINITY;
    let mut active_high = f32::NEG_INFINITY;
    let mut active_reach_low = f32::INFINITY;
    let mut active_reach_high = f32::NEG_INFINITY;
    while tick <= spec.total() {
        let attack = tick == 0;
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, attack, false),
            WorldContact::ground_only(&ground),
        );
        let centre = encounter.combatant(Side::Player).position();
        let blade = encounter.blade_world(Side::Player);
        let tip_reach = (Vec2::new(blade.tip.x, blade.tip.z) - centre).length();
        let base_reach = (Vec2::new(blade.base.x, blade.base.z) - centre).length();
        let elapsed = encounter.combatant(Side::Player).action().elapsed();
        let phase = encounter.combatant(Side::Player).action().label(&spec);
        max_reach = max_reach.max(tip_reach);
        if spec.is_active(elapsed) {
            active_low = active_low.min(blade.tip.y);
            active_high = active_high.max(blade.tip.y);
            active_reach_low = active_reach_low.min(tip_reach);
            active_reach_high = active_reach_high.max(tip_reach);
        }
        if tick % 4 == 0 || spec.is_active(elapsed) {
            println!(
                "{elapsed:>4}  {phase:<9}  {tip_reach:>9.4}  {:>10.4}  {base_reach:>10.4}  {:>11.4}",
                blade.tip.y, blade.base.y
            );
        }
        tick += 1;
    }
    println!();
    // Both sides, measured off the pose path rather than off the encounter, so
    // the adversary's own reach is a number too and not an assumption.
    let weapon = encounter.weapon();
    println!("side        max tip reach   active reach       active height      connects out to");
    for side in SIDES {
        let spec = *encounter.attack_spec(side);
        let envelope = fixture::swing_envelope(
            encounter.character(side),
            weapon,
            &spec,
            encounter.combatant(side).weapon_side(),
        );
        let target = encounter.combatant(side.other()).capsule().radius;
        println!(
            "{:<11} {:>13.4}   {:>5.4} to {:>5.4}   {:>5.4} to {:>5.4}   {:>14.4}",
            side.name(),
            envelope.max_tip_reach,
            envelope.active_reach.0,
            envelope.active_reach.1,
            envelope.active_height.0,
            envelope.active_height.1,
            envelope.connects_out_to(target)
        );
    }
    println!();
    println!("maximum tip reach over the whole swing: {max_reach:.4} world units");
    println!(
        "during the active window the tip sweeps height {active_low:.4} to {active_high:.4} and reach {active_reach_low:.4} to {active_reach_high:.4}"
    );
    println!(
        "the adversary's torso band is {:.4} to {:.4} above the ground",
        capsule.axis.base.y - capsule.radius,
        capsule.axis.tip.y + capsule.radius
    );
    let strike = fixture::adversary().strike_range;
    let connect = active_reach_high + radius;
    println!(
        "a hit connects out to about {connect:.4} centre to centre; the adversary commits at {strike:.4}"
    );
    let dodge = fixture::dodge();
    println!(
        "a dodge of {:.2} units has to clear {:.4}, the slack between that reach and the strike range",
        dodge.distance,
        (connect - strike).max(0.0)
    );
    Ok(())
}

/// Scans every tick of the adversary's swing and reports whether a dodge pressed
/// then escapes it.
///
/// The dodge distance is chosen from this table rather than from arithmetic: the
/// blade's height matters as much as its reach, because the first active ticks
/// pass above a body and the last ones pass through it.
fn dodge() -> Result<(), String> {
    let setup = fixture::golden_setup();
    let ground = fixture::golden_ground();
    let encounter =
        Encounter::new(&setup, Some(&ground)).map_err(|error| format!("setup: {error}"))?;
    let spec = *encounter.attack_spec(Side::Adversary);
    drop(encounter);
    println!(
        "the adversary's swing is {} ticks: windup [0, {}), active [{}, {}), recovery to {}",
        spec.total(),
        spec.active_start(),
        spec.active_start(),
        spec.active_end(),
        spec.total()
    );
    println!();
    for direction in [
        fixture::DodgeDirection::Away,
        fixture::DodgeDirection::Lateral,
    ] {
        println!("dodging {}", direction.name());
        println!("  pressed on tick   phase      started   hit   closest approach");
        let mut escaped = Vec::new();
        // From tick one: a swing's tick zero happens inside the step that starts
        // it, so no observer standing outside the step can press on it.
        for pressed_at in 1..spec.active_end() + 6 {
            let probe = fixture::probe_dodge(&setup, Some(&ground), pressed_at, direction, 2_400)
                .map_err(|error| format!("dodge probe: {error}"))?;
            let phase = if pressed_at < spec.active_start() {
                "windup"
            } else if pressed_at < spec.active_end() {
                "active"
            } else {
                "recovery"
            };
            if probe.started && !probe.hit {
                escaped.push(pressed_at);
            }
            if pressed_at % 4 == 0 || pressed_at >= spec.active_start() - 8 {
                println!(
                    "  {pressed_at:>15}   {phase:<9}  {:>7}   {:>3}   {:>16.4}",
                    probe.started,
                    if probe.hit { "yes" } else { "no" },
                    probe.closest
                );
            }
        }
        match (escaped.first(), escaped.last()) {
            (Some(first), Some(last)) => println!(
                "  escapes when pressed between tick {first} and tick {last} ({} of {} ticks)",
                escaped.len(),
                spec.active_end()
            ),
            _ => println!("  no dodge escapes this swing at all"),
        }
        println!();
    }
    Ok(())
}

/// Where the blade actually is, tick by tick, across one side's swing.
///
/// The question a capture cannot answer on its own: when the rules say a hit
/// landed, was it the edge of the blade that reached the body, or the guard end
/// of it? `closest` is the distance between the two segments' nearest points and
/// `gap` is what is left after the blade radius and the capsule radius are taken
/// off, so a negative `gap` is penetration. `at` says which fraction along the
/// blade the nearest point sits on: `0.00` is the guard, `1.00` is the tip.
/// The yaw that points along a planar direction.
///
/// `veldwake_combat::movement::facing_of` with the `None` case removed, and it
/// delegates rather than repeating the arithmetic: the first version of this
/// wrote `(-direction.x).atan2(-direction.y)`, which mirrors the whole column
/// and made a swing that connected look like one aimed forty-one degrees wide.
fn bearing_of(direction: glam::Vec2) -> f32 {
    veldwake_combat::movement::facing_of(direction).unwrap_or(0.0)
}

/// Signed difference in degrees, folded into `[-180, 180)`.
fn wrap_degrees(radians: f32) -> f32 {
    let mut degrees = radians.to_degrees() % 360.0;
    if degrees >= 180.0 {
        degrees -= 360.0;
    }
    if degrees < -180.0 {
        degrees += 360.0;
    }
    degrees
}

/// How much of a facing error one swing forgives, at every range it reaches.
///
/// The question a played run cannot answer on its own, because a blind driver
/// aims no better than it walks: is a swing hard to land because the player
/// aimed badly, or because the window is too narrow to aim into at all? The
/// answer is a number of degrees, measured by standing a passive body at a
/// bearing and swinging.
fn aim() -> Result<(), String> {
    let ground = fixture::golden_ground();
    let mut setup = fixture::sandbox_setup();
    // A body that will not move, will not swing and will not be knocked
    // anywhere: the measurement is of the attacker's arc, nothing else.
    setup.tuning.player_attack.knockback = 0.0;
    println!("one swing against a still body, by range and facing error");
    println!();
    print!("   range ");
    let errors: Vec<i16> = (-60..=60).step_by(5).collect();
    for error in &errors {
        print!("{error:>4}");
    }
    println!();
    for step in 0_u8..=12 {
        let range = 1.6 + f32::from(i16::from(step)) * 0.1;
        print!("{range:>8.2} ");
        for error in &errors {
            let hit = swing_at(&setup, &ground, range, f32::from(*error))?;
            print!("{:>4}", if hit { "x" } else { "." });
        }
        println!();
    }
    println!();
    println!("`x` is a landed hit. Rows are centre-to-centre range in world units,");
    println!("columns are the attacker's facing error in degrees.");
    Ok(())
}

/// Runs one swing with the attacker at a fixed facing error and reports whether
/// it landed.
fn swing_at(
    setup: &EncounterSetup,
    ground: &FlatGround,
    range: f32,
    error_degrees: f32,
) -> Result<bool, String> {
    let mut setup = *setup;
    // The two bodies on the `z` axis, and the attacker's start facing turned off
    // the line between them by `error`. Turning it by walking would not do: a
    // body that walks to change where it looks has also changed the range.
    setup.starts = [Vec2::new(0.0, range * 0.5), Vec2::new(0.0, -range * 0.5)];
    setup.facing_offsets[Side::Player.index()] = error_degrees.to_radians();
    let mut encounter =
        Encounter::new(&setup, Some(ground)).map_err(|error| format!("setup: {error}"))?;
    encounter.arm();
    let spec = *encounter.attack_spec(Side::Player);
    let mut landed = false;
    // Standing still throughout, so the only thing under test is the arc.
    for tick in 0..spec.total() + 2 {
        let events = encounter.step(
            Intent::player(Vec2::ZERO, tick == 0, false),
            WorldContact::ground_only(ground),
        );
        landed |= events.iter().any(|event| event.name() == "hit");
    }
    Ok(landed)
}

fn contact(side: Option<&str>, arena: Option<&str>) -> Result<(), String> {
    let attacker = match side.unwrap_or("player") {
        "player" => Side::Player,
        "adversary" => Side::Adversary,
        other => return Err(format!("`{other}` is not a side")),
    };
    // The sandbox is the passive adversary the one-hit-per-swing tests use, and
    // the reason it is offered here: a swing that stops connecting in it is a
    // reach or a volume problem, with no brain in the way to blame.
    let sandbox = match arena.unwrap_or("golden") {
        "golden" => false,
        "sandbox" => true,
        other => return Err(format!("`{other}` is not an arena")),
    };
    let victim = attacker.other();
    let ground = fixture::golden_ground();
    let setup = if sandbox {
        fixture::sandbox_setup()
    } else {
        fixture::golden_setup()
    };
    let mut encounter =
        Encounter::new(&setup, Some(&ground)).map_err(|error| format!("setup: {error}"))?;
    encounter.arm();
    let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&encounter));
    let radius = encounter.weapon().blade_radius_world();
    println!(
        "{} swings, measured against {}'s hurt volume",
        attacker.name(),
        victim.name()
    );
    println!();
    println!(
        "  tick  phase       elapsed  distance   closest      gap    at  base y   tip y            facing  bearing    off  event"
    );
    let mut printed = 0_u32;
    for _ in 0..fixture::GOLDEN_RUN_TICKS {
        let intent = if sandbox {
            let free = encounter.combatant(Side::Player).can_act();
            Intent::player(Vec2::new(0.0, -1.0), free, false)
        } else {
            runner.next_intent(&encounter)
        };
        let previous = encounter.blade_world(attacker);
        let events = encounter.step(intent, WorldContact::ground_only(&ground));
        let action = encounter.combatant(attacker).action();
        if !action.is_attacking() {
            continue;
        }
        let blade = encounter.blade_world(attacker);
        let capsule = encounter.combatant(victim).hurt_capsule();
        // `closest_points` returns a **squared** distance. Printing it as a
        // distance is a mistake this column made once and will not make again.
        let (on_blade, _, squared) = closest_points(blade, capsule.axis);
        let closest = squared.sqrt();
        let gap = closest - radius - capsule.radius;
        let along = if blade.length() > f32::EPSILON {
            (on_blade - blade.base).length() / blade.length()
        } else {
            0.0
        };
        let sweep = Sweep::new(previous, blade, radius);
        let landed = events.iter().any(|event| event.name() == "hit");
        // Facing against bearing, because a swing that misses at contact range
        // is either too short or pointed somewhere else, and the two look the
        // same in a distance column.
        let from = encounter.combatant(attacker).position();
        let to = encounter.combatant(victim).position();
        let facing = encounter.combatant(attacker).state().facing;
        let bearing = bearing_of(to - from);
        println!(
            "{:>6}  {:<10} {:>7}  {:>8.4}  {:>8.4}  {:>7.4}  {:>4.2}  {:>6.3}  {:>6.3}               {:>7.2}  {:>7.2}  {:>5.2}  {}{}",
            encounter.tick_index(),
            action.label(encounter.attack_spec(attacker)),
            action.elapsed(),
            from.distance(to),
            closest,
            gap,
            along,
            blade.base.y,
            blade.tip.y,
            facing.to_degrees(),
            bearing.to_degrees(),
            wrap_degrees(bearing - facing),
            if landed { "HIT" } else { "" },
            if sweep_capsule(&sweep, &capsule).is_some() && !landed {
                " (sweep would touch)"
            } else {
                ""
            }
        );
        printed += 1;
        if printed >= 240 {
            break;
        }
        if !sandbox && runner.finished() {
            runner.restart();
        }
    }
    Ok(())
}

fn moments() -> Result<(), String> {
    let ground = fixture::golden_ground();
    let outcome = fixture::run_script(
        GOLDEN_SCRIPT,
        &fixture::golden_setup(),
        Some(&ground),
        fixture::GOLDEN_RUN_TICKS,
    )
    .map_err(|error| format!("script: {error}"))?;
    println!(
        "script {} over {} ticks ({:.1} s)",
        GOLDEN_SCRIPT.name,
        outcome.ticks,
        seconds_for(outcome.ticks as u32)
    );
    println!();
    println!("moment                 first tick   at        intent");
    for (moment, tick) in NAMED_MOMENTS.iter().zip(outcome.moments) {
        match tick {
            Some(tick) => println!(
                "{:<22} {:>10}   {:>6.2}s   {}",
                moment.name,
                tick,
                f64::from(tick as u32) / f64::from(COMBAT_TICK_HZ),
                moment.intent
            ),
            None => println!(
                "{:<22} {:>10}   {:>7}   {}",
                moment.name, "NEVER", "-", moment.intent
            ),
        }
    }
    println!();
    if outcome.every_moment_reached() {
        println!("every named moment is reachable from the reference script");
    } else {
        println!("MISSING: {:?}", outcome.missing_moments());
        return Err("the reference script does not reach every named moment".to_owned());
    }
    Ok(())
}

fn script() -> Result<(), String> {
    let ground = fixture::golden_ground();
    let outcome = fixture::run_script(
        GOLDEN_SCRIPT,
        &fixture::golden_setup(),
        Some(&ground),
        fixture::GOLDEN_RUN_TICKS,
    )
    .map_err(|error| format!("script: {error}"))?;
    let counters = outcome.counters;
    println!(
        "ticks {}  signature {:#018x}",
        counters.ticks, outcome.signature
    );
    println!();
    println!("counter                 player   adversary");
    let rows: [(&str, [u32; 2]); 8] = [
        ("swings", counters.swings),
        ("hits landed", counters.hits),
        ("whiffs", counters.whiffs),
        ("dodges", counters.dodges),
        ("dodges refused", counters.dodges_refused),
        ("staggers taken", counters.staggers),
        ("defeats", counters.defeats),
        ("blocked moves", counters.blocked_moves),
    ];
    for (name, values) in rows {
        println!("{name:<22} {:>6}   {:>9}", values[0], values[1]);
    }
    println!(
        "{:<22} {:>6}   {:>9}",
        "frozen ticks", counters.frozen_ticks[0], counters.frozen_ticks[1]
    );
    println!(
        "{:<22} {:>6}   {:>9}",
        "lowest health", outcome.min_health[0], outcome.min_health[1]
    );
    println!();
    println!(
        "resets {}  separations {}  hit queries {}  worst sweep substeps {}  multi-hit suppressed {}",
        counters.resets,
        counters.separations,
        counters.hit_queries,
        counters.sweep_substeps_max,
        counters.multi_hit_suppressed
    );
    println!(
        "events dropped {}  worst body overlap after separation {:.4} world units",
        counters.events_dropped, outcome.worst_overlap
    );
    if counters.events_dropped > 0 {
        return Err("a tick could not hold its events".to_owned());
    }
    Ok(())
}

/// The partition-equivalence evidence.
///
/// The same exact elapsed time, delivered as 30, 60 and 144 frames a second, must
/// produce the same number of ticks in the same order — and therefore the same
/// encounter, because the script is keyed to ticks.
fn partition() -> Result<(), String> {
    let seconds = 20_u32;
    println!("delivering exactly {seconds} s of wall time at three frame rates");
    println!();
    println!(" Hz   frames   ticks   capped frames   dropped ticks   signature");
    let mut signatures = Vec::new();
    for hz in [30_u32, 60, 144] {
        let frames = u64::from(hz) * u64::from(seconds);
        let total_nanos = u128::from(seconds) * 1_000_000_000;
        let base_nanos = total_nanos / u128::from(frames);
        let remainder_nanos = total_nanos % u128::from(frames);
        let ground = fixture::golden_ground();
        let setup = fixture::golden_setup();
        let mut encounter =
            Encounter::new(&setup, Some(&ground)).map_err(|error| format!("setup: {error}"))?;
        encounter.arm();
        let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&encounter));
        let mut clock = CombatClock::new();
        let mut ticks = 0_u64;
        let mut bytes = Vec::new();
        let mut delivered = 0_u128;
        for index in 0..u128::from(frames) {
            let nanos = base_nanos + u128::from(index < remainder_nanos);
            let frame = Duration::from_nanos(
                u64::try_from(nanos).map_err(|_| "frame duration overflows u64")?,
            );
            delivered += frame.as_nanos();
            let due = clock.advance(frame);
            for _ in 0..due {
                let intent = runner.next_intent(&encounter);
                let events = encounter.step(intent, WorldContact::ground_only(&ground));
                ticks += 1;
                bytes.extend_from_slice(&encounter.tick_index().to_le_bytes());
                for side in SIDES {
                    let combatant = encounter.combatant(side);
                    bytes.extend_from_slice(&combatant.health().current().to_le_bytes());
                    bytes.extend_from_slice(&combatant.position().x.to_bits().to_le_bytes());
                    bytes.extend_from_slice(&combatant.position().y.to_bits().to_le_bytes());
                }
                for event in events.iter() {
                    bytes.extend_from_slice(event.name().as_bytes());
                }
                if runner.finished() {
                    runner.restart();
                }
            }
        }
        if delivered != total_nanos {
            return Err(format!(
                "{hz} Hz did not deliver the requested total duration"
            ));
        }
        let signature = veldwake_combat::fixture::trace_signature(&bytes);
        println!(
            "{hz:>3}   {frames:>6}   {ticks:>5}   {:>13}   {:>13}   {signature:#018x}",
            clock.capped_frames(),
            clock.dropped()
        );
        signatures.push((hz, ticks, signature));
    }
    println!();
    let Some((_, first_ticks, first_signature)) = signatures.first().copied() else {
        return Err("no partitions were run".to_owned());
    };
    for (hz, ticks, signature) in &signatures {
        if *ticks != first_ticks || *signature != first_signature {
            return Err(format!(
                "{hz} Hz produced {ticks} ticks / {signature:#018x} against {first_ticks} / {first_signature:#018x}"
            ));
        }
    }
    println!(
        "every partition produced {first_ticks} ticks and the identical trace {first_signature:#018x}"
    );
    println!("every partition delivered exactly {seconds} s, including its nanosecond remainder");
    Ok(())
}

fn signature() -> Result<(), String> {
    let measured = fixture::measured_values().map_err(|error| format!("{error}"))?;
    let locked = fixture::locked_values();
    println!("value                          locked               measured  status");
    let mut changed = false;
    for (index, (name, value)) in measured.into_iter().enumerate() {
        let (locked_name, locked_value) = locked[index];
        if name != locked_name {
            return Err("the fixture tables drifted apart".to_owned());
        }
        let status = if value == locked_value {
            "ok"
        } else {
            changed = true;
            "CHANGED"
        };
        println!("{name:<26} {locked_value:#018x}   {value:#018x}  {status}");
    }
    if changed {
        return Err("the combat fixtures no longer match their locked values".to_owned());
    }
    Ok(())
}

fn bench(iterations: Option<&str>) -> Result<(), String> {
    let ticks: u64 = match iterations {
        Some(raw) => raw
            .trim()
            .parse()
            .map_err(|_| format!("`{raw}` is not a tick count"))?,
        None => 12_000,
    };
    let ground = fixture::golden_ground();
    let setup = fixture::golden_setup();
    let mut encounter =
        Encounter::new(&setup, Some(&ground)).map_err(|error| format!("setup: {error}"))?;
    encounter.arm();
    let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&encounter));

    // A short warm-up first. Without it the worst-case figure is the very first
    // tick paying for cold code and a cold cache, which is a measurement of
    // process start rather than of a combat tick.
    for _ in 0..240 {
        let intent = runner.next_intent(&encounter);
        let _ = encounter.step(intent, WorldContact::ground_only(&ground));
        if runner.finished() {
            runner.restart();
        }
    }
    // The whole tick, which is the number that matters.
    let mut worst = Duration::ZERO;
    let started = Instant::now();
    for _ in 0..ticks {
        let intent = runner.next_intent(&encounter);
        let tick_started = Instant::now();
        let _ = encounter.step(intent, WorldContact::ground_only(&ground));
        worst = worst.max(tick_started.elapsed());
        if runner.finished() {
            runner.restart();
        }
    }
    let total = started.elapsed();
    println!(
        "combat tick over {ticks} ticks: mean {:.3} us, worst {:.3} us",
        total.as_secs_f64() * 1.0e6 / ticks as f64,
        worst.as_secs_f64() * 1.0e6
    );
    println!(
        "  a {COMBAT_TICK_HZ} Hz simulation therefore costs about {:.3} us per second of play",
        total.as_secs_f64() * 1.0e6 / ticks as f64 * f64::from(COMBAT_TICK_HZ)
    );

    // The same measurement with combat initiative: the lunge, the spacing
    // dodge and the outcome-dependent recovery all running, driven by the read
    // policy and starting a fresh fight whenever one ends.
    let policy = veldwake_combat::oracle::OraclePolicy::Read {
        lag: 24,
        walk_only: false,
        side: 1.0,
    };
    let initiative =
        veldwake_combat::oracle::initiative_oracle_setup(veldwake_combat::CombatSeed::GOLDEN);
    let fresh = || -> Result<Encounter, String> {
        let mut encounter = Encounter::new(&initiative, Some(&ground))
            .map_err(|error| format!("setup: {error}"))?;
        encounter.arm();
        Ok(encounter)
    };
    let mut encounter = fresh()?;
    let mut worst = Duration::ZERO;
    let mut stepping = Duration::ZERO;
    let mut fights = 0_u32;
    for _ in 0..ticks {
        let intent = policy.intent(&encounter);
        let tick_started = Instant::now();
        let _ = encounter.step(intent, WorldContact::ground_only(&ground));
        let spent = tick_started.elapsed();
        stepping += spent;
        worst = worst.max(spent);
        if encounter.outcome().is_some() {
            encounter = fresh()?;
            fights += 1;
        }
    }
    println!(
        "combat initiative tick over {ticks} ticks ({fights} fights): mean {:.3} us, worst {:.3} us",
        stepping.as_secs_f64() * 1.0e6 / ticks as f64,
        worst.as_secs_f64() * 1.0e6
    );

    // Ground queries, measured rather than counted by hand.
    let counting = veldwake_combat::encounter::CountingGround::new(&ground);
    let mut encounter =
        Encounter::new(&setup, Some(&counting)).map_err(|error| format!("setup: {error}"))?;
    encounter.arm();
    let before = counting.queries();
    let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&encounter));
    let sample = 1_200_u64;
    for _ in 0..sample {
        let intent = runner.next_intent(&encounter);
        let _ = encounter.step(intent, WorldContact::ground_only(&counting));
        if runner.finished() {
            runner.restart();
        }
    }
    println!(
        "ground queries: {} over {sample} ticks, {:.2} per tick",
        counting.queries() - before,
        (counting.queries() - before) as f64 / sample as f64
    );

    // The hit query alone, on a sweep that actually connects.
    let body = Capsule::new(
        Segment::new(
            glam::Vec3::new(0.0, 0.5, 0.0),
            glam::Vec3::new(0.0, 1.8, 0.0),
        ),
        0.5,
    );
    let sweep = Sweep::new(
        Segment::new(
            glam::Vec3::new(0.0, 1.2, -2.0),
            glam::Vec3::new(0.0, 1.2, -1.0),
        ),
        Segment::new(
            glam::Vec3::new(0.0, 1.2, 0.4),
            glam::Vec3::new(0.0, 1.2, 1.4),
        ),
        CHARACTER_VOXEL_SIZE,
    );
    let queries = 200_000_u64;
    let started = Instant::now();
    let mut hits = 0_u64;
    for _ in 0..queries {
        if sweep_capsule(&sweep, &body).is_some() {
            hits += 1;
        }
    }
    let elapsed = started.elapsed();
    println!(
        "hit query: {:.4} us each over {queries} sweeps at {} substeps ({hits} connected)",
        elapsed.as_secs_f64() * 1.0e6 / queries as f64,
        sweep.substeps()
    );

    // Compiling the weapon, which happens once.
    let descriptor = WeaponDescriptor::golden();
    let mut compiler = WeaponCompiler::new();
    let mut samples = Vec::with_capacity(64);
    for _ in 0..64 {
        let started = Instant::now();
        let compiled = compiler
            .compile_descriptor(&descriptor)
            .map_err(|error| format!("weapon: {error}"))?;
        samples.push(started.elapsed());
        std::hint::black_box(compiled.quad_count());
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    println!(
        "weapon compile: median {:.1} us over 64 runs (min {:.1}, max {:.1})",
        median.as_secs_f64() * 1.0e6,
        samples[0].as_secs_f64() * 1.0e6,
        samples[samples.len() - 1].as_secs_f64() * 1.0e6
    );
    Ok(())
}

/// Prints the whole exchange, tick by tick, so the adversary's transitions and
/// the timelines can be read rather than inferred from counters.
fn trace(every: Option<&str>) -> Result<(), String> {
    let stride: u64 = match every {
        Some(raw) => raw
            .trim()
            .parse()
            .map_err(|_| format!("`{raw}` is not a tick stride"))?,
        None => 12,
    };
    let (mut encounter, _, ground) = build()?;
    encounter.arm();
    let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&encounter));
    let mut previous_brain = encounter.brain().state().name();
    for _ in 0..fixture::GOLDEN_RUN_TICKS {
        let intent = runner.next_intent(&encounter);
        let events = encounter.step(intent, WorldContact::ground_only(&ground));
        let brain = encounter.brain().state().name();
        let interesting = !events.is_empty() || brain != previous_brain;
        if interesting || encounter.tick_index() % stride == 0 {
            let names: Vec<&str> = events.iter().map(|event| event.name()).collect();
            println!(
                "{}  leg {:<20} {}",
                describe(&encounter),
                runner.leg_name(),
                names.join(" ")
            );
        }
        previous_brain = brain;
        if runner.finished() {
            runner.restart();
        }
    }
    Ok(())
}

fn describe(encounter: &Encounter) -> String {
    let player = encounter.combatant(Side::Player);
    let adversary = encounter.combatant(Side::Adversary);
    format!(
        "t{:>5} d{:>5.2} player {:<9} {:>3} hp{:>4} | adversary {:<9} {:>3} hp{:>4} brain {}",
        encounter.tick_index(),
        encounter.separation_distance(),
        player.action().label(encounter.attack_spec(Side::Player)),
        player.action().elapsed(),
        player.health().current(),
        adversary
            .action()
            .label(encounter.attack_spec(Side::Adversary)),
        adversary.action().elapsed(),
        adversary.health().current(),
        encounter.brain().state().name()
    )
}
