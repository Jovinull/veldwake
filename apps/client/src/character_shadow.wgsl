// The shadow pass for character body parts: depth only, from the sun.
//
// The same geometry the world pass draws, transformed by the light's
// view-projection instead of the camera's, so anything that casts a shadow is
// something that is actually drawn. It is a separate shader from the chunk
// shadow pass only because a body part carries a full transform where a chunk
// carries a translation and a scale.

@group(0) @binding(0)
var<uniform> scene: SceneUniform;

struct PartUniform {
    model: mat4x4<f32>,
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

@vertex
fn vs_main(input: VertexInput) -> @builtin(position) vec4<f32> {
    let world_position = (part.model * vec4<f32>(input.position, 1.0)).xyz;
    return scene.light_view_projection * vec4<f32>(world_position, 1.0);
}
