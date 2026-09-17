// The procedural sky: a three-stop vertical gradient with a soft sun disc and
// a warm halo. No textures, no clouds, no scattering model.
//
// Drawn first, as one full-screen triangle that writes no depth, so terrain
// simply draws over it. The gradient function is identical to the one the world
// shader fogs toward, which is what makes the horizon seamless by construction
// rather than by tuning.

@group(0) @binding(0)
var<uniform> scene: SceneUniform;

// Angular size of the disc, as a cosine threshold. Soft-edged, not a hard dot.
const SUN_DISC_INNER = 0.9992;
const SUN_DISC_OUTER = 0.9997;
// Two halo lobes: a tight bright one and a wide faint one.
const SUN_HALO_TIGHT_EXPONENT = 220.0;
const SUN_HALO_WIDE_EXPONENT = 12.0;
const SUN_HALO_TIGHT_STRENGTH = 0.55;
const SUN_HALO_WIDE_STRENGTH = 0.10;
const SUN_DISC_STRENGTH = 2.2;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ray: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    // One oversized triangle covering the viewport: cheaper than a quad and
    // free of the seam a two-triangle quad leaves down the diagonal.
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let corner = corners[index];

    var output: VertexOutput;
    output.clip_position = vec4<f32>(corner, 1.0, 1.0);
    // Unproject the far plane to get a world-space direction for this pixel.
    let far_point = scene.inverse_view_projection * vec4<f32>(corner, 1.0, 1.0);
    output.ray = far_point.xyz / far_point.w - scene.camera_position.xyz;
    return output;
}

fn sky_color(direction: vec3<f32>) -> vec3<f32> {
    let up = clamp(direction.y, 0.0, 1.0);
    if up < SKY_MID_HEIGHT {
        return mix(
            scene.sky_horizon.rgb,
            scene.sky_mid.rgb,
            smoothstep(0.0, SKY_MID_HEIGHT, up),
        );
    }
    return mix(
        scene.sky_mid.rgb,
        scene.sky_zenith.rgb,
        smoothstep(SKY_MID_HEIGHT, 1.0, up),
    );
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let direction = normalize(input.ray);
    var color = sky_color(direction);

    // Below the horizon the gradient holds its horizon colour, which is also
    // what fog resolves to, so ground-level haze and sky agree exactly.
    let alignment = max(dot(direction, scene.sun.xyz), 0.0);
    let disc = smoothstep(SUN_DISC_INNER, SUN_DISC_OUTER, alignment);
    let halo = pow(alignment, SUN_HALO_TIGHT_EXPONENT) * SUN_HALO_TIGHT_STRENGTH
        + pow(alignment, SUN_HALO_WIDE_EXPONENT) * SUN_HALO_WIDE_STRENGTH;
    // The sun fades with its own intensity, so overcast has a bright patch
    // where the sun is rather than a disc burning through the cloud.
    color = color + scene.sun_color.rgb * ((disc * SUN_DISC_STRENGTH + halo) * scene.sun.w);

    return vec4<f32>(color, 1.0);
}
