// The one stylized shading model, shared by everything lit.
//
// WGSL has no include directive, so host pipeline construction prepends this
// source, after `scene.wgsl`, to every shader that shades a surface. It exists
// so that terrain and characters cannot drift apart: there is one sun, one
// ambient model, one shadow filter, one specular lobe, and one fog, and adding
// a second content domain does not add a second copy of any of them.
//
// Deliberately not physically based. Every constant that carries art intent
// comes from the scene uniform, which `lighting.rs` fills from the style bible;
// what lives here is the shading model, not the palette.

@group(0) @binding(0)
var<uniform> scene: SceneUniform;
@group(0) @binding(1)
var shadow_map: texture_depth_2d;
@group(0) @binding(2)
var shadow_sampler: sampler_comparison;

// Tightness of the specular lobe. High enough that only water and wet rock
// catch the sun, which is the cue the style bible asks water to carry.
const SPECULAR_EXPONENT = 64.0;
// Depth bias floor and slope scale, in light-space depth units.
const SHADOW_MIN_BIAS = 0.00035;
const SHADOW_SLOPE_BIAS = 0.0022;

/// The procedural sky in one direction. `SKY_MID_HEIGHT` is defined in the
/// shared scene source, so fog resolves to the same gradient break as the sky.
fn sky_color(direction: vec3<f32>) -> vec3<f32> {
    let up = clamp(direction.y, 0.0, 1.0);
    var base: vec3<f32>;
    if up < SKY_MID_HEIGHT {
        base = mix(
            scene.sky_horizon.rgb,
            scene.sky_mid.rgb,
            smoothstep(0.0, SKY_MID_HEIGHT, up),
        );
    } else {
        base = mix(
            scene.sky_mid.rgb,
            scene.sky_zenith.rgb,
            smoothstep(SKY_MID_HEIGHT, 1.0, up),
        );
    }
    return base;
}

/// How much sun reaches a point: one where fully lit, zero where fully
/// shadowed.
///
/// `normal_offset` is how far along its own normal the sample point is pushed
/// before the lookup, in world units. It is a per-domain value rather than a
/// constant because the shadow map's texel is about one tenth of a world unit,
/// which is under a terrain voxel and over a character limb: the offset that
/// keeps terrain free of acne would move a character's sample clean off its own
/// body.
fn sun_visibility(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    ndl: f32,
    normal_offset: f32,
) -> f32 {
    // A back-facing surface needs no map lookup; it is in its own shadow.
    if ndl <= 0.0 {
        return 0.0;
    }
    let offset = world_position + normal * normal_offset;
    let light_clip = scene.light_view_projection * vec4<f32>(offset, 1.0);
    if light_clip.w <= 0.0 {
        return 1.0;
    }
    let projected = light_clip.xyz / light_clip.w;
    let uv = vec2<f32>(projected.x * 0.5 + 0.5, projected.y * -0.5 + 0.5);
    // Outside the single cascade the world is lit rather than dark: a shadow
    // box edge that darkens everything beyond it is worse than no shadow.
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || projected.z > 1.0 {
        return 1.0;
    }

    let bias = max(SHADOW_SLOPE_BIAS * (1.0 - ndl), SHADOW_MIN_BIAS);
    let reference = projected.z - bias;
    let texel = scene.params.z;
    // A fixed three-by-three kernel: a soft edge, not a blur.
    var total = 0.0;
    for (var row = -1; row <= 1; row = row + 1) {
        for (var column = -1; column <= 1; column = column + 1) {
            let tap = uv + vec2<f32>(f32(column), f32(row)) * texel;
            total = total + textureSampleCompareLevel(
                shadow_map,
                shadow_sampler,
                tap,
                reference,
            );
        }
    }
    return total / 9.0;
}

/// The whole lit-surface model: sun, hemispheric ambient, filtered shadow,
/// voxel form separation, restrained specular, and distance fog.
fn shade_surface(
    albedo: vec3<f32>,
    surface_normal: vec3<f32>,
    world_position: vec3<f32>,
    specular: f32,
    shadow_normal_offset: f32,
) -> vec3<f32> {
    let normal = normalize(surface_normal);
    let to_camera_vector = scene.camera_position.xyz - world_position;
    let distance_to_camera = length(to_camera_vector);
    let view_direction = to_camera_vector / max(distance_to_camera, 0.0001);
    let sun_direction = scene.sun.xyz;
    let ndl = max(dot(normal, sun_direction), 0.0);

    // Hemispheric ambient: cool from the sky, warm and dim from the ground.
    // A single grey would make every shadowed face read as dead.
    let upness = normal.y * 0.5 + 0.5;
    let ambient = mix(scene.ground_bounce.rgb, scene.sky_ambient.rgb, upness)
        * scene.sky_ambient.a;
    let sun = scene.sun_color.rgb * scene.sun.w * ndl;

    // "Shadowed surfaces keep ambient, and never darken to zero." Ambient is
    // the floor, which is why a shadow here is a cool blue-green rather than a
    // hole. `sun_color.a` is a hard lower bound under it, not the rule: it only
    // matters for a material dark enough that even ambient would vanish.
    let visibility = sun_visibility(world_position, normal, ndl, shadow_normal_offset);
    let lit = sun + ambient;
    let shadowed = max(ambient, lit * scene.sun_color.a);
    let shading = mix(shadowed, lit, visibility);

    // Normal-based value separation: a small, fixed offset per horizontal axis
    // so a voxel form still reads when the sun is behind it. Kept well inside
    // the style bible's within-band contrast limit.
    let form = 1.0 + 0.055 * normal.x - 0.035 * normal.z;

    var color = albedo * shading * form;

    // Restrained specular. Water carries it; rock has a trace; everything else
    // has none, because the vertex attribute is zero there.
    let half_vector = normalize(sun_direction + view_direction);
    let specular_strength = specular * scene.ground_bounce.a;
    if specular_strength > 0.0 {
        let lobe = pow(max(dot(normal, half_vector), 0.0), SPECULAR_EXPONENT);
        color = color + scene.sun_color.rgb * (lobe * specular_strength * visibility * scene.sun.w);
    }

    // Exponential distance fog with a height component: valley air reads
    // thicker than ridge air. The target is the sky along this view ray, so a
    // distant silhouette dissolves into exactly what is drawn behind it.
    let height_factor = exp(-scene.params.x * (world_position.y - scene.params.y));
    let optical_depth = scene.fog.a * distance_to_camera * clamp(height_factor, 0.25, 2.5);
    let fog_amount = 1.0 - exp(-optical_depth);
    // Along the horizon the target is the style bible's fog colour; as the ray
    // tilts up toward open sky it becomes the sky itself, so a ridge seen from
    // the valley floor dissolves into what is drawn behind it.
    let ray = -view_direction;
    let haze = mix(scene.fog.rgb, sky_color(ray), smoothstep(0.0, 0.25, ray.y));
    return mix(color, haze, clamp(fog_amount, 0.0, 1.0));
}
