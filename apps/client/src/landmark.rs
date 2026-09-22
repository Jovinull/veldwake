//! What the world's landmarks look like from where the game actually stands.
//!
//! The second level of M8's visibility. `veldwake-procedural` decides where a
//! landmark goes with a world proxy that knows nothing about cameras, pixels,
//! fog or framing: it measures how much silhouette a point in the world can
//! see past the terrain and the trees. That is the right question for
//! placement and the wrong one for presentation, because a silhouette clear of
//! every occluder is still worth nothing if it sits above the frame or dissolves
//! into haze.
//!
//! This module asks the presentation question, with the client's own camera,
//! projection and fog. Nothing here is a second placement rule: it cannot move
//! a landmark, only report what the real view of one is.
//!
//! **The division is exact and neither half is the whole answer.** The world
//! proxy answers occlusion: what stands between a viewer and a silhouette.
//! This level answers framing and legibility: where the silhouette lands in
//! the frame and how much of its contrast the air leaves. A landmark is
//! visible when both say so, and the test that compares them checks the
//! direction that matters — the proxy may not call something visible that the
//! camera cannot frame. Nothing here raycasts, so a framing on its own never
//! means "can be seen".

use glam::{Vec3, Vec4Swizzles};
use veldwake_character::GroundSampler;
use veldwake_procedural::{LandmarkInstance, SilhouetteClass, TerrainGenerator};

use crate::camera::Camera;
use crate::lighting::Lighting;

/// How the real view of one landmark comes out.
#[derive(Clone, Copy, Debug)]
pub struct Framing {
    /// Silhouette points tested, spread over the landmark's own voxels.
    pub samples: usize,
    /// How many of them the projection puts inside the frame.
    pub inside: usize,
    /// Normalised device `y` of the highest point inside the frame, where
    /// `-1` is the bottom edge and `+1` the top.
    pub highest: f32,
    /// And of the lowest, which is how much of the landmark is standing on
    /// visible ground rather than cropped away.
    pub lowest: f32,
    /// World units from the camera to the landmark's crown.
    pub distance: f32,
    /// The fraction of the landmark's own contrast that survives the fog at
    /// that distance. `1.0` is untouched, `0.5` is the style bible's
    /// half-contrast distance.
    pub contrast: f32,
}

impl Framing {
    /// Whether a viewer would say they can see it: enough of it is in frame
    /// and enough of its contrast survives the air.
    #[must_use]
    pub fn reads(&self, least_samples: usize, least_contrast: f32) -> bool {
        self.inside >= least_samples && self.contrast >= least_contrast
    }
}

/// How many points across each axis of the landmark the oracle projects.
///
/// A grid rather than the whole voxel set: a landmark is a few hundred voxels
/// and the question is where its silhouette lands, not how many of its cells
/// do. Eleven is odd, so the crown column itself is always one of them.
const SILHOUETTE_SAMPLES: usize = 11;

/// Projects a landmark through the real camera.
///
/// The samples are spread over the landmark's occupied columns and over the
/// height each one actually fills, so a gate is sampled around its opening and
/// a broken shaft around its stump.
#[must_use]
pub fn frame_of(camera: &Camera, instance: &LandmarkInstance) -> Framing {
    let matrix = camera.view_projection();
    let (low_x, high_x, low_z, high_z) = instance.bounds;
    let mut samples = 0_usize;
    let mut inside = 0_usize;
    let mut highest = f32::NEG_INFINITY;
    let mut lowest = f32::INFINITY;

    for step_z in 0..SILHOUETTE_SAMPLES {
        for step_x in 0..SILHOUETTE_SAMPLES {
            let x = spread(low_x, high_x, step_x);
            let z = spread(low_z, high_z, step_z);
            let Some((column_low, column_high)) = instance.column(x, z) else {
                continue;
            };
            for step_y in 0..SILHOUETTE_SAMPLES {
                let y = spread(column_low, column_high, step_y);
                samples += 1;
                #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
                let point = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                let clip = matrix * point.extend(1.0);
                if clip.w <= 0.0 {
                    continue;
                }
                let ndc = clip.xyz() / clip.w;
                if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 || ndc.z > 1.0 {
                    continue;
                }
                inside += 1;
                highest = highest.max(ndc.y);
                lowest = lowest.min(ndc.y);
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
    let crown = Vec3::new(
        instance.crown_column.0 as f32 + 0.5,
        instance.crown_top() as f32,
        instance.crown_column.1 as f32 + 0.5,
    );
    let distance = (crown - camera.position()).length();
    Framing {
        samples,
        inside,
        highest: if inside == 0 { f32::NAN } else { highest },
        lowest: if inside == 0 { f32::NAN } else { lowest },
        distance,
        contrast: surviving_contrast(distance, crown.y),
    }
}

/// The fraction of a surface's own colour that reaches the eye through the
/// clear-weather fog.
///
/// The same exponential the scene shader applies, read from the same
/// [`Lighting`] table, so this cannot drift from what is drawn. The height
/// term is the shader's, clamped the same way.
#[must_use]
pub fn surviving_contrast(distance: f32, height: f32) -> f32 {
    let lighting = Lighting::for_weather(crate::lighting::Weather::Clear);
    let height_factor =
        (-(height - lighting.fog_reference_height) * lighting.fog_height_falloff).exp();
    let optical_depth = lighting.fog_density * distance * height_factor.clamp(0.25, 2.5);
    (-optical_depth).exp()
}

/// The real follow camera, standing at a column and looking at a landmark.
///
/// Built from [`crate::camera::FollowController`] rather than from a hand-made
/// pose, because a hand-made pose is a different camera from the one the game
/// uses and would prove nothing about the game.
#[must_use]
pub fn looking_from(
    stand: (i64, i64),
    ground: &dyn GroundSampler,
    at: (i64, i64),
) -> Option<Camera> {
    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
    let (sx, sz) = (stand.0 as f32 + 0.5, stand.1 as f32 + 0.5);
    let floor = ground.surface(f64::from(sx), f64::from(sz))?;
    #[expect(clippy::cast_possible_truncation, reason = "region heights")]
    let anchor = Vec3::new(sx, floor as f32, sz);
    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
    let (tx, tz) = (at.0 as f32 + 0.5, at.1 as f32 + 0.5);
    // The yaw convention the camera uses: clockwise from `-Z`.
    let facing = (tx - sx).atan2(-(tz - sz));

    let mut camera = Camera::default();
    let mut controller = crate::camera::FollowController::behind(anchor, facing);
    let mut input = crate::input::InputState::default();
    controller.update(
        &mut camera,
        &mut input,
        anchor,
        Vec3::ZERO,
        std::time::Duration::from_millis(16),
        Some(ground),
    );
    Some(camera)
}

/// The camera poses M8's visual evidence is taken from.
///
/// Derived from the plan rather than written down: a pose whose coordinates
/// were typed in would photograph wherever the landmarks used to be. The
/// names are fixed so the capture script and the milestone document can refer
/// to them, and every one of them stands where the game would put a player's
/// camera, not at an arbitrary point in the air.
///
/// - `landmark-first-choice-a` and `-b`: the overlook, turned to each of the
///   two landmarks a player can see from where the session begins.
/// - `landmark-reveal`: standing at the landmark that reveals the third,
///   turned to the third.
/// - `landmark-gate`: close to the gate, square on to its opening, which is
///   the shot that shows whether a gate reads as something to walk through.
#[must_use]
pub fn spawn_landmark_camera(name: &str, generator: &TerrainGenerator) -> Option<Camera> {
    let plan = generator.landmarks();
    let ground = crate::character::TerrainGround::new(generator);
    let overlook = plan.overlook().column;
    let first: Vec<&LandmarkInstance> = plan
        .instances()
        .iter()
        .filter(|one| one.role == veldwake_procedural::LandmarkRole::FirstChoice)
        .collect();
    let revealed = plan
        .instances()
        .iter()
        .find(|one| one.role == veldwake_procedural::LandmarkRole::Revealed);
    match name {
        "landmark-first-choice-a" => looking_from(overlook, &ground, first.first()?.crown_column),
        "landmark-first-choice-b" => looking_from(overlook, &ground, first.get(1)?.crown_column),
        "landmark-reveal" => {
            let revealed = revealed?;
            let from = plan.instances()[revealed.revealed_from?].crown_column;
            looking_from(from, &ground, revealed.crown_column)
        }
        "landmark-gate" => {
            let gate = plan
                .instances()
                .iter()
                .find(|one| one.descriptor.class == SilhouetteClass::Gate)?;
            let (low_x, high_x, low_z, high_z) = gate.bounds;
            // Square on to the opening, one body-length back from the
            // footprint on the axis the gate does not span.
            let (centre_x, centre_z) = ((low_x + high_x) / 2, (low_z + high_z) / 2);
            let stand = if (high_x - low_x) < (high_z - low_z) {
                (low_x - 10, centre_z)
            } else {
                (centre_x, low_z - 10)
            };
            looking_from(stand, &ground, (centre_x, centre_z))
        }
        _ => None,
    }
}

/// Where a landmark's crown lands on the vertical axis of the frame.
///
/// `1.0` is the top edge, so a value above one is a crown the default camera
/// pitch leaves off the screen. Reported rather than clamped, because how far
/// above the edge it is is the measurement.
#[must_use]
pub fn crown_height_in_frame(camera: &Camera, instance: &LandmarkInstance) -> Option<f32> {
    #[expect(clippy::cast_precision_loss, reason = "region coordinates")]
    let crown = Vec3::new(
        instance.crown_column.0 as f32 + 0.5,
        instance.crown_top() as f32,
        instance.crown_column.1 as f32 + 0.5,
    );
    let clip = camera.view_projection() * crown.extend(1.0);
    (clip.w > 0.0).then(|| clip.y / clip.w)
}

/// The `step`-th of [`SILHOUETTE_SAMPLES`] points spread over `low..=high`.
///
/// Inclusive of both ends, so an edge of the silhouette is always sampled,
/// and collapsing to `low` for a one-voxel span rather than dividing by zero.
fn spread(low: i64, high: i64, step: usize) -> i64 {
    let span = high - low;
    if span <= 0 {
        return low;
    }
    let last = i64::try_from(SILHOUETTE_SAMPLES.saturating_sub(1))
        .unwrap_or(1)
        .max(1);
    let index = i64::try_from(step).unwrap_or(0).min(last);
    low + (span * index + last / 2) / last
}

#[cfg(test)]
mod tests {
    use super::{Framing, crown_height_in_frame, frame_of, looking_from, surviving_contrast};
    use crate::character::TerrainGround;
    use crate::lighting::Lighting;
    use veldwake_procedural::{LandmarkRole, TerrainGenerator};

    /// Enough of a silhouette to be a thing on the horizon rather than a
    /// speck. The world proxy's own floor is eight voxels; this counts
    /// projected samples, so it is a different number about the same idea.
    const READS: usize = 12;
    /// Half the contrast is the style bible's own threshold for "far away but
    /// still legible", quoted at 320 units.
    const LEGIBLE: f32 = 0.5;

    fn world() -> TerrainGenerator {
        TerrainGenerator::golden()
    }

    #[test]
    fn both_first_choices_are_in_frame_from_the_overlook() {
        // The milestone's first claim: a player who stands where the session
        // begins and turns around sees two places worth walking to.
        let generator = world();
        let ground = TerrainGround::new(&generator);
        let plan = generator.landmarks();
        let overlook = plan.overlook().column;
        for instance in plan.instances() {
            if instance.role != LandmarkRole::FirstChoice {
                continue;
            }
            let Some(camera) = looking_from(overlook, &ground, instance.crown_column) else {
                panic!("the overlook has no ground");
            };
            let framing: Framing = frame_of(&camera, instance);
            assert!(
                framing.reads(READS, LEGIBLE),
                "landmark {} reads as {framing:?} from the overlook",
                instance.index
            );
            assert!(
                framing.highest > -0.2,
                "landmark {} tops out at {} in the frame, which is under the player's feet",
                instance.index,
                framing.highest
            );
        }
    }

    #[test]
    fn the_reveal_is_framed_and_legible_from_what_reveals_it() {
        // Only half of "revealed" is this level's business. That the overlook
        // cannot see it is occlusion, which the world proxy measures and
        // `the_third_landmark_is_a_reveal_and_not_a_third_option` asserts;
        // projecting it from the overlook would put it in frame, because a
        // projection does not know a hill is in the way. What this level owes
        // the milestone is the other half: when the player is standing at the
        // landmark that reveals it, the thing is in the frame and readable.
        let generator = world();
        let ground = TerrainGround::new(&generator);
        let plan = generator.landmarks();
        let Some(revealed) = plan
            .instances()
            .iter()
            .find(|one| one.role == LandmarkRole::Revealed)
        else {
            panic!("the composition has no reveal");
        };
        let revealer = plan.instances()[revealed.revealed_from.unwrap_or_default()].crown_column;
        let Some(camera) = looking_from(revealer, &ground, revealed.crown_column) else {
            panic!("the revealing landmark has no ground beside it");
        };
        let framing = frame_of(&camera, revealed);
        assert!(
            framing.reads(READS, LEGIBLE),
            "the reveal reads as {framing:?} from what is supposed to reveal it"
        );
        assert!(
            framing.samples > framing.inside / 2,
            "the sampling collapsed: {framing:?}"
        );
        assert!(
            framing.distance <= 170.0,
            "the reveal stands {} units away, past the sight limit the composition caps at",
            framing.distance
        );
        assert!(
            framing.lowest > -1.0 && framing.highest > framing.lowest,
            "the reveal is a line rather than a silhouette: {framing:?}"
        );
    }

    #[test]
    fn the_world_proxy_and_the_presentation_agree_about_what_is_visible() {
        // Two levels, one answer. The proxy may be conservative — it knows
        // nothing about the frame — but it may not call visible what the
        // camera cannot see, because that is how a landmark ends up placed
        // somewhere nobody will ever look at it.
        let generator = world();
        let ground = TerrainGround::new(&generator);
        let plan = generator.landmarks();
        for instance in plan.instances() {
            let from = match instance.revealed_from {
                Some(revealer) => plan.instances()[revealer].crown_column,
                None => plan.overlook().column,
            };
            let Some(camera) = looking_from(from, &ground, instance.crown_column) else {
                panic!("nowhere to stand to look at landmark {}", instance.index);
            };
            let framing = frame_of(&camera, instance);
            assert!(
                instance.visible.visible >= 8.0,
                "the plan kept landmark {} on {:.1} visible voxels",
                instance.index,
                instance.visible.visible
            );
            assert!(
                framing.inside > 0,
                "the world proxy calls landmark {} visible and the camera cannot see it: {framing:?}",
                instance.index
            );
        }
    }

    #[test]
    fn the_fog_leaves_a_landmark_legible_at_the_distance_it_stands() {
        // The composition caps a sight line at 170 units. This is what the
        // shader does to a surface that far away, computed from the same
        // table the renderer uploads.
        assert!(surviving_contrast(0.0, 20.0) > 0.99);
        let near = surviving_contrast(60.0, 45.0);
        let far = surviving_contrast(170.0, 45.0);
        assert!(near > far, "fog does not thicken with distance");
        assert!(
            far > 0.6,
            "a landmark at the sight limit keeps only {far} of its contrast"
        );
        // And the half-contrast distance is where the style bible puts it.
        let half = surviving_contrast(320.0, 18.0);
        assert!(
            (half - 0.5).abs() < 0.02,
            "half contrast landed at {half} instead of 0.5"
        );
    }

    #[test]
    fn the_crown_of_a_first_choice_stands_above_the_default_frame() {
        // Measured, and then kept. At the distance the composition chose, a
        // landmark is a tower rising out of the top of the screen rather than
        // a complete little shape inside it. Pushing the pair out until the
        // crown fitted was tried and captured: at 87 and 96 units the canopy
        // leaves 8.3 and 11.5 visible voxels instead of 21.3 and 22.0, and the
        // shot is worse. A player who wants the top looks up; this test exists
        // so the next agent knows that is a decision and not an oversight.
        let generator = world();
        let ground = TerrainGround::new(&generator);
        let plan = generator.landmarks();
        let overlook = plan.overlook().column;
        let mut cropped = 0_u32;
        for instance in plan.instances() {
            if instance.role != LandmarkRole::FirstChoice {
                continue;
            }
            let Some(camera) = looking_from(overlook, &ground, instance.crown_column) else {
                panic!("the overlook has no ground");
            };
            let Some(crown) = crown_height_in_frame(&camera, instance) else {
                panic!("landmark {} is behind the camera", instance.index);
            };
            let framing = frame_of(&camera, instance);
            assert!(
                framing.inside >= READS,
                "landmark {} shows {} samples, which is not a silhouette",
                instance.index,
                framing.inside
            );
            if crown > 1.0 {
                cropped += 1;
            }
        }
        assert_eq!(
            cropped, 2,
            "the number of first choices whose crown sits above the frame moved;              re-measure and say in the milestone what the new composition looks like"
        );
    }

    #[test]
    fn a_crown_stays_darker_than_the_sky_behind_it_after_the_fog() {
        // `LANDMARK_STYLE.md` puts this measurement in the client on purpose:
        // the world generator has no fog and no sky, so "is the top still
        // dark enough to read against the sky" can only be answered here.
        // The rule is at least `0.20` of luminance between the fogged crown
        // and the palest sky it could stand against.
        let generator = world();
        let ground = TerrainGround::new(&generator);
        let plan = generator.landmarks();
        let lighting = Lighting::for_weather(crate::lighting::Weather::Clear);
        let fog = luminance(lighting.fog_color);
        let sky = luminance(lighting.sky_horizon).max(luminance(lighting.sky_zenith));
        let crown_albedo = luminance(veldwake_procedural::LandmarkMaterial::Band.albedo());
        for instance in plan.instances() {
            let from = match instance.revealed_from {
                Some(revealer) => plan.instances()[revealer].crown_column,
                None => plan.overlook().column,
            };
            let Some(camera) = looking_from(from, &ground, instance.crown_column) else {
                panic!("nowhere to stand to look at landmark {}", instance.index);
            };
            let framing = frame_of(&camera, instance);
            // What the eye receives: the surface's own value through the fog,
            // plus the fog's own colour filling in the rest.
            let apparent = crown_albedo.mul_add(framing.contrast, fog * (1.0 - framing.contrast));
            assert!(
                sky - apparent >= 0.20,
                "landmark {} reads at {apparent:.3} against a sky of {sky:.3} at {:.0} units",
                instance.index,
                framing.distance
            );
        }
    }

    fn luminance([r, g, b]: [f32; 3]) -> f32 {
        0.2126_f32.mul_add(r, 0.7152_f32.mul_add(g, 0.0722 * b))
    }
}
