@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var glow_near: texture_2d<f32>;
@group(0) @binding(2) var glow_mid: texture_2d<f32>;
@group(0) @binding(3) var glow_far: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;
struct Settings { controls: vec2<f32>, output: vec2<f32>, weather: vec4<f32>, atmosphere: vec4<f32> };
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
fn weather_grade(hdr: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    let w = settings.weather;
    let exposure = settings.atmosphere.y;
    let overcast = smoothstep(0.55, 1.0, w.x);
    let squall = smoothstep(10.0, 30.0, settings.atmosphere.x) * w.y;
    let winter = w.z;
    let luma = dot(hdr, vec3<f32>(0.2126,0.7152,0.0722));
    let saturation = 1.03 - overcast*0.10 - squall*0.19 - winter*0.12;
    var graded = mix(vec3<f32>(luma), hdr, saturation);
    // Steel-blue rain, pearl snow, warm fair-weather highlights. Preserve dark
    // detail: this is a color transform, not an opaque grey/dark overlay.
    let cool = vec3<f32>(0.89,0.98,1.08);
    let pearl = vec3<f32>(0.97,1.03,1.10);
    let tint = mix(mix(vec3<f32>(1.035,1.01,0.975),cool,overcast),pearl,winter);
    graded *= tint * (1.0 - squall*0.06 + winter*0.06);
    let edge = smoothstep(0.18,1.45,dot(uv*2.0-1.0,uv*2.0-1.0));
    graded *= 1.0 - edge * (w.y*0.045+squall*0.09);
    return max(mix(hdr,graded,exposure),vec3<f32>(0.0));
}
// AA consumes display-encoded color. Quantization and CRT optics stay later.
@fragment fn fs_tonemap(o: Out) -> @location(0) vec4<f32> {
    var radiance = nearest(o.uv);
    if settings.controls.x > 0.5 && settings.controls.y > 0.0 {
        let amount = select(0.55, 0.22, settings.controls.x > 1.5);
        radiance += glow(o.uv) * settings.controls.y * amount;
    }
    return vec4<f32>(filmic_display(weather_grade(radiance,o.uv)), 1.0);
}
@fragment fn fs_main(o: Out) -> @location(0) vec4<f32> {
    let strength = settings.controls.y;
    if settings.controls.x < 0.5 || strength <= 0. {
        return vec4<f32>(retro_display(weather_grade(nearest(o.uv),o.uv), o.clip.xy), 1.);
    }
    if settings.controls.x < 1.5 {
        // Keep the sharp pixel source. Only the extracted light is softened.
        return vec4<f32>(retro_display(weather_grade(nearest(o.uv) + glow(o.uv) * strength * 0.55,o.uv), o.clip.xy), 1.);
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
    var color = retro_display(weather_grade(mix(nearest(uv), beam_color, min(strength * 0.72, 1.)) + glow(uv) * strength * 0.22,uv), o.clip.xy);
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
