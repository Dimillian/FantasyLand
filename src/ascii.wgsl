// One fragment per terminal cell. The presentation pass only reads this encoded
// glyph/color grid, never the original scene when ASCII is selected.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
struct Settings { controls: vec4<f32>, output: vec4<f32> };
@group(0) @binding(2) var<uniform> settings: Settings;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1., -1.), vec2<f32>(3., -1.), vec2<f32>(-1., 3.));
    var o: Out; o.clip = vec4<f32>(p[i], 0., 1.);
    o.uv = vec2<f32>(p[i].x * 0.5 + 0.5, 0.5 - p[i].y * 0.5); return o;
}
fn luma(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)); }
@fragment fn fs_main(o: Out) -> @location(0) vec4<f32> {
    let cell_size = vec2<f32>(6., 9.) * settings.controls.z;
    let uv = (floor(o.clip.xy) + 0.5) * cell_size / settings.output.xy;
    let step = cell_size / settings.output.xy * 0.32;
    var color = vec3<f32>(0.);
    var horizontal = 0.;
    var vertical = 0.;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let c = filmic_display(textureSampleLevel(source, linear_sampler, uv + vec2<f32>(f32(x), f32(y)) * step, 0.).rgb);
            let weight = select(1., 2., x == 0) * select(1., 2., y == 0);
            color += c * weight / 16.;
            horizontal += luma(c) * f32(x) * select(1., 2., y == 0) / 4.;
            vertical += luma(c) * f32(y) * select(1., 2., x == 0) / 4.;
        }
    }
    let brightness = clamp(luma(color), 0., 1.);
    // Printable ASCII only. Density increases through punctuation, letters and
    // symbols. Strong contours select strokes that preserve their direction.
    let ramp = array<u32, 14>(0u, 1u, 2u, 3u, 4u, 5u, 6u, 7u, 8u, 9u, 10u, 12u, 13u, 14u);
    var glyph = ramp[min(u32(pow(brightness, 0.72) * 15.), 13u)];
    if length(vec2<f32>(horizontal, vertical)) > 0.11 {
        if abs(horizontal) > abs(vertical) * 2.0 { glyph = 15u; }
        else if abs(vertical) > abs(horizontal) * 2.0 { glyph = 5u; }
        else if horizontal * vertical > 0. { glyph = 16u; }
        else { glyph = 17u; }
    }
    var foreground = clamp(pow(max(color, vec3<f32>(0.)), vec3<f32>(0.62)) * 1.28, vec3<f32>(0.), vec3<f32>(1.));
    let ink = 0.40 + 0.60 * sqrt(brightness);
    if settings.controls.w > 0.5 && settings.controls.w < 1.5 { foreground = vec3<f32>(1., 0.68, 0.25) * ink; }
    if settings.controls.w > 1.5 { foreground = vec3<f32>(0.35, 1., 0.56) * ink; }
    return vec4<f32>(foreground, f32(glyph) / 255.);
}
