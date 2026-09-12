// Shared HDR presentation. Palette reduction happens after light and Bloom.
fn filmic_display(radiance: vec3<f32>) -> vec3<f32> {
    let x = max(radiance, vec3<f32>(0.0)) * 0.80;
    let mapped = clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
    return pow(mapped, vec3<f32>(1.0 / 2.2));
}
fn retro_display(radiance: vec3<f32>, pixel: vec2<f32>) -> vec3<f32> {
    let color = filmic_display(radiance);
    let b = array<f32,16>(0.,8.,2.,10.,12.,4.,14.,6.,3.,11.,1.,9.,15.,7.,13.,5.);
    let p = vec2<u32>(pixel) % vec2<u32>(4u);
    let dither = (b[p.y * 4u + p.x] / 16.0 - 0.5) * 0.38;
    return max(floor(color * 128.0 + dither) / 128.0, vec3<f32>(0.0));
}
