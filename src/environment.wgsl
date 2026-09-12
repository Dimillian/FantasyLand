// One overhead depth map is shared by ambient enclosure, snow deposition and
// precipitation shelter. It renders actual nearby trees, terrain and rocks.
@group(0) @binding(3) var shelter_depth: texture_depth_2d;
@group(0) @binding(4) var reflected_scene: texture_2d<f32>;
@group(0) @binding(5) var environment_sampler: sampler;

fn open_sky(world: vec3<f32>) -> f32 {
    if u.shelter_params.w < 0.5 { return 1.0; }
    let clip = u.shelter_matrix * vec4<f32>(world - u.shelter_origin.xyz, 1.0);
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if clip.z <= 0.0 || clip.z >= 1.0 || any(uv < vec2<f32>(0.02)) || any(uv > vec2<f32>(0.98)) { return 1.0; }
    let dz = clip.z - 0.00045;
    let d = u.shelter_params.x * 1.4;
    let center = textureSampleCompareLevel(shelter_depth, sun_comparison, uv, dz);
    let sides = textureSampleCompareLevel(shelter_depth, sun_comparison, uv + vec2<f32>(d, 0.0), dz)
        + textureSampleCompareLevel(shelter_depth, sun_comparison, uv - vec2<f32>(d, 0.0), dz)
        + textureSampleCompareLevel(shelter_depth, sun_comparison, uv + vec2<f32>(0.0, d), dz)
        + textureSampleCompareLevel(shelter_depth, sun_comparison, uv - vec2<f32>(0.0, d), dz);
    let fade = 1.0 - smoothstep(u.shelter_params.y * 0.72, u.shelter_params.y * 0.95, length(world.xz - u.shelter_origin.xz));
    return mix(1.0, center * 0.44 + sides * 0.14, fade);
}

fn weather_light_visibility(world: vec3<f32>) -> f32 {
    let light = normalize(u.light.xyz);
    if light.y < 0.035 { return 1.0 - u.weather.x * 0.50; }
    let at_cloud = world.xz + light.xz * max(weather_cloud_base(false) - world.y, 0.0) / light.y;
    let cloud = weather_cloud_field(at_cloud, false);
    return mix(1.0, 0.26, cloud.x) * (1.0 - u.weather.x * 0.12);
}

fn reflected_environment(world: vec3<f32>, normal: vec3<f32>, view: vec3<f32>, roughness: f32) -> vec3<f32> {
    let ray = reflect(-view, normal);
    if u.reflection_params.y > 0.5 && u.reflection_params.z < 0.5 && abs(world.y - u.reflection_params.x) < 0.65 {
        let clip = u.reflection_matrix * vec4<f32>(world - u.camera.xyz, 1.0);
        if clip.w > 0.01 {
            var uv = vec2<f32>(clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5);
            uv += normal.xz * (0.007 + roughness * 0.011);
            let edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
            if edge > 0.008 {
                let size = vec2<f32>(textureDimensions(reflected_scene));
                let blur = (0.5 + roughness * 3.0) / size;
                let c = textureSampleLevel(reflected_scene, environment_sampler, uv, 0.0).rgb * 0.50
                    + textureSampleLevel(reflected_scene, environment_sampler, uv + blur, 0.0).rgb * 0.25
                    + textureSampleLevel(reflected_scene, environment_sampler, uv - blur, 0.0).rgb * 0.25;
                if edge > 0.04 { return c; }
                return mix(sky_radiance(ray), c, smoothstep(0.008, 0.04, edge));
            }
        }
    }
    return sky_radiance(ray);
}

struct PrecipOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) data: vec2<f32>,
};
@vertex fn vs_precip(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> PrecipOut {
    let snow = instance >= 12288u;
    let local = select(instance, instance - 12288u, snow);
    let cell = vec2<f32>(f32(local % 32u), f32((local / 32u) % 32u)) - vec2<f32>(16.0) + floor(u.camera.xz / 4.0);
    let layer = f32(local / 1024u);
    let random = hash21(cell + vec2<f32>(layer * 5.13, layer * 9.31));
    let random2 = hash21(cell.yx + vec2<f32>(layer * 13.71, 4.19));
    let density = select(u.weather.y, u.weather.z, snow);
    let speed = select(25.0 + u.storm.z * 16.0, 1.5 + random2 * 1.7, snow);
    let time = u.params.x;
    var xz = cell * 4.0 + vec2<f32>(random, random2) * 4.0;
    let phase = fract((u.camera.y + 74.0 + time * speed - random * 148.0 - layer * 23.3) / 148.0);
    xz += u.storm.xy * (phase - 0.5) * select(0.70, 2.3, snow);
    xz += select(vec2<f32>(0.0), vec2<f32>(sin(time * 1.4 + random * 17.0), cos(time * 0.9 + random2 * 19.0)) * 0.65, snow);
    let y = u.camera.y + 74.0 - phase * 148.0;
    let center = vec3<f32>(xz.x, y, xz.y);
    let quad = array<vec2<f32>,6>(vec2<f32>(-1.,-1.),vec2<f32>(1.,-1.),vec2<f32>(1.,1.),vec2<f32>(-1.,-1.),vec2<f32>(1.,1.),vec2<f32>(-1.,1.));
    let q = quad[vertex];
    let right = vec3<f32>(cos(u.params.y), 0.0, sin(u.params.y));
    let up = vec3<f32>(-sin(u.params.y) * sin(u.params.z), cos(u.params.z), cos(u.params.y) * sin(u.params.z));
    let physical_width = select(0.013 + random2 * 0.012, 0.042 + random2 * 0.075, snow);
    // Keep rain visible at a 240–450p internal resolution without filling the
    // view with opaque lines. Snow retains its naturally varied flake size.
    let pixel_width = length(center - u.camera.xyz) * 1.4531 / max(u.settings.w,120.0);
    let width = max(physical_width,pixel_width * select(0.36,0.48,snow));
    let streak = select(normalize(vec3<f32>(u.storm.x * 0.14, -1.0, u.storm.y * 0.14)) * (0.5 + random * 0.75), up * width, snow);
    var p = center + right * q.x * width + streak * q.y;
    var out: PrecipOut;
    out.clip = u.view_projection * vec4<f32>(p - u.camera.xyz, 1.0);
    if random > density || density < 0.01 { out.clip = vec4<f32>(2.0,2.0,2.0,1.0); }
    out.world = p; out.uv = q; out.data = vec2<f32>(select(0.0,1.0,snow), density);
    return out;
}
@fragment fn fs_precip(v: PrecipOut) -> @location(0) vec4<f32> {
    let shelter = open_sky(v.world);
    if shelter < 0.3 { discard; }
    let snow = v.data.x;
    let shape = select((1.0 - abs(v.uv.x)) * (1.0 - abs(v.uv.y) * 0.45), 1.0 - smoothstep(0.35,1.0,length(v.uv)), snow > 0.5);
    let distance = length(v.world - u.camera.xyz);
    let fade = smoothstep(0.7,3.0,distance) * (1.0 - smoothstep(45.0,78.0,distance));
    let color = mix(vec3<f32>(0.40,0.49,0.60),vec3<f32>(0.80,0.88,1.0),snow) * (0.20 + daylight() * 0.80 + u.storm.w * 2.0);
    return vec4<f32>(color, shape * fade * shelter * mix(0.42,0.83,snow));
}
