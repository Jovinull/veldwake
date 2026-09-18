// Voxel chips, drawn instanced: one unit cube, one draw call, one instance per
// live particle. Shares the scene uniform with every other pipeline and reads
// only the view-projection — a chip is emissive and takes no light, because a
// spark that is shadowed is not a spark.

@group(0) @binding(0)
var<uniform> scene: SceneUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    // How lit the cube face is, from its own normal alone: enough to keep a
    // chip from reading as a flat silhouette without making it take shadows.
    @location(1) facing: f32,
};

@vertex
fn vs_main(
    @location(0) unit: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // xyz world centre, w edge length.
    @location(2) placement: vec4<f32>,
    @location(3) color: vec4<f32>,
) -> VertexOutput {
    var output: VertexOutput;
    let world_position = placement.xyz + unit * placement.w;
    output.clip_position = scene.view_projection * vec4<f32>(world_position, 1.0);
    output.color = color.rgb;
    output.facing = clamp(dot(normal, normalize(scene.sun.xyz)), 0.0, 1.0);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Mostly its own colour, with a little face shading so the cube has edges.
    let shaded = input.color * (0.78 + 0.22 * input.facing);
    return vec4<f32>(shaded, 1.0);
}
