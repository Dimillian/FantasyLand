@group(0) @binding(2) var light_depth: texture_depth_2d;
@group(0) @binding(3) var light_comparison: sampler_comparison;
struct VolumeOut {
    @location(0) scattering: vec4<f32>,
    @location(1) distance: f32,
};

@fragment fn fs_rays(o: Out) -> VolumeOut {
    let full_size = vec2<i32>(fog.resolution.xy);
    let low_pixel = vec2<i32>(o.clip.xy);
    let base = low_pixel * 2;
    // Conservative 2x2 depth downsampling keeps shafts behind foreground
    // silhouettes. Retain the selected pixel's ray, including on odd resizes.
    var nearest_depth = 1.0;
    var nearest_pixel = clamp(base, vec2<i32>(0), full_size - vec2<i32>(1));
    for (var y = 0; y < 2; y += 1) {
        for (var x = 0; x < 2; x += 1) {
            let pixel = clamp(base + vec2<i32>(x, y), vec2<i32>(0), full_size - vec2<i32>(1));
            let depth = textureLoad(scene_depth, pixel, 0);
            if depth < nearest_depth {
                nearest_depth = depth;
                nearest_pixel = pixel;
            }
        }
    }
    let uv = (vec2<f32>(nearest_pixel) + vec2<f32>(0.5)) / fog.resolution.xy;
    let ray = normalize(relative_position(uv, 0.5));
    let distance = integration_distance(uv, nearest_depth);
    var result: VolumeOut;
    result.scattering = volume_integral(ray, distance, low_pixel);
    result.distance = distance;
    return result;
}
