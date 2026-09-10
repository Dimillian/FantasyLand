// Procedural low-poly materials and atmosphere. No sampled textures or skybox.
// params = elapsed seconds, camera yaw, camera pitch, hour of day.
// camera.w = viewport aspect; settings = near chunk radius, grass fade metres, 0, 0.
struct Globals {
    view_projection: mat4x4<f32>,
    camera: vec4<f32>,
    light: vec4<f32>,
    fog: vec4<f32>,
    params: vec4<f32>,
    settings: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Globals;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) material: f32,
};
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) @interpolate(flat) material: f32,
};

fn hash21(p: vec2<f32>) -> f32 {
    // Small-domain hashing avoids the large sine arguments that can form bands
    // at remote world coordinates. Stable in world space, including negative x/z.
    var q = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    q += vec3<f32>(dot(q, q.yzx + vec3<f32>(33.33)));
    return fract((q.x + q.y) * q.z);
}

fn noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let blend = f * f * (vec2<f32>(3.0) - 2.0 * f);
    return mix(
        mix(hash21(cell), hash21(cell + vec2<f32>(1.0, 0.0)), blend.x),
        mix(hash21(cell + vec2<f32>(0.0, 1.0)), hash21(cell + vec2<f32>(1.0, 1.0)), blend.x),
        blend.y
    );
}

fn fbm(p: vec2<f32>) -> f32 {
    // Rotated octaves keep weather and water patterns away from the terrain grid.
    let rotation = mat2x2<f32>(vec2<f32>(0.80, 0.60), vec2<f32>(-0.60, 0.80));
    var q = p;
    var value = noise(q) * 0.53;
    q = rotation * q * 2.03 + vec2<f32>(7.7, 3.1);
    value += noise(q) * 0.27;
    q = rotation * q * 2.09 + vec2<f32>(2.3, 8.4);
    value += noise(q) * 0.13;
    q = rotation * q * 2.01 + vec2<f32>(9.2, 1.7);
    value += noise(q) * 0.07;
    return value;
}

fn bayer(p: vec2<f32>) -> f32 {
    let x = u32(p.x) % 4u;
    let y = u32(p.y) % 4u;
    let matrix = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return matrix[y * 4u + x] / 16.0 - 0.5;
}

fn solar_elevation() -> f32 {
    return sin((u.params.w - 6.0) * 0.261799388);
}

fn daylight() -> f32 {
    return smoothstep(-0.08, 0.18, solar_elevation());
}

fn twilight() -> f32 {
    let elevation = solar_elevation();
    return smoothstep(-0.18, 0.02, elevation) * (1.0 - smoothstep(0.08, 0.42, elevation));
}

fn horizon_color(direction: vec3<f32>) -> vec3<f32> {
    let sunward = pow(max(dot(direction, normalize(u.light.xyz)), 0.0), 3.0);
    let day_haze = u.fog.rgb;
    let night_haze = vec3<f32>(0.115, 0.155, 0.205);
    let haze = mix(night_haze, day_haze, daylight());
    return mix(haze, vec3<f32>(0.67, 0.39, 0.24), twilight() * (0.15 + sunward * 0.45));
}

fn sky_gradient(direction: vec3<f32>) -> vec3<f32> {
    let day = daylight();
    let zenith = mix(vec3<f32>(0.025, 0.040, 0.078), vec3<f32>(0.235, 0.445, 0.685), day);
    let height = max(direction.y, 0.0);
    var color = mix(horizon_color(direction), zenith, pow(clamp(height, 0.0, 1.0), 0.52));
    // A wide atmospheric glow locates the sun without washing out the sky.
    let sunward = max(dot(direction, normalize(u.light.xyz)), 0.0);
    let glow = pow(sunward, 11.0) * 0.085 + pow(sunward, 80.0) * 0.07;
    let glow_color = mix(vec3<f32>(0.93, 0.43, 0.19), vec3<f32>(0.92, 0.86, 0.65), smoothstep(0.04, 0.5, solar_elevation()));
    color += glow_color * glow * day;
    return color;
}

fn transform_vertex(v: VertexIn) -> VertexOut {
    var o: VertexOut;
    var p = v.position;
    if v.material > 0.5 && v.material < 1.5 {
        let weight = clamp((1.4 - v.material) / 0.4, 0.0, 1.0);
        p.x += sin(u.params.x * 0.85 + p.x * 0.13 + p.z * 0.17) * 0.06 * weight;
    }
    if v.material > 5.5 && v.material < 6.5 {
        // Fractional material encodes bend weight; roots remain fixed.
        let gust = sin(u.params.x * 1.7 + p.x * 0.7 + p.z * 0.4);
        let weight = clamp((v.material - 6.0) / 0.4, 0.0, 1.0);
        p.x += gust * 0.065 * weight;
        p.z += sin(u.params.x * 1.3 + p.z * 0.8) * 0.035 * weight;
    }
    o.clip = u.view_projection * vec4<f32>(p - u.camera.xyz, 1.0);
    o.world = p;
    o.normal = v.normal;
    o.color = v.color;
    o.material = v.material;
    return o;
}

@vertex fn vs_main(v: VertexIn) -> VertexOut {
    return transform_vertex(v);
}

fn water_color(world: vec3<f32>, distance: f32, channel: vec3<f32>, footprint: f32) -> vec3<f32> {
    let t = u.params.x;
    let p = world.xz;
    let ocean = step(0.5, channel.z);
    let depth = max(channel.z - 1.0, 0.0);
    let flow = normalize(channel.xy + vec2<f32>(0.00001));
    let velocity = mix(flow * 1.4, vec2<f32>(0.72, 0.38), ocean);
    let near_detail = 1.0 - smoothstep(70.0, 350.0, distance);
    // Band-limit the moving ripples at cliff-top and horizon distances. Without
    // this their repeated normal pattern aliases into rings across the ocean.
    let wave_detail = 1.0 - smoothstep(1.5, 10.0, footprint);
    let fine_filter = 1.0 - smoothstep(0.6, 2.0, footprint);
    let fine_detail = near_detail * fine_filter;
    let warp = noise((p - velocity * t) * 0.035);
    let phase_a = dot(p, vec2<f32>(0.31, 0.17)) - t * dot(velocity, vec2<f32>(0.31, 0.17)) + warp * 3.5;
    let phase_b = dot(p, vec2<f32>(-0.19, 0.43)) - t * dot(velocity, vec2<f32>(-0.19, 0.43));
    let phase_c = dot(p, vec2<f32>(1.41, 0.63)) - t * dot(velocity, vec2<f32>(1.41, 0.63)) + warp * 5.0;
    let a = sin(phase_a) * wave_detail;
    let b = sin(phase_b) * wave_detail;
    let c = sin(phase_c) * fine_detail;
    let wave = a * 0.48 + b * 0.31 + c * 0.21;
    let slope_x = (cos(phase_a) * 0.035 + cos(phase_b) * -0.022) * wave_detail + cos(phase_c) * 0.035 * fine_detail;
    let slope_z = (cos(phase_a) * 0.019 + cos(phase_b) * 0.049) * wave_detail + cos(phase_c) * 0.019 * fine_detail;
    let normal = normalize(vec3<f32>(-slope_x, 1.0, -slope_z));
    let view = normalize(u.camera.xyz - world);
    let reflection = reflect(-view, normal);
    let fresnel = 0.08 + 0.55 * pow(1.0 - max(dot(normal, view), 0.0), 4.0);

    // The vertex color carries flow direction; the material palette comes from
    // continuous world-space noise so water chunk cells do not set its tone.
    let broad_current = noise((p - velocity * t) * 0.012);
    var color = mix(vec3<f32>(0.075, 0.285, 0.335), vec3<f32>(0.160, 0.435, 0.475), broad_current);
    let sea = mix(vec3<f32>(0.11, 0.47, 0.46), vec3<f32>(0.025, 0.16, 0.30), smoothstep(0.0, 45.0, depth));
    color = mix(color, sea * (0.94 + broad_current * 0.12), ocean);
    color *= u.light.w * (0.94 + wave * 0.12);
    var reflected_sky = sky_gradient(reflection);
    // Broad reflected weather is enough to suggest a real sky without tracing
    // the costly cloud layers again for every water fragment.
    let reflected_cloud = smoothstep(0.56, 0.78, fbm(reflection.xz / max(reflection.y, 0.12) * 1.2 + vec2<f32>(t * 0.0009, 4.1)));
    reflected_sky = mix(reflected_sky, vec3<f32>(0.63, 0.69, 0.67) * u.light.w, reflected_cloud * 0.22 * daylight());
    color = mix(color, reflected_sky, fresnel);

    // Interrupted flowing strokes, with fewer fine marks at grazing distance.
    let breaks = mix(0.5, smoothstep(0.30, 0.65, noise((p - velocity * t) * vec2<f32>(0.12, 0.35))), fine_filter);
    let crest = smoothstep(0.69, 0.91, a * 0.65 + c * 0.35) * breaks * near_detail;
    color += vec3<f32>(0.12, 0.15, 0.135) * crest * u.light.w;
    let half_vector = normalize(normalize(u.light.xyz) + view);
    let specular = pow(max(dot(normal, half_vector), 0.0), 115.0);
    let glint = smoothstep(0.40, 0.80, specular) * (0.3 + breaks * 0.7);
    color += vec3<f32>(0.42, 0.40, 0.29) * glint * daylight() * (0.30 + near_detail * 0.35);
    // Broken surf follows the interpolated seabed depth, so it traces coves and
    // headlands instead of drawing a straight wave across the shore.
    let surf_phase = depth * 1.65 - t * 1.15 + noise(p * 0.045) * 2.2;
    let surf = (1.0 - smoothstep(0.4, 3.5, depth)) * smoothstep(0.50, 0.91, sin(surf_phase));
    let wash = (1.0 - smoothstep(0.0, 0.3, depth)) * 0.45;
    color = mix(color, vec3<f32>(0.76, 0.83, 0.75) * u.light.w, max(surf * 0.75, wash) * ocean * (0.45 + breaks * 0.55));
    return color;
}

@fragment fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let distance = length(v.world - u.camera.xyz);
    let water_footprint = max(length(dpdx(v.world.xz)), length(dpdy(v.world.xz)));
    if v.material > 6.5 && v.material < 8.5 {
        // Near terrain replaces far patches exactly on the streamed chunk mask.
        let tile_delta = floor(v.world.xz / 192.0) - floor(u.camera.xz / 192.0);
        if length(tile_delta) <= u.settings.x + 0.5 {
            discard;
        }
    }
    if v.material > 5.5 && v.material < 6.5 {
        let grass_end = select(250.0, u.settings.y, u.settings.y > 1.0);
        let keep = 1.0 - smoothstep(grass_end * 0.66, grass_end, distance);
        if distance >= grass_end || hash21(floor(v.world.xz * 0.8)) > keep {
            discard;
        }
    }

    let normal = normalize(v.normal);
    let diffuse = max(dot(normal, normalize(u.light.xyz)), 0.0);
    let illumination = (0.76 + floor(diffuse * 5.0 + 0.5) / 5.0 * 0.38) * u.light.w;
    var color = v.color * illumination;
    var grain = hash21(floor(v.world.xz * 1.4)) - 0.5;
    if v.material > 1.5 && v.material < 3.5 {
        grain = hash21(floor(vec2<f32>(v.world.x + v.world.z, v.world.y) * 3.5)) - 0.5;
    }
    // Pixel grain fades before it can become a shimmering distant pattern.
    let grain_strength = (1.0 - smoothstep(80.0, 500.0, distance)) * 0.09;
    color *= 1.0 + grain * grain_strength;
    if v.material > 5.5 && v.material < 6.5 {
        // Backlit blades stay legible beside the darker opaque tree canopies.
        color = v.color * (0.85 + diffuse * 0.30) * u.light.w;
    }
    if (v.material > 3.5 && v.material < 4.5) || (v.material > 7.5 && v.material < 8.5) {
        color = water_color(v.world, distance, v.color, water_footprint);
    }

    // Retain the low-poly palette while restoring rich midtones in daylight.
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luma), color, 1.12);
    color = pow(max(color, vec3<f32>(0.0)), vec3<f32>(0.94));
    let direction = normalize(v.world - u.camera.xyz);
    let fog_distance = max(u.fog.w, 100.0);
    let fog_amount = 1.0 - exp(-pow(distance / fog_distance, 1.34));
    color = mix(color, horizon_color(direction), clamp(fog_amount, 0.0, 0.97));
    let dither = bayer(v.clip.xy) * 0.55;
    color = floor(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)) * 64.0 + dither) / 64.0;
    return vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
}

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs_sky(@builtin(vertex_index) index: u32) -> SkyOut {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: SkyOut;
    o.clip = vec4<f32>(positions[index], 0.99999, 1.0);
    o.uv = positions[index] * 0.5 + 0.5;
    return o;
}

fn sky_ray(uv: vec2<f32>) -> vec3<f32> {
    let yaw = u.params.y;
    let pitch = u.params.z;
    let forward = vec3<f32>(sin(yaw) * cos(pitch), sin(pitch), -cos(yaw) * cos(pitch));
    let right = vec3<f32>(cos(yaw), 0.0, sin(yaw));
    let up = vec3<f32>(-sin(yaw) * sin(pitch), cos(pitch), cos(yaw) * sin(pitch));
    let ndc = uv * 2.0 - 1.0;
    // tan(72deg / 2), matching Renderer::render's perspective_rh projection.
    return normalize(forward + right * ndc.x * max(u.camera.w, 0.1) * 0.72654253 + up * ndc.y * 0.72654253);
}

fn cloud_layer(ray: vec3<f32>, altitude: f32, scale: f32, drift: vec2<f32>, threshold: f32) -> vec2<f32> {
    let horizon_fade = smoothstep(0.018, 0.15, ray.y);
    let plane_distance = max(altitude - u.camera.y, 100.0) / max(ray.y, 0.015);
    let p = (u.camera.xz + ray.xz * plane_distance) * scale + drift;
    let weather = noise(p * 0.23 + vec2<f32>(13.4, 7.7));
    let density_field = fbm(p + vec2<f32>(noise(p * 0.6), noise(p * 0.6 + vec2<f32>(9.1, 2.4))) * 0.6);
    let density = smoothstep(threshold, threshold + 0.19, density_field + (weather - 0.5) * 0.17);
    // Coherent denser undersides and bright broken edges make clouds read as
    // large forms. The few tones match the flat-shaded geometry below.
    let thickness = smoothstep(threshold + 0.04, threshold + 0.25, density_field);
    return vec2<f32>(density * horizon_fade, floor(thickness * 4.0 + 0.5) / 4.0);
}

@fragment fn fs_sky(v: SkyOut) -> @location(0) vec4<f32> {
    let ray = sky_ray(v.uv);
    let day = daylight();
    let sun_dir = normalize(u.light.xyz);
    var color = sky_gradient(ray);

    // A compact warm sun is drawn before the clouds, so clouds occlude it.
    let sun_alignment = dot(ray, sun_dir);
    let sun_disk = smoothstep(0.99987, 0.99995, sun_alignment) * smoothstep(-0.015, 0.025, ray.y) * day;
    let sun_color = mix(vec3<f32>(1.0, 0.58, 0.27), vec3<f32>(1.0, 0.94, 0.74), smoothstep(0.02, 0.50, solar_elevation()));
    color = mix(color, sun_color, sun_disk);

    if ray.y > 0.0 {
        let t = u.params.x;
        // Thin high cloud streaks sit behind slower, substantial low cumulus.
        let high = cloud_layer(ray, 4800.0, 0.00029, vec2<f32>(t * 0.00055 + 21.0, -t * 0.00022), 0.51);
        let high_color = mix(vec3<f32>(0.13, 0.18, 0.25), vec3<f32>(0.70, 0.76, 0.76), day);
        color = mix(color, high_color, high.x * 0.35);

        let low = cloud_layer(ray, 2300.0, 0.00048, vec2<f32>(t * 0.00095, t * 0.00024), 0.48);
        let edge_light = pow(max(dot(ray, sun_dir), 0.0), 8.0);
        let cloud_lit = mix(vec3<f32>(0.25, 0.29, 0.34), vec3<f32>(0.85, 0.85, 0.77), day);
        let cloud_shade = mix(vec3<f32>(0.10, 0.135, 0.20), vec3<f32>(0.53, 0.60, 0.63), day);
        var cloud_color = mix(cloud_lit, cloud_shade, low.y * 0.8);
        cloud_color += vec3<f32>(0.09, 0.065, 0.025) * edge_light * (1.0 - low.y) * day;
        cloud_color = mix(cloud_color, vec3<f32>(0.75, 0.44, 0.28), twilight() * edge_light * 0.5);
        color = mix(color, cloud_color, low.x * 0.88);

        // Sparse world-oriented stars appear only above the night haze. Cubic
        // directional cells avoid a screen-space starfield that follows yaw.
        let stars_cell = floor(ray.xz / max(ray.y + 0.25, 0.25) * 195.0);
        let star = step(0.9978, hash21(stars_cell)) * smoothstep(0.15, 0.50, ray.y);
        let star_strength = (1.0 - day) * (1.0 - low.x) * (1.0 - high.x * 0.5);
        color += vec3<f32>(0.62, 0.68, 0.74) * star * star_strength;
    }

    let dither = bayer(v.clip.xy) * 0.60;
    color = floor(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)) * 80.0 + dither) / 80.0;
    return vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
}
