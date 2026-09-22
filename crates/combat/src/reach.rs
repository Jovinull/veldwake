//! Where a swing's blade actually goes, measured from compiled geometry.
//!
//! **This is production code, not a fixture.** It was a measuring tool in
//! `fixture.rs` for as long as the only question it answered was "what should
//! the strike range and the dodge distance be?". M9 gives a combatant a choice
//! of weapon, and the moment the *rules* have to know how far the held weapon
//! reaches — [`crate::encounter::Encounter`] derives its aim-assist range from
//! exactly this number — a second implementation of the formula becomes the
//! thing that drifts. So the primitive lives here, the encounter calls it, and
//! `fixture` and `combat-probe` call the same function rather than a copy.
//!
//! What it is: a pure function of a compiled body, a compiled weapon and an
//! attack spec. It poses the body through every tick of one swing with the
//! same [`pose_with`] path the fight uses, and reads where the blade went.
//!
//! What it is not: it has no world, no terrain, no client, no camera and no
//! encounter. It does not know a fight is happening, and it answers the same
//! way whether or not one is.

use glam::Vec2;

use veldwake_character::skeleton::{BoneId, Side as BodySide};
use veldwake_character::{CharacterState, CompiledCharacter, pose_with};

use crate::combatant::{Action, SIDES, SwingId};
use crate::spec::AttackSpec;
use crate::weapon::CompiledWeapon;

/// Where a body's blade goes during its own swing, measured rather than
/// declared.
///
/// Every number is in world units and is relative to the body's own centre and
/// its own ground, because the swing is measured on a body standing at the
/// origin of a flat world with a default state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackEnvelope {
    /// Furthest the tip gets from the body's own centre, at any point of the
    /// action — windup and recovery included.
    pub max_tip_reach: f32,
    /// Reach of the tip while the blade can connect.
    pub active_reach: (f32, f32),
    /// Height of the tip above the ground while the blade can connect.
    pub active_height: (f32, f32),
}

impl AttackEnvelope {
    /// Centre-to-centre distance at which this swing can still touch a body of
    /// the given capsule radius.
    ///
    /// The furthest the tip gets while the blade is live, plus the radius of
    /// the thing it is swung at. This is the number an aim-assist range and a
    /// strike range are both chosen against, and it is why a longer blade does
    /// not need any constant to be edited before it reaches further.
    #[must_use]
    pub fn connects_out_to(&self, target_radius: f32) -> f32 {
        self.active_reach.1 + target_radius
    }
}

/// Measures one body's attack envelope with one weapon under one spec.
///
/// Poses the body at every tick from `0` to `spec.total()` inclusive through
/// the same action overlay and the same `pose_with` the encounter uses, so the
/// answer cannot disagree with the swing the fight actually performs.
#[must_use]
pub fn attack_envelope(
    character: &CompiledCharacter,
    weapon: &CompiledWeapon,
    spec: &AttackSpec,
    weapon_side: BodySide,
) -> AttackEnvelope {
    let mut max_tip_reach = 0.0_f32;
    let mut active_reach = (f32::INFINITY, f32::NEG_INFINITY);
    let mut active_height = (f32::INFINITY, f32::NEG_INFINITY);
    let state = CharacterState::default();
    for elapsed in 0..=spec.total() {
        let action = Action::Attack {
            swing: SwingId::first(),
            elapsed,
            hits: [false; SIDES.len()],
        };
        let overlay = action.overlay(spec, weapon_side, state.facing);
        let posed = pose_with(character, &state, None, Some(&overlay));
        let blade = weapon.blade_world(
            posed.world_matrix(),
            posed.bone_world()[BoneId::HandR.index()],
        );
        let reach = Vec2::new(blade.tip.x, blade.tip.z).length();
        max_tip_reach = max_tip_reach.max(reach);
        if spec.is_active(elapsed) {
            active_reach = (active_reach.0.min(reach), active_reach.1.max(reach));
            active_height = (
                active_height.0.min(blade.tip.y),
                active_height.1.max(blade.tip.y),
            );
        }
    }
    AttackEnvelope {
        max_tip_reach,
        active_reach,
        active_height,
    }
}

#[cfg(test)]
mod tests {
    use super::attack_envelope;
    use crate::fixture;
    use crate::weapon::WeaponCompiler;
    use veldwake_character::CharacterCompiler;
    use veldwake_character::skeleton::Side as BodySide;

    #[test]
    fn an_envelope_is_a_pure_function_of_its_three_inputs() {
        let mut compiler = CharacterCompiler::new();
        let Ok(body) = compiler.compile_descriptor(&fixture::player_descriptor()) else {
            panic!("the player fixture must compile");
        };
        let Ok(weapon) = WeaponCompiler::new().compile_descriptor(&fixture::weapon_descriptor())
        else {
            panic!("the weapon fixture must compile");
        };
        let Ok(spec) = fixture::player_attack().compile() else {
            panic!("the player attack must compile");
        };
        let first = attack_envelope(&body, &weapon, &spec, BodySide::Right);
        let second = attack_envelope(&body, &weapon, &spec, BodySide::Right);
        assert_eq!(first, second, "two measurements of one swing disagree");
    }

    #[test]
    fn the_active_band_is_inside_the_whole_action_and_reaches_forward() {
        let mut compiler = CharacterCompiler::new();
        let Ok(body) = compiler.compile_descriptor(&fixture::player_descriptor()) else {
            panic!("the player fixture must compile");
        };
        let Ok(weapon) = WeaponCompiler::new().compile_descriptor(&fixture::weapon_descriptor())
        else {
            panic!("the weapon fixture must compile");
        };
        let Ok(spec) = fixture::player_attack().compile() else {
            panic!("the player attack must compile");
        };
        let envelope = attack_envelope(&body, &weapon, &spec, BodySide::Right);
        assert!(
            envelope.active_reach.0 <= envelope.active_reach.1,
            "the active band is inverted"
        );
        assert!(
            envelope.active_reach.1 <= envelope.max_tip_reach,
            "the active window reaches past the whole action"
        );
        assert!(
            envelope.active_height.0 > 0.0,
            "the blade passes through the ground during its live window"
        );
        // A target with a radius reaches further than a point target, by
        // exactly that radius. The relation, not the number.
        let point = envelope.connects_out_to(0.0);
        let bodied = envelope.connects_out_to(0.5);
        assert!((bodied - point - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn a_longer_blade_reaches_further_with_the_same_body_and_spec() {
        let mut compiler = CharacterCompiler::new();
        let Ok(body) = compiler.compile_descriptor(&fixture::player_descriptor()) else {
            panic!("the player fixture must compile");
        };
        let Ok(original) = WeaponCompiler::new().compile_descriptor(&fixture::weapon_descriptor())
        else {
            panic!("the weapon fixture must compile");
        };
        let Ok(found) =
            WeaponCompiler::new().compile_descriptor(&fixture::found_weapon_descriptor())
        else {
            panic!("the found weapon must compile");
        };
        let Ok(spec) = fixture::player_attack().compile() else {
            panic!("the player attack must compile");
        };
        let short = attack_envelope(&body, &original, &spec, BodySide::Right);
        let long = attack_envelope(&body, &found, &spec, BodySide::Right);
        assert!(
            long.active_reach.1 > short.active_reach.1,
            "the found blade does not reach further: {} against {}",
            long.active_reach.1,
            short.active_reach.1
        );
    }
}
