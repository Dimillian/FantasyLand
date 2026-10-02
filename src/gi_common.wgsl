// Shared std140/std430 layouts; mirrored by gi.rs. RGB coefficients represent
// diffuse irradiance/pi, so the surface shader multiplies by albedo exactly once.
struct GiUniform {
    origin_cell: vec4<f32>,
    voxel_dims: vec4<u32>,
    probe_dims: vec4<u32>,
    parameters: vec4<f32>,
    sun: vec4<f32>,
    direct: vec4<f32>,
    ambient: vec4<f32>,
    update: vec4<u32>,
    scroll: vec4<i32>,
    hearths: array<vec4<f32>, 8>,
}
struct GiProbe {
    position: vec4<f32>,
    sh0: vec4<f32>,
    shx: vec4<f32>,
    shy: vec4<f32>,
    shz: vec4<f32>,
    distance0: vec4<f32>,
    distance1: vec4<f32>,
    moment0: vec4<f32>,
    moment1: vec4<f32>,
}
