struct Atmosphere {
    inverse_view_projection: mat4x4<f32>,
    shadow_matrix: mat4x4<f32>,
    camera: vec4<f32>,
    shadow_origin: vec4<f32>,
    light_direction: vec4<f32>,
    light_color: vec4<f32>,
    atmosphere: vec4<f32>,
    shadow: vec4<f32>,
    room: array<vec4<f32>,3>,
    resolution: vec4<f32>,
};
@group(0) @binding(0) var<uniform> fog: Atmosphere;
@group(0) @binding(1) var scene_depth: texture_depth_2d;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: Out;
    o.clip = vec4<f32>(p[i], 0.0, 1.0);
    o.uv = vec2<f32>(p[i].x * 0.5 + 0.5, 0.5 - p[i].y * 0.5);
    return o;
}
fn relative_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let p = fog.inverse_view_projection * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return p.xyz / max(p.w, 0.0000001);
}
fn integration_distance(uv: vec2<f32>, depth: f32) -> f32 {
    if depth >= 0.999999 { return fog.atmosphere.y; }
    return min(length(relative_position(uv, depth)), fog.atmosphere.y);
}
