// One overhead depth map is shared by ambient enclosure, snow deposition and
// precipitation shelter. It renders actual nearby trees, terrain and rocks.
@group(0) @binding(3) var shelter_depth: texture_depth_2d;
@group(0) @binding(4) var reflected_scene: texture_2d<f32>;
@group(0) @binding(5) var environment_sampler: sampler;
@group(0) @binding(6) var submerged_scene: texture_2d<f32>;
@group(0) @binding(7) var submerged_depth: texture_depth_2d;

fn water_bed_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let forward = vec3<f32>(sin(u.params.y)*cos(u.params.z),sin(u.params.z),-cos(u.params.y)*cos(u.params.z));
    let right = vec3<f32>(cos(u.params.y),0.0,sin(u.params.y));
    let up = vec3<f32>(-sin(u.params.y)*sin(u.params.z),cos(u.params.z),cos(u.params.y)*sin(u.params.z));
    let ndc = vec2<f32>(uv.x*2.0-1.0,1.0-uv.y*2.0);
    let linear_depth = 0.08 / max(1.0-depth*(1.0-0.08/36000.0),0.000001);
    return u.camera.xyz + (forward+(right*ndc.x*u.camera.w+up*ndc.y)*0.72654253)*linear_depth;
}
fn refracted_water(world: vec3<f32>, normal: vec3<f32>, scattering: vec3<f32>, rain: f32, ocean: f32) -> vec3<f32> {
    let clip = u.view_projection * vec4<f32>(world-u.camera.xyz,1.0);
    if clip.w <= 0.01 { return scattering; }
    let size = vec2<f32>(textureDimensions(submerged_scene));
    let uv = vec2<f32>(clip.x/clip.w*0.5+0.5,0.5-clip.y/clip.w*0.5);
    let right = vec3<f32>(cos(u.params.y),0.0,sin(u.params.y));
    let up = vec3<f32>(-sin(u.params.y)*sin(u.params.z),cos(u.params.z),cos(u.params.y)*sin(u.params.z));
    let distortion = vec2<f32>(dot(normal,right),-dot(normal-vec3<f32>(0.0,1.0,0.0),up))*0.024;
    let water_depth = clip.z/clip.w;
    // Each color sample uses the exact same texel as its validated depth. A
    // farther dry bank is still dry: reject its reconstructed world elevation.
    for (var attempt=0u;attempt<2u;attempt+=1u) {
        let offset = select(distortion,vec2<f32>(0.0),attempt==1u);
        let sample_uv = uv+offset;
        if any(sample_uv<vec2<f32>(0.0)) || any(sample_uv>=vec2<f32>(1.0)) { continue; }
        let pixel = vec2<i32>(sample_uv*size);
        let texel_uv = (vec2<f32>(pixel)+vec2<f32>(0.5))/size;
        let bed_depth = textureLoad(submerged_depth,pixel,0);
        if bed_depth<=water_depth || bed_depth>=0.999999 { continue; }
        let bed_world = water_bed_position(texel_uv,bed_depth);
        if bed_world.y>world.y+0.035 { continue; }
        let thickness = clamp(length(bed_world-u.camera.xyz)-length(world-u.camera.xyz),0.0,40.0);
        let absorption = mix(vec3<f32>(0.28,0.13,0.10),vec3<f32>(0.22,0.075,0.045),ocean)*(1.0+rain*0.75);
        let transmission = exp(-absorption*thickness);
        let bed = textureLoad(submerged_scene,pixel,0).rgb;
        return bed*transmission+scattering*(vec3<f32>(1.0)-transmission);
    }
    return scattering;
}

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
    if u.reflection_params.y > 0.5 && u.reflection_params.z < 0.5 && abs(world.y - u.reflection_params.x) < 1.0 {
        let clip = u.reflection_matrix * vec4<f32>(world - u.camera.xyz, 1.0);
        if clip.w > 0.01 {
            var uv = vec2<f32>(clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5);
            let right = vec3<f32>(cos(u.params.y),0.0,sin(u.params.y));
            let up = vec3<f32>(-sin(u.params.y)*sin(u.params.z),cos(u.params.z),cos(u.params.y)*sin(u.params.z));
            let deviation = normal - vec3<f32>(0.0,1.0,0.0);
            uv += vec2<f32>(dot(deviation,right),-dot(deviation,up)) * (0.010 + roughness * 0.008);
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

// Same bounded metres/second response as precipitation::drift_velocity.
// Renderer integrates this velocity using real frame time; changing weather
// never multiplies the new velocity by the total age of the application.
fn precipitation_drift(snow: bool) -> vec2<f32> {
    let speed = length(u.storm.xy);
    let bounded_speed = min(speed, 26.0);
    let storm = smoothstep(8.0, 22.0, bounded_speed);
    let response = select(mix(0.025, 0.14, storm), mix(0.07, 0.16, storm), snow);
    return u.storm.xy * (response * bounded_speed / max(speed, 0.00001));
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
    let advection = select(u.precipitation_offset.xy, u.precipitation_offset.zw, snow);
    // Inverse advection selects stable emitters surrounding the camera. Moving
    // the camera only exchanges emitters at the distant edge of this lattice.
    let cell = vec2<f32>(f32(local % 32u), f32((local / 32u) % 32u)) - vec2<f32>(16.0)
        + floor((u.camera.xz - advection) / 4.0);
    // CPU offsets wrap at 4096 metres; equivalent emitter keys must wrap at
    // 1024 cells so both positions and hashes remain identical across wrapping.
    let key = cell - floor(cell / 1024.0) * 1024.0;
    let layer = f32(local / 1024u);
    let random = hash21(key + vec2<f32>(layer * 5.13, layer * 9.31));
    let random2 = hash21(key.yx + vec2<f32>(layer * 13.71, 4.19));
    let density = select(u.weather.y, u.weather.z, snow);
    // Constant per-particle fall speed: gusts no longer change time*speed and
    // teleport every drop or make rainfall appear to accelerate.
    let speed = select(25.0 + random2 * 8.0, 1.5 + random2 * 1.7, snow);
    let time = u.params.x;
    var xz = cell * 4.0 + vec2<f32>(random, random2) * 4.0 + advection;
    let phase = fract((u.camera.y + 88.0 + time * speed - random * 176.0 - layer * 23.3) / 176.0);
    // This expression cancels continuous camera altitude changes. The vertical
    // repeat switches only beyond the 78 m visibility limit; x/z never reset.
    let y = u.camera.y + 88.0 - phase * 176.0;
    xz += select(vec2<f32>(0.0), vec2<f32>(sin(time * 1.2 + random * 17.0) * 0.26,
        cos(time * 0.83 + random2 * 19.0) * 0.22), snow);
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
    // Streaks follow the very same velocity integrated into drop positions.
    // Ordinary rain is within half a degree of gravity; even tempest drift is
    // bounded to roughly eight degrees. Snow remains a fluttering billboard.
    let drift = precipitation_drift(snow);
    let velocity = vec3<f32>(drift.x, -speed, drift.y);
    let streak = select(velocity * (0.015 + random * 0.009), up * width, snow);
    let p = center + right * q.x * width + streak * q.y;
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
    // New horizontal emitters appear outside this fade, including when wind
    // advects the inverse lattice past the camera or its offsets wrap.
    let field_fade = 1.0 - smoothstep(43.0,58.0,length(v.world.xz - u.camera.xz));
    let fade = smoothstep(0.7,3.0,distance) * (1.0 - smoothstep(45.0,78.0,distance)) * field_fade;
    let color = mix(vec3<f32>(0.40,0.49,0.60),vec3<f32>(0.80,0.88,1.0),snow) * (0.20 + daylight() * 0.80 + u.storm.w * 2.0);
    return vec4<f32>(color, shape * fade * shelter * mix(0.42,0.83,snow));
}
