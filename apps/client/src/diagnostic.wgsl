struct CameraUniform {
    view_projection: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct ModelUniform {
    // xyz: chunk origin in world units (ChunkCoord * 32), never scaled.
    translation: vec4<f32>,
    // x: local cell size (1 for Lod0, 2 for Lod1); yzw padding.
    scale: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> model: ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let world_position = model.translation.xyz + input.position * model.scale.x;
    output.clip_position = camera.view_projection * vec4<f32>(world_position, 1.0);
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
