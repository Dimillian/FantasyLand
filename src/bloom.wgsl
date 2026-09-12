@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1., -1.), vec2<f32>(3., -1.), vec2<f32>(-1., 3.));
    var o: Out; o.clip = vec4<f32>(positions[i], 0., 1.);
    o.uv = vec2<f32>(positions[i].x * 0.5 + 0.5, 0.5 - positions[i].y * 0.5); return o;
}
fn sample_at(uv: vec2<f32>) -> vec3<f32> { return textureSampleLevel(source, linear_sampler, uv, 0.).rgb; }
fn highlight(uv: vec2<f32>) -> vec3<f32> {
    // The scene is already linear HDR: retain radiance above display white.
    let color = max(sample_at(uv), vec3<f32>(0.));
    let brightness = max(color.r, max(color.g, color.b));
    let threshold = 0.42;
    let knee = 0.18;
    let soft = clamp(brightness - threshold + knee, 0., knee * 2.);
    let contribution = max(brightness - threshold, soft * soft / (4. * knee));
    return color * contribution / max(brightness, 0.0001);
}
@fragment fn fs_extract(o: Out) -> @location(0) vec4<f32> {
    let d = 0.5 / vec2<f32>(textureDimensions(source));
    let color = highlight(o.uv + d) + highlight(o.uv - d)
        + highlight(o.uv + vec2<f32>(d.x, -d.y)) + highlight(o.uv + vec2<f32>(-d.x, d.y));
    return vec4<f32>(color * 0.25, 1.);
}
@fragment fn fs_downsample(o: Out) -> @location(0) vec4<f32> {
    let d = 0.5 / vec2<f32>(textureDimensions(source));
    return vec4<f32>((sample_at(o.uv + d) + sample_at(o.uv - d)
        + sample_at(o.uv + vec2<f32>(d.x, -d.y)) + sample_at(o.uv + vec2<f32>(-d.x, d.y))) * 0.25, 1.);
}
fn blur(uv: vec2<f32>, axis: vec2<f32>) -> vec4<f32> {
    // Nine-tap Gaussian collapsed to five bilinear samples.
    let d = axis / vec2<f32>(textureDimensions(source));
    var color = sample_at(uv) * 0.227027027;
    color += (sample_at(uv + d * 1.3846153846) + sample_at(uv - d * 1.3846153846)) * 0.3162162162;
    color += (sample_at(uv + d * 3.2307692308) + sample_at(uv - d * 3.2307692308)) * 0.0702702703;
    return vec4<f32>(color, 1.);
}
@fragment fn fs_horizontal(o: Out) -> @location(0) vec4<f32> { return blur(o.uv, vec2<f32>(1., 0.)); }
@fragment fn fs_vertical(o: Out) -> @location(0) vec4<f32> { return blur(o.uv, vec2<f32>(0., 1.)); }
