@group(0) @binding(1) var sun_depth: texture_depth_2d;
@group(0) @binding(2) var sun_comparison: sampler_comparison;
struct SunCascades {
    far_matrix: mat4x4<f32>,
    origin: vec4<f32>,
    params: vec4<f32>, // inverse far size, near blend start/end, enabled
};
@group(0) @binding(12) var<uniform> sun_cascades: SunCascades;
@group(0) @binding(13) var far_sun_depth: texture_depth_2d;

fn cascade_uv(clip: vec4<f32>) -> vec2<f32> {
    return vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
}
fn cascade_valid(clip: vec4<f32>) -> bool {
    let uv = cascade_uv(clip);
    return clip.z > 0.0 && clip.z < 1.0 && all(uv >= vec2<f32>(0.002)) && all(uv <= vec2<f32>(0.998));
}
fn cascade_filter(depth: texture_depth_2d, clip: vec4<f32>, bias: f32, texel: f32) -> f32 {
    let uv = cascade_uv(clip);
    let offset = texel * 0.75;
    let z = clip.z - bias;
    return (textureSampleCompareLevel(depth, sun_comparison, uv + vec2<f32>(-offset,-offset), z)
        + textureSampleCompareLevel(depth, sun_comparison, uv + vec2<f32>(offset,-offset), z)
        + textureSampleCompareLevel(depth, sun_comparison, uv + vec2<f32>(-offset,offset), z)
        + textureSampleCompareLevel(depth, sun_comparison, uv + vec2<f32>(offset,offset), z)) * 0.25;
}
fn far_cascade_visibility(world: vec3<f32>, bias: f32) -> f32 {
    let far_clip = sun_cascades.far_matrix * vec4<f32>(world - sun_cascades.origin.xyz, 1.0);
    if !cascade_valid(far_clip) { return 1.0; }
    return cascade_filter(far_sun_depth, far_clip, bias, sun_cascades.params.x);
}
fn cascaded_sun_visibility(world: vec3<f32>, normal: vec3<f32>, distance: f32) -> f32 {
    let bias = 0.000035 + (1.0 - max(dot(normal, normalize(u.light.xyz)), 0.0)) * 0.000075;
    // Most pixels need one transform and one map. Avoid projecting into both
    // light frusta outside the narrow transition, without changing PCF quality.
    if distance >= sun_cascades.params.z { return far_cascade_visibility(world, bias); }
    let near_clip = u.shadow_matrix * vec4<f32>(world - u.shadow_origin.xyz, 1.0);
    let near_valid = cascade_valid(near_clip);
    // Only the overlap evaluates both maps. Tall geometry outside the near
    // light frustum falls back to the far map instead of acquiring a seam.
    if near_valid && distance <= sun_cascades.params.y {
        return cascade_filter(sun_depth, near_clip, bias, u.shadow_params.x);
    }
    let far_visibility = far_cascade_visibility(world, bias);
    if !near_valid { return far_visibility; }
    let near_visibility = cascade_filter(sun_depth, near_clip, bias, u.shadow_params.x);
    return mix(near_visibility, far_visibility, smoothstep(sun_cascades.params.y, sun_cascades.params.z, distance));
}

fn sun_visibility(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    if u.shadow_params.z < 0.5 { return 1.0; }
    let distance = length(world.xz - u.camera.xz);
    let influence = 1.0 - smoothstep(u.shadow_params.y * 0.65, u.shadow_params.y * 0.90, distance);
    if influence <= 0.0 { return 1.0; }
    if sun_cascades.params.w > 0.5 {
        return mix(1.0, cascaded_sun_visibility(world, normal, distance), influence * clamp(u.shadow_params.w, 0.0, 1.0));
    }
    let clip = u.shadow_matrix * vec4<f32>(world - u.shadow_origin.xyz, 1.0);
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if clip.z <= 0.0 || clip.z >= 1.0 || any(uv < vec2<f32>(0.002)) || any(uv > vec2<f32>(0.998)) { return 1.0; }
    let bias = 0.000035 + (1.0 - max(dot(normal, normalize(u.light.xyz)), 0.0)) * 0.000075;
    let z = clip.z - bias;
    let offset = u.shadow_params.x * 0.75;
    let shade = (textureSampleCompareLevel(sun_depth, sun_comparison, uv + vec2<f32>(-offset,-offset), z)
        + textureSampleCompareLevel(sun_depth, sun_comparison, uv + vec2<f32>(offset,-offset), z)
        + textureSampleCompareLevel(sun_depth, sun_comparison, uv + vec2<f32>(-offset,offset), z)
        + textureSampleCompareLevel(sun_depth, sun_comparison, uv + vec2<f32>(offset,offset), z)) * 0.25;
    return mix(1.0, shade, influence * clamp(u.shadow_params.w, 0.0, 1.0));
}
