// Chunk geometry, drawn with the shared stylized shading model.
//
// What lives here is the chunk's own contract: a two-`vec4` model uniform that
// places an unrotated chunk and scales its local cell size, and the `Lod1`
// debug tint. The sun, the ambient model, the shadow filter, the specular lobe,
// and the fog all live in `shading.wgsl`, which every lit shader shares, so a
// second content domain cannot drift away from terrain's lighting.

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
// How far along its own normal a terrain surface is pushed before the shadow
// lookup. Under a voxel, so a shadow never detaches from what casts it.
const TERRAIN_SHADOW_NORMAL_OFFSET = 0.35;

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

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = shade_surface(
        input.color,
        input.normal,
        input.world_position,
        input.specular,
        TERRAIN_SHADOW_NORMAL_OFFSET,
    );
    return vec4<f32>(color, 1.0);
}
