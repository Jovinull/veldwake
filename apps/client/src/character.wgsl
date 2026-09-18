// Character body parts, drawn with the same stylized shading model as terrain.
//
// The only thing a character needs that a chunk does not is a full transform:
// a body part rotates about its joint, and the chunk model uniform carries a
// translation and a uniform scale with no rotation at all. Rather than widen
// that uniform — and with it every recorded chunk byte measurement — characters
// carry their own eighty-byte part uniform and their own pipeline. The vertex
// layout, the scene bind group, and every lighting constant are shared.

struct PartUniform {
    // World matrix of one body part: character placement, the character's yaw,
    // the voxel scale, the bone's world transform, and the part's grid origin,
    // all composed on the CPU. The renderer composes nothing.
    model: mat4x4<f32>,
    // x: how far along its own normal this surface is pushed before the shadow
    // lookup, in world units. yzw: padding.
    params: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> part: PartUniform;

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
    let world_position = (part.model * vec4<f32>(input.position, 1.0)).xyz;
    output.clip_position = scene.view_projection * vec4<f32>(world_position, 1.0);
    output.world_position = world_position;
    // The part matrix carries a uniform scale, so rotating the normal by the
    // upper three-by-three and renormalizing is exact; no inverse transpose is
    // needed and none is uploaded.
    output.normal = normalize((part.model * vec4<f32>(input.normal, 0.0)).xyz);
    output.color = input.color;
    output.specular = input.specular;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = shade_surface(
        input.color,
        input.normal,
        input.world_position,
        input.specular,
        part.params.x,
    );
    return vec4<f32>(color, 1.0);
}
