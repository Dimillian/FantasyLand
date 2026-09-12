@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var glow_near: texture_2d<f32>;
@group(0) @binding(2) var glow_mid: texture_2d<f32>;
@group(0) @binding(3) var glow_far: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;
struct Settings { controls: vec2<f32>, output: vec2<f32> };
@group(0) @binding(5) var<uniform> settings: Settings;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1., -1.), vec2<f32>(3., -1.), vec2<f32>(-1., 3.));
    var o: Out; o.clip = vec4<f32>(p[i], 0., 1.);
    o.uv = vec2<f32>(p[i].x * 0.5 + 0.5, 0.5 - p[i].y * 0.5); return o;
}
fn nearest(uv: vec2<f32>) -> vec3<f32> {
    let size = textureDimensions(scene);
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
    return textureLoad(scene, pixel, 0).rgb;
}
fn glow(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(glow_near, linear_sampler, uv, 0.).rgb * 0.55
        + textureSampleLevel(glow_mid, linear_sampler, uv, 0.).rgb * 0.30
        + textureSampleLevel(glow_far, linear_sampler, uv, 0.).rgb * 0.28;
}
@fragment fn fs_main(o: Out) -> @location(0) vec4<f32> {
    let strength = settings.controls.y;
    if settings.controls.x < 0.5 || strength <= 0. {
        return vec4<f32>(retro_display(nearest(o.uv), o.clip.xy), 1.);
    }
    if settings.controls.x < 1.5 {
        // Keep the sharp pixel source. Only the extracted light is softened.
        return vec4<f32>(retro_display(nearest(o.uv) + glow(o.uv) * strength * 0.55, o.clip.xy), 1.);
    }
    // Stationary CRT optics: no random jitter or temporal flicker.
    let q = o.uv * 2. - 1.;
    let bowed = q * (1. + dot(q, q) * 0.035 * strength);
    let uv = bowed * 0.5 + 0.5;
    let edge_distance = min(min(uv.x, uv.y), min(1. - uv.x, 1. - uv.y));
    let edge = smoothstep(0., 1.5 / max(settings.output.x, settings.output.y), edge_distance);
    let scene_size = vec2<f32>(textureDimensions(scene));
    let fringe = vec2<f32>(0.48 * strength * (0.3 + dot(q, q)) / scene_size.x, 0.);
    let red = textureSampleLevel(scene, linear_sampler, uv + fringe, 0.).r;
    let green = textureSampleLevel(scene, linear_sampler, uv, 0.).g;
    let blue = textureSampleLevel(scene, linear_sampler, uv - fringe, 0.).b;
    let beam_color = vec3<f32>(red, green, blue);
    var color = retro_display(mix(nearest(uv), beam_color, min(strength * 0.72, 1.)) + glow(uv) * strength * 0.22, o.clip.xy);
    // Limit line density at small viewports to keep beams above the pixel grid.
    let lines = min(scene_size.y, settings.output.y / 3.);
    let beam = pow(max(sin(fract(uv.y * lines) * 3.14159265), 0.), 0.65);
    color *= 1. - min(strength * 0.30, 0.45) * (1. - beam);
    // RGB phosphor triads follow physical output pixels, not moving geometry.
    let phosphor = u32(o.clip.x) % 3u;
    var mask = vec3<f32>(0.86);
    if phosphor == 0u { mask.r = 1.12; }
    if phosphor == 1u { mask.g = 1.12; }
    if phosphor == 2u { mask.b = 1.12; }
    color *= mix(vec3<f32>(1.), mask, min(strength * 0.68, 1.));
    let vignette = 1. - smoothstep(0.3, 1.8, dot(q, q)) * 0.16 * strength;
    color *= vignette * (1. + strength * 0.065);
    return vec4<f32>(mix(vec3<f32>(0.004, 0.006, 0.008), color, edge), 1.);
}
