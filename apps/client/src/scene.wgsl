// Shared scene state. Every shader in the client includes this declaration by
// convention: the buffer layout is defined once in `renderer.rs` and must match
// here exactly. WGSL has no include, so the struct is repeated verbatim in each
// file that needs it; this file is the reference copy and the place to change
// it first.
//
// Packing note: a uniform buffer aligns every member to sixteen bytes, so
// scalars ride in the `w` lane of a vector rather than sitting alone. Each lane
// is named in the comment beside it.

struct SceneUniform {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    light_view_projection: mat4x4<f32>,
    // xyz: camera world position. w: unused.
    camera_position: vec4<f32>,
    // xyz: unit vector toward the sun. w: sun intensity.
    sun: vec4<f32>,
    // rgb: sun colour. a: fraction of the lit value a shadowed surface keeps.
    sun_color: vec4<f32>,
    // rgb: ambient from above. a: ambient intensity.
    sky_ambient: vec4<f32>,
    // rgb: ambient bounced from below. a: how much specular water keeps.
    ground_bounce: vec4<f32>,
    sky_zenith: vec4<f32>,
    sky_mid: vec4<f32>,
    sky_horizon: vec4<f32>,
    // rgb: fog colour at the horizon. a: fog density per voxel.
    fog: vec4<f32>,
    // x: fog height falloff. y: fog reference height. z: shadow map texel size.
    // w: how far a Lod1 mesh blends toward the debug tint.
    params: vec4<f32>,
};

// Shared by the sky pass and the world fog target. WGSL has no include, so
// host pipeline construction prepends this source to both shaders.
const SKY_MID_HEIGHT = 0.35;
