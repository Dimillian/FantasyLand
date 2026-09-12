@group(0) @binding(1) var sun_depth: texture_depth_2d;
@group(0) @binding(2) var sun_comparison: sampler_comparison;

fn sun_visibility(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    if u.shadow_params.z < 0.5 { return 1.0; }
    let distance = length(world.xz - u.camera.xz);
    let influence = 1.0 - smoothstep(u.shadow_params.y * 0.65, u.shadow_params.y * 0.90, distance);
    if influence <= 0.0 { return 1.0; }
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
