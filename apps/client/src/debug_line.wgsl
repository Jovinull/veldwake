// Wireframe debug primitives: one uniform per box or face outline.
// Shares the scene uniform with the world pipeline and reads only its
// view-projection; the shading state is irrelevant to a line.

@group(0) @binding(0)
var<uniform> scene: SceneUniform;

struct PrimitiveUniform {
    // xyz: world origin of the volume; w: its edge length in world units.
    placement: vec4<f32>,
    // rgb: line color; a unused.
    color: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> primitive: PrimitiveUniform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_main(@location(0) unit: vec3<f32>) -> VertexOutput {
    var output: VertexOutput;
    let world_position = primitive.placement.xyz + unit * primitive.placement.w;
    output.clip_position = scene.view_projection * vec4<f32>(world_position, 1.0);
    return output;
}

@fragment
fn fs_main(_input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(primitive.color.rgb, 1.0);
}
