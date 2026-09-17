// Stylized world shading: one directional sun, hemispheric ambient, a single
// filtered shadow map, a restrained specular, and distance fog that resolves
// into the same sky the sky pass draws.
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

struct ModelUniform {
    // xyz: chunk origin in world units (ChunkCoord * 32), never scaled.
    translation: vec4<f32>,
    // x: local cell size (1 for Lod0, 2 for Lod1); yzw padding.
    scale: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> model: ModelUniform;

// Cool blue for the coarse level in the Lod debug view.
const LOD1_DEBUG_TINT = vec3<f32>(0.35, 0.60, 1.0);
// Tightness of the specular lobe. High enough that only water and wet rock
// catch the sun, which is the cue the style bible asks water to carry.
const SPECULAR_EXPONENT = 64.0;
// How far along its own normal a surface is pushed before the shadow lookup.
// Under a voxel, so a shadow never detaches from what casts it.
const SHADOW_NORMAL_OFFSET = 0.35;
// Depth bias floor and slope scale, in light-space depth units.
const SHADOW_MIN_BIAS = 0.00035;
const SHADOW_SLOPE_BIAS = 0.0022;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) specular: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world_position: vec3<f32>,
    @location(3) specular: f32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world_position = model.translation.xyz + input.position * model.scale.x;
    output.clip_position = scene.view_projection * vec4<f32>(world_position, 1.0);
    output.world_position = world_position;
    output.normal = input.normal;
    output.specular = input.specular;
    // `scale.x` is the level: 1 for Lod0, 2 for Lod1. Only the coarse level is
    // tinted, and only while the Lod debug view is active.
    let coarse = step(1.5, model.scale.x);
    output.color = mix(input.color, LOD1_DEBUG_TINT, scene.params.w * coarse);
    return output;
}

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

/// How much sun reaches a point: one where fully lit, zero where fully shadowed.
fn sun_visibility(world_position: vec3<f32>, normal: vec3<f32>, ndl: f32) -> f32 {
    // A back-facing surface needs no map lookup; it is in its own shadow.
    if ndl <= 0.0 {
        return 0.0;
    }
    let offset = world_position + normal * SHADOW_NORMAL_OFFSET;
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

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal);
    let to_camera_vector = scene.camera_position.xyz - input.world_position;
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
    let visibility = sun_visibility(input.world_position, normal, ndl);
    let lit = sun + ambient;
    let shadowed = max(ambient, lit * scene.sun_color.a);
    let shading = mix(shadowed, lit, visibility);

    // Normal-based value separation: a small, fixed offset per horizontal axis
    // so a voxel form still reads when the sun is behind it. Kept well inside
    // the style bible's within-band contrast limit.
    let form = 1.0 + 0.055 * normal.x - 0.035 * normal.z;

    var color = input.color * shading * form;

    // Restrained specular. Water carries it; rock has a trace; everything else
    // has none, because the vertex attribute is zero there.
    let half_vector = normalize(sun_direction + view_direction);
    let specular_strength = input.specular * scene.ground_bounce.a;
    if specular_strength > 0.0 {
        let lobe = pow(max(dot(normal, half_vector), 0.0), SPECULAR_EXPONENT);
        color = color + scene.sun_color.rgb * (lobe * specular_strength * visibility * scene.sun.w);
    }

    // Exponential distance fog with a height component: valley air reads
    // thicker than ridge air. The target is the sky along this view ray, so a
    // distant silhouette dissolves into exactly what is drawn behind it.
    let height_factor = exp(-scene.params.x * (input.world_position.y - scene.params.y));
    let optical_depth = scene.fog.a * distance_to_camera * clamp(height_factor, 0.25, 2.5);
    let fog_amount = 1.0 - exp(-optical_depth);
    // Along the horizon the target is the style bible's fog colour; as the ray
    // tilts up toward open sky it becomes the sky itself, so a ridge seen from
    // the valley floor dissolves into what is drawn behind it.
    let ray = -view_direction;
    let haze = mix(scene.fog.rgb, sky_color(ray), smoothstep(0.0, 0.25, ray.y));
    color = mix(color, haze, clamp(fog_amount, 0.0, 1.0));

    return vec4<f32>(color, 1.0);
}
