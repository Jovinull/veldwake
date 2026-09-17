//! The style bible expressed as numbers the renderer can upload.
//!
//! Every constant here is transcribed from `docs/audiovisual/STYLE_BIBLE.md`.
//! Nothing in this module invents a value, and nothing in the shaders invents
//! one either: a shader reads what this module uploads. That is what makes the
//! art direction reviewable as a document rather than as scattered literals.
//!
//! The module is deliberately free of GPU types so the shadow fitting and the
//! weather table can be tested headlessly.

use glam::{Mat4, Vec3};

/// Sun azimuth in degrees, measured clockwise from world north. World north is
/// `-Z`, which is the direction the camera faces at zero yaw.
const SUN_AZIMUTH_DEGREES: f32 = -38.0;
/// Sun elevation in degrees above the horizon.
const SUN_ELEVATION_DEGREES: f32 = 34.0;

/// Distance at which clear-weather fog leaves terrain about half its contrast.
const FOG_HALF_CONTRAST_DISTANCE: f32 = 320.0;
/// How fast fog thins with altitude, per voxel. Valley air reads thicker than
/// ridge air, which is the style bible's height component.
const FOG_HEIGHT_FALLOFF: f32 = 0.018;
/// Altitude at which the fog density above is the quoted value. The valley
/// floor, so the quoted half-contrast distance is the one a player standing in
/// the meadow actually sees.
const FOG_REFERENCE_HEIGHT: f32 = 18.0;

/// Hard lower bound on a shadowed surface, as a fraction of its lit value.
///
/// Ambient normally sits above this, and the style bible's "roughly a quarter
/// of the lit value" is what ambient produces on the golden palette. The bound
/// exists so a material dark enough for ambient to vanish still reads as a
/// surface rather than as a hole.
const SHADOW_FLOOR: f32 = 0.12;

/// The two weather states this slice has. Not a simulation: a switch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Weather {
    #[default]
    Clear,
    Overcast,
}

impl Weather {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Overcast => "overcast",
        }
    }

    /// The next state in the diagnostic toggle order.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Clear => Self::Overcast,
            Self::Overcast => Self::Clear,
        }
    }
}

/// Every lighting value one weather state implies.
#[derive(Clone, Copy, Debug)]
pub struct Lighting {
    /// Unit vector from a surface toward the sun.
    pub sun_direction: Vec3,
    pub sun_color: [f32; 3],
    pub sun_intensity: f32,
    /// Ambient arriving from above, cool.
    pub sky_ambient: [f32; 3],
    /// Ambient bounced from below, warm and dim.
    pub ground_bounce: [f32; 3],
    pub ambient_intensity: f32,
    pub sky_zenith: [f32; 3],
    pub sky_mid: [f32; 3],
    pub sky_horizon: [f32; 3],
    pub fog_color: [f32; 3],
    /// Extinction per voxel at the reference height.
    pub fog_density: f32,
    /// Per-voxel falloff of fog density with altitude.
    pub fog_height_falloff: f32,
    /// Height the density is quoted at.
    pub fog_reference_height: f32,
    /// How much of its specular response water keeps.
    pub water_specular: f32,
    /// Fraction of the lit value a shadowed surface keeps.
    pub shadow_floor: f32,
}

impl Lighting {
    /// The style bible's weather table, verbatim.
    #[must_use]
    pub fn for_weather(weather: Weather) -> Self {
        let base_density = std::f32::consts::LN_2 / FOG_HALF_CONTRAST_DISTANCE;
        let common = Self {
            sun_direction: sun_direction(),
            sun_color: [1.000, 0.945, 0.827],
            sun_intensity: 1.00,
            sky_ambient: [0.352, 0.443, 0.561],
            ground_bounce: [0.208, 0.196, 0.157],
            ambient_intensity: 0.38,
            sky_zenith: [0.243, 0.408, 0.667],
            sky_mid: [0.435, 0.600, 0.796],
            sky_horizon: [0.647, 0.741, 0.824],
            fog_color: [0.588, 0.690, 0.784],
            fog_density: base_density,
            fog_height_falloff: FOG_HEIGHT_FALLOFF,
            fog_reference_height: FOG_REFERENCE_HEIGHT,
            water_specular: 1.0,
            shadow_floor: SHADOW_FLOOR,
        };
        match weather {
            Weather::Clear => common,
            Weather::Overcast => Self {
                sun_color: [0.820, 0.843, 0.878],
                sun_intensity: 0.42,
                ambient_intensity: 0.62,
                sky_zenith: [0.404, 0.435, 0.482],
                // The style bible gives zenith and horizon for overcast; the
                // mid stop is their midpoint, so the gradient stays a three-stop
                // curve without inventing a fourth colour.
                sky_mid: midpoint([0.404, 0.435, 0.482], [0.639, 0.659, 0.690]),
                sky_horizon: [0.639, 0.659, 0.690],
                fog_color: [0.612, 0.635, 0.667],
                fog_density: base_density * 1.85,
                water_specular: 0.5,
                ..common
            },
        }
    }
}

fn midpoint(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        f32::midpoint(a[0], b[0]),
        f32::midpoint(a[1], b[1]),
        f32::midpoint(a[2], b[2]),
    ]
}

/// Unit vector from a surface toward the sun.
#[must_use]
pub fn sun_direction() -> Vec3 {
    let azimuth = SUN_AZIMUTH_DEGREES.to_radians();
    let elevation = SUN_ELEVATION_DEGREES.to_radians();
    let horizontal = elevation.cos();
    Vec3::new(
        azimuth.sin() * horizontal,
        elevation.sin(),
        -azimuth.cos() * horizontal,
    )
    .normalize()
}

/// Edge of the square shadow map, in texels.
pub const SHADOW_MAP_EDGE: u32 = 2048;
/// Half-edge of the world box the shadow map covers, in voxels.
///
/// One cascade. Large enough that a ridge casts onto the valley floor within
/// the readable foreground and midground; small enough that 2048 texels give
/// roughly nine texels per voxel edge, which is what makes the filter kernel a
/// soft edge rather than a blur.
pub const SHADOW_RADIUS: f32 = 112.0;
/// How far ahead of the camera the shadow box is centred. The camera looks
/// forward, so centring the box on the camera itself would spend half of it
/// behind the viewer.
const SHADOW_LEAD: f32 = 0.55;

/// Centre of the shadow box for a camera.
#[must_use]
pub fn shadow_centre(camera_position: Vec3, camera_forward: Vec3) -> Vec3 {
    camera_position + camera_forward * (SHADOW_RADIUS * SHADOW_LEAD)
}

/// The light's view-projection matrix for one frame.
///
/// Orthographic, because a directional light has no position. The centre is
/// snapped to a whole shadow texel: without that, moving the camera slides the
/// sampling grid under every shadow edge and the edges crawl, which is the
/// shimmering the review rejects.
#[must_use]
pub fn shadow_view_projection(centre: Vec3, sun: Vec3) -> Mat4 {
    let up = if sun.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let eye = centre + sun * (SHADOW_RADIUS * 2.0);
    let view = glam::camera::rh::view::look_at_mat4(eye, centre, up);

    let texel = (2.0 * SHADOW_RADIUS) / SHADOW_MAP_EDGE as f32;
    let in_light = view.transform_point3(centre);
    let snap = Vec3::new(
        (in_light.x / texel).floor().mul_add(texel, -in_light.x),
        (in_light.y / texel).floor().mul_add(texel, -in_light.y),
        0.0,
    );
    let snapped_view = Mat4::from_translation(snap) * view;

    let projection = glam::camera::rh::proj::directx::orthographic(
        -SHADOW_RADIUS,
        SHADOW_RADIUS,
        -SHADOW_RADIUS,
        SHADOW_RADIUS,
        0.0,
        SHADOW_RADIUS * 4.0,
    );
    projection * snapped_view
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-4;

    #[test]
    fn the_sun_is_where_the_style_bible_puts_it() {
        let sun = sun_direction();
        assert!((sun.length() - 1.0).abs() < EPSILON, "not a unit vector");
        // Elevation 34 degrees above the horizon.
        assert!((sun.y - SUN_ELEVATION_DEGREES.to_radians().sin()).abs() < EPSILON);
        // Azimuth -38 degrees from north puts the sun west of north: negative x
        // and negative z, since north is -Z.
        assert!(sun.x < 0.0, "the sun is east of north: {sun:?}");
        assert!(sun.z < 0.0, "the sun is south of the viewer: {sun:?}");
        // High enough to light a valley floor, low enough to cast a long shadow.
        assert!((0.4..0.7).contains(&sun.y));
    }

    #[test]
    fn overcast_lowers_the_key_and_raises_the_fill() {
        let clear = Lighting::for_weather(Weather::Clear);
        let overcast = Lighting::for_weather(Weather::Overcast);

        assert!(overcast.sun_intensity < clear.sun_intensity * 0.6);
        assert!(overcast.ambient_intensity > clear.ambient_intensity);
        assert!(overcast.fog_density > clear.fog_density);
        assert!(overcast.water_specular < clear.water_specular);
        // "Overcast must remain readable": total light must not collapse.
        let total = |lighting: &Lighting| lighting.sun_intensity + lighting.ambient_intensity;
        assert!(
            total(&overcast) > total(&clear) * 0.65,
            "overcast is too dark to read"
        );
        // Both states keep the same sun position: this is weather, not time.
        assert_eq!(clear.sun_direction, overcast.sun_direction);
    }

    #[test]
    fn the_sky_gradient_darkens_upward_in_both_states() {
        for weather in [Weather::Clear, Weather::Overcast] {
            let lighting = Lighting::for_weather(weather);
            let value = |color: [f32; 3]| {
                color[1].mul_add(0.7152, color[0].mul_add(0.2126, color[2] * 0.0722))
            };
            let zenith = value(lighting.sky_zenith);
            let mid = value(lighting.sky_mid);
            let horizon = value(lighting.sky_horizon);
            assert!(
                zenith < mid && mid < horizon,
                "{} sky is not a monotone gradient: {zenith} {mid} {horizon}",
                weather.name()
            );
            // "Fog colour must match the sky near the horizon."
            let fog = value(lighting.fog_color);
            assert!(
                (fog - horizon).abs() < 0.08,
                "{} fog and horizon disagree: {fog} against {horizon}",
                weather.name()
            );
        }
    }

    #[test]
    fn fog_leaves_half_the_contrast_at_the_quoted_distance() {
        let clear = Lighting::for_weather(Weather::Clear);
        let remaining = (-clear.fog_density * FOG_HALF_CONTRAST_DISTANCE).exp();
        assert!((remaining - 0.5).abs() < 0.01, "contrast left: {remaining}");
        // Fog must not swallow the foreground.
        assert!((-clear.fog_density * 48.0).exp() > 0.85);
    }

    #[test]
    fn the_weather_toggle_is_a_cycle() {
        assert_eq!(Weather::default(), Weather::Clear);
        assert_eq!(Weather::Clear.next(), Weather::Overcast);
        assert_eq!(Weather::Overcast.next(), Weather::Clear);
        assert_ne!(Weather::Clear.name(), Weather::Overcast.name());
    }

    #[test]
    fn the_shadow_box_leads_the_camera_and_holds_it() {
        let position = Vec3::new(10.0, 30.0, -5.0);
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let centre = shadow_centre(position, forward);
        assert!(centre.z < position.z, "the box is behind the camera");
        assert!(
            (centre - position).length() < SHADOW_RADIUS,
            "the camera falls outside its own shadow box"
        );
    }

    #[test]
    fn the_shadow_projection_contains_the_box_it_claims_to() {
        let sun = sun_direction();
        let centre = Vec3::new(4.0, 20.0, -12.0);
        let matrix = shadow_view_projection(centre, sun);
        for corner in [
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(-1.0, 1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(-1.0, -1.0, 1.0),
        ] {
            // A point well inside the box must land inside the unit cube, with
            // depth in the zero-to-one range wgpu uses.
            let point = centre + corner * (SHADOW_RADIUS * 0.5);
            let clip = matrix * point.extend(1.0);
            assert!((-1.0..=1.0).contains(&clip.x), "x out of range: {clip:?}");
            assert!((-1.0..=1.0).contains(&clip.y), "y out of range: {clip:?}");
            assert!((0.0..=1.0).contains(&clip.z), "z out of range: {clip:?}");
        }
    }

    #[test]
    fn the_shadow_centre_snaps_so_edges_do_not_crawl() {
        let sun = sun_direction();
        let texel = (2.0 * SHADOW_RADIUS) / SHADOW_MAP_EDGE as f32;
        let base = Vec3::new(0.0, 20.0, 0.0);
        // A sub-texel camera move must not move the sampling grid at all.
        let first = shadow_view_projection(base, sun);
        let nudged = shadow_view_projection(base + Vec3::X * (texel * 0.1), sun);
        let moved: f32 = first
            .to_cols_array()
            .iter()
            .zip(nudged.to_cols_array())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(moved < texel, "a sub-texel move shifted the map: {moved}");

        // A large move must move it, or the shadow box would never follow.
        let far = shadow_view_projection(base + Vec3::X * 50.0, sun);
        let shifted: f32 = first
            .to_cols_array()
            .iter()
            .zip(far.to_cols_array())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(shifted > texel, "the shadow box never follows the camera");
    }

    #[test]
    fn the_shadow_map_gives_more_than_one_texel_per_voxel() {
        // Below one texel per voxel the filter kernel is a blur, not an edge.
        let texels_per_voxel = SHADOW_MAP_EDGE as f32 / (2.0 * SHADOW_RADIUS);
        assert!(
            texels_per_voxel > 4.0,
            "{texels_per_voxel} texels per voxel"
        );
    }
}
