fn volume_visibility(clip: vec4<f32>) -> f32 {
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if clip.z <= 0.0 || clip.z >= 1.0 || any(uv < vec2<f32>(0.002)) || any(uv > vec2<f32>(0.998)) { return 1.0; }
    // One bilinear PCF lookup per sample. The shadow pass uses exactly the same
    // foliage alpha test as the visible leaves, so holes produce actual shafts.
    let visibility = textureSampleCompareLevel(light_depth, light_comparison, uv, clip.z - 0.00004);
    return mix(1.0, visibility, fog.shadow.z);
}

// Both resolutions use the same integrator, sample count and stable dither.
// A rejected reconstruction is therefore actual occluded participating media,
// rather than an identity pixel or extrapolated background illumination.
fn volume_integral(ray: vec3<f32>, distance: f32, low_pixel: vec2<i32>) -> vec4<f32> {
    let jitter = fract(52.9829189 * fract(dot(vec2<f32>(low_pixel), vec2<f32>(0.06711056, 0.00583715))));
    let steps = fog.shadow.w;
    let mu = dot(ray, fog.light_direction.xyz);
    let g = fog.atmosphere.w;
    // Henyey-Greenstein forward scatter. A restrained cap limits the solar halo
    // while retaining beams seen at an angle through leaves and across water.
    let phase = min((1.0 - g*g) / pow(max(1.0 + g*g - 2.0*g*mu, 0.03), 1.5), 5.0);
    let radiance = fog.light_color.rgb * fog.light_direction.w * fog.light_color.w * phase;
    var scattering = vec3<f32>(0.0);
    var transmittance = 1.0;
    // The directional shadow transform is affine. Transform the ray once,
    // including in the full-resolution leaf-edge fallback, not at every step.
    let shadow_start = fog.shadow_matrix * vec4<f32>(fog.camera.xyz - fog.shadow_origin.xyz, 1.0);
    let shadow_ray = fog.shadow_matrix * vec4<f32>(ray, 0.0);
    for (var i = 0u; i < 20u; i += 1u) {
        if f32(i) >= steps { break; }
        // Quadratic spacing allocates most samples to nearby canopy gaps. Each
        // interval still integrates its exact length, avoiding brightness bias.
        let t0 = f32(i) / steps;
        let t1 = (f32(i) + 1.0) / steps;
        let start = t0 * t0 * distance;
        let end = t1 * t1 * distance;
        let step_length = end - start;
        let t = mix(start, end, 0.12 + 0.76 * fract(jitter + f32(i) * 0.61803399));
        let sample = ray * t;
        let altitude = max((fog.camera.y - fog.camera.w) + sample.y, 0.0);
        let density = fog.atmosphere.x * (0.28 + 0.72 * exp(-altitude * fog.atmosphere.z));
        let opacity = 1.0 - exp(-density * step_length);
        scattering += radiance * volume_visibility(shadow_start + shadow_ray * t) * (transmittance * opacity);
        transmittance *= 1.0 - opacity;
    }
    return vec4<f32>(scattering, transmittance);
}
