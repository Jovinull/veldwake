// The shadow pass: depth only, from the sun.
//
// One cascade, one pipeline, no fragment shader. The geometry is the same chunk
// meshes the world pass draws, transformed by the light's view-projection
// instead of the camera's, so anything that casts a shadow is something that is
// actually drawn.

@group(0) @binding(0)
var<uniform> scene: SceneUniform;

struct ModelUniform {
    translation: vec4<f32>,
    scale: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> model: ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) specular: f32,
};

@vertex
fn vs_main(input: VertexInput) -> @builtin(position) vec4<f32> {
    let world_position = model.translation.xyz + input.position * model.scale.x;
    return scene.light_view_projection * vec4<f32>(world_position, 1.0);
}
