// Wireframe debug primitives: one uniform per box or face outline.
// Shares the camera bind group with the voxel pipeline; `debug.x` is the
// `Lod1` tint strength and is unused here.
struct CameraUniform {
    view_projection: mat4x4<f32>,
    debug: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

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
    output.clip_position = camera.view_projection * vec4<f32>(world_position, 1.0);
    return output;
}

@fragment
fn fs_main(_input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(primitive.color.rgb, 1.0);
}
