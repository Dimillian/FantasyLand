@group(0) @binding(2) var scattered_light: texture_2d<f32>;
@group(0) @binding(3) var integrated_distance: texture_2d<f32>;
@group(0) @binding(4) var light_depth: texture_depth_2d;
@group(0) @binding(5) var light_comparison: sampler_comparison;

@fragment fn fs_composite(o: Out) -> @location(0) vec4<f32> {
    let full_pixel = clamp(vec2<i32>(o.clip.xy), vec2<i32>(0), vec2<i32>(fog.resolution.xy) - vec2<i32>(1));
    let depth = textureLoad(scene_depth, full_pixel, 0);
    let distance = integration_distance(o.uv, depth);
    let coordinate = o.uv * fog.resolution.zw - vec2<f32>(0.5);
    let base = vec2<i32>(floor(coordinate));
    let fraction = fract(coordinate);
    var result = vec4<f32>(0.0);
    var weights: array<f32, 4>;
    var pixels: array<vec2<i32>, 4>;
    var total_weight = 0.0;
    for (var y = 0; y < 2; y += 1) {
        for (var x = 0; x < 2; x += 1) {
            let pixel = clamp(base + vec2<i32>(x, y), vec2<i32>(0), vec2<i32>(fog.resolution.zw) - vec2<i32>(1));
            let sample_distance = textureLoad(integrated_distance, pixel, 0).r;

            let error = abs(distance - sample_distance);
            let xy_weight = select(vec2<f32>(1.0) - fraction, fraction, vec2<bool>(x == 1, y == 1));
            let bilateral = exp(-error / (0.50 + min(distance, sample_distance) * 0.018));
            let weight = xy_weight.x * xy_weight.y * bilateral;
            weights[y*2+x] = weight;
            pixels[y*2+x] = pixel;
            total_weight += weight;
        }
    }
    // Unmatched leaf pixels use their exact-depth integral. Do not fetch four
    // low-resolution scattering texels that will be discarded in that case.
    if total_weight > 0.000001 {
        for (var i=0; i<4; i+=1) {result += textureLoad(scattered_light,pixels[i],0)*weights[i];}
    }
    // Well-supported smooth surfaces keep the inexpensive quarter-area result.
    // Thin leaves/stems can have no matching nearest-depth representative. The
    // old identity fallback omitted ALL fog there, producing black floating
    // dashes and hard on/off flicker as wind changed the winning depth sample.
    if total_weight >= 0.12 { return result / total_weight; }
    let ray = normalize(relative_position(o.uv, 0.5));
    let fine = volume_integral(ray, distance, full_pixel / vec2<i32>(2));
    if total_weight <= 0.000001 { return fine; }
    // Blend the transition continuously. Both estimates end at foreground
    // depth; the fallback never copies distant light across an occluding leaf.
    let confidence = smoothstep(0.015, 0.12, total_weight);
    return mix(fine, result / total_weight, confidence);
}
