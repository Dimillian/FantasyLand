// Procedural low-poly pigments and atmosphere. No authored surface textures.
// params = elapsed seconds, camera yaw, camera pitch, hour of day.
// camera.w = viewport aspect; settings = near chunk radius, grass fade metres, 0, 0.
// distant = canopy start metres, canopy end metres, transition width metres, 0.
// climate = woodland cover, wetland mist, sandstone weight, mountain/exposure.
// air = local valley/water altitude, wind strength, moisture, reserved.
struct Globals {
    view_projection: mat4x4<f32>,
    camera: vec4<f32>,
    light: vec4<f32>,
    fog: vec4<f32>,
    params: vec4<f32>,
    settings: vec4<f32>,
    shadow_matrix: mat4x4<f32>,
    shadow_origin: vec4<f32>,
    shadow_params: vec4<f32>,
    distant: vec4<f32>,
    climate: vec4<f32>,
    air: vec4<f32>,
    // Actual bodies are independent of the dominant light stored in u.light.
    solenne: vec4<f32>, // direction, daylight
    aster: vec4<f32>,   // direction, angular radius
    vey: vec4<f32>,     // direction, angular radius
    direct: vec4<f32>, // primary light RGB, intensity
    ambient: vec4<f32>, // hemispheric tint, star visibility
    weather: vec4<f32>, // cloud cover, rain, snow, fog
    storm: vec4<f32>,   // wind x/z m/s, gust, lightning
    surface: vec4<f32>, // wetness, accumulated snow, temperature C, weather seconds
    precipitation_offset: vec4<f32>, // integrated rain x/z and snow x/z drift
    reflection_matrix: mat4x4<f32>,
    reflection_params: vec4<f32>,
    shelter_matrix: mat4x4<f32>,
    shelter_origin: vec4<f32>,
    shelter_params: vec4<f32>,
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

fn solar_elevation() -> f32 { return u.solenne.y; }
fn daylight() -> f32 { return u.solenne.w; }
fn twilight() -> f32 {
    let elevation = solar_elevation();
    return smoothstep(-0.21, -0.01, elevation) * (1.0 - smoothstep(0.10, 0.43, elevation));
}

fn horizon_color(direction: vec3<f32>) -> vec3<f32> {
    let sunward = pow(max(dot(direction, u.solenne.xyz), 0.0), 4.0);
    let day_haze = u.fog.rgb;
    let night_haze = vec3<f32>(0.048, 0.073, 0.12);
    let haze = mix(night_haze, day_haze, daylight());
    let twilight_haze = mix(vec3<f32>(0.245, 0.205, 0.33), vec3<f32>(0.78, 0.365, 0.16), sunward);
    let fair = mix(haze, twilight_haze, twilight() * (0.38 + sunward * 0.40));
    let overcast = smoothstep(0.60, 1.0, u.weather.x);
    let rain_air = mix(vec3<f32>(0.035, 0.056, 0.095), vec3<f32>(0.32, 0.38, 0.43), daylight());
    let snow_air = mix(vec3<f32>(0.09, 0.125, 0.185), vec3<f32>(0.65, 0.70, 0.73), daylight());
    let weather_air = mix(rain_air, snow_air, clamp(u.weather.z * 0.8 + u.surface.y * 0.2, 0.0, 1.0));
    return mix(fair, weather_air, overcast * (0.43 + u.weather.w * 0.40))
        + vec3<f32>(0.30, 0.38, 0.53) * u.storm.w;
}

fn sky_gradient(direction: vec3<f32>) -> vec3<f32> {
    let day = daylight();
    let zenith = mix(vec3<f32>(0.008, 0.015, 0.035), vec3<f32>(0.105, 0.295, 0.57), day);
    let height = max(direction.y, 0.0);
    var color = mix(horizon_color(direction), zenith, pow(clamp(height, 0.0, 1.0), 0.43));
    // Purple upper twilight and a compact amber forward scatter preserve depth.
    color = mix(color, vec3<f32>(0.125, 0.105, 0.235), twilight() * height * 0.40);
    let sunward = max(dot(direction, u.solenne.xyz), 0.0);
    let glow = pow(sunward, 14.0) * 0.095 + pow(sunward, 100.0) * 0.09;
    let glow_color = mix(vec3<f32>(1.0, 0.43, 0.15), vec3<f32>(1.0, 0.88, 0.60), smoothstep(0.04, 0.5, solar_elevation()));
    color += glow_color * glow * smoothstep(-0.13, 0.04, solar_elevation());
    let moonward = max(dot(direction, u.aster.xyz), 0.0);
    color += vec3<f32>(0.055, 0.080, 0.15) * pow(moonward, 32.0) * u.ambient.w * smoothstep(0.0, 0.10, u.aster.y);
    return color;
}

// Identical helper in shadow.wgsl. The prevailing wind agrees with rainfall's
// east-northeast direction. Broad gusts move neighboring plants together;
// the small crosswind oscillation prevents a rigid synchronized lean.
fn vegetation_wind(world: vec3<f32>, time: f32, strength: f32) -> vec2<f32> {
    let direction = normalize(u.storm.xy + vec2<f32>(0.001,0.0));
    let across = vec2<f32>(-direction.y,direction.x);
    let wave = time * 0.70 - dot(world.xz, direction) * 0.026;
    let cross_wave = time * 0.39 + dot(world.xz, across) * 0.019;
    let gust = 0.53 + sin(wave) * 0.27 + sin(cross_wave) * 0.16;
    let flutter = sin(time * 1.9 + dot(world.xz, vec2<f32>(0.31, 0.23))) * 0.08;
    return (direction * (gust + flutter) + across * sin(cross_wave) * 0.12)
         * clamp(strength, 0.0, 2.5);
}

fn transform_vertex(v: VertexIn) -> VertexOut {
    var o: VertexOut;
    var p = v.position;
    if v.material > 0.5 && v.material < 1.5 {
        let weight = clamp((1.4 - v.material) / 0.4, 0.0, 1.0);
        let bend = vegetation_wind(p, u.params.x, u.air.y) * (0.32 * weight);
        p.x += bend.x;
        p.z += bend.y;
    }
    if v.material > 5.5 && v.material < 6.5 {
        // Fractional material encodes bend weight; roots remain fixed.
        let weight = clamp((v.material - 6.0) / 0.4, 0.0, 1.0);
        let bend = vegetation_wind(p, u.params.x, u.air.y) * (0.11 * weight * weight);
        p.x += bend.x;
        p.z += bend.y;
    }
    var surface_normal = v.normal;
    if ((v.material > 3.5 && v.material < 4.5) || (v.material > 7.5 && v.material < 8.5)) && v.color.z > 0.5 {
        // Broad geometric sea swells survive coarse terrain tessellation. Near
        // shore they converge to the exact clipped coastline; fine ripples are
        // confined to the water material, and freshwater heights stay unchanged.
        let depth = max(v.color.z - 1.0,0.0);
        let direction = vec2<f32>(0.86,0.510294);
        let crosswind = vec2<f32>(-direction.y,direction.x);
        let wind = smoothstep(2.0,22.0,length(u.storm.xy));
        let amplitude = (0.035 + wind * 0.31) * smoothstep(0.5,12.0,depth);
        let k = 0.06981317;
        let phase = dot(p.xz,direction) * k - u.params.x * 0.67;
        let cross_phase = dot(p.xz,direction * 0.62 + crosswind * 0.78) * k * 0.71 - u.params.x * 0.47;
        p.y += (sin(phase) + sin(cross_phase) * 0.48) * amplitude;
        let slope = (cos(phase) * direction + cos(cross_phase) * (direction * 0.62 + crosswind * 0.78) * 0.3408) * amplitude * k;
        surface_normal = normalize(vec3<f32>(-slope.x,1.0,-slope.y));
    }
    if (v.material > 3.5 && v.material < 4.5) || (v.material > 7.5 && v.material < 8.5) {
        let local = local_water(v.position);
        let shore = smoothstep(0.02, 0.65, max(abs(v.color.z) - 1.0, 0.0));
        p.y += local.height * shore;
    }
    o.clip = u.view_projection * vec4<f32>(p - u.camera.xyz, 1.0);
    o.world = p;
    o.normal = surface_normal;
    o.color = v.color;
    o.material = v.material;
    return o;
}

@vertex fn vs_main(v: VertexIn) -> VertexOut {
    return transform_vertex(v);
}

// Water inputs: color.xy is downstream velocity in metres/second, zero for
// ocean/retained lakes. color.z retains signed depth (+sea,-freshwater), biased1.
// Optional geometry-owned foam sheets reserve speed6..8; normal currents≤4.5.
// Local surface displacements taper to zero at the clipped shoreline.
fn water_unit(v: vec2<f32>, fallback: vec2<f32>) -> vec2<f32> {
    let n = length(v);
    if n < 0.001 { return fallback; }
    return v / n;
}

fn water_rain_slope(p: vec2<f32>, time: f32, rain: f32, detail: f32) -> vec2<f32> {
    if rain * detail < 0.015 { return vec2<f32>(0.0); }
    // Four neighboring deterministic rain cells, each a growing finite ring.
    // Detail vanishes before the ring becomes subpixel or reaches the horizon.
    let grid = p / 3.0;
    let base = floor(grid - vec2<f32>(0.5));
    var slope = vec2<f32>(0.0);
    for (var i = 0u; i < 4u; i += 1u) {
        let cell = base + vec2<f32>(f32(i & 1u), f32(i >> 1u));
        let seed = hash21(cell + vec2<f32>(17.3, 81.1));
        let jitter = vec2<f32>(seed, hash21(cell + vec2<f32>(41.2, 6.7))) - vec2<f32>(0.5);
        let center = (cell + vec2<f32>(0.5) + jitter * 0.35) * 3.0;
        let delta = p - center;
        let radius = length(delta);
        let age = fract(time * 0.72 + seed);
        let front = 0.09 + age * 1.24;
        let band = radius - front;
        let envelope = (1.0 - smoothstep(0.035, 0.19, abs(band)))
                     * (1.0 - age) * smoothstep(0.0, 0.09, age);
        slope += delta / max(radius, 0.035) * cos(band * 37.0) * envelope;
    }
    return slope * rain * detail * 0.037;
}

fn water_color(world: vec3<f32>, distance: f32, channel: vec3<f32>, footprint: f32, surface_normal: vec3<f32>) -> vec3<f32> {
    let time = u.params.x;
    let p = world.xz;
    let ocean = step(0.5, channel.z);
    let has_depth = step(0.5, abs(channel.z));
    let depth = mix(5.0, max(abs(channel.z) - 1.0, 0.0), has_depth);
    let speed = length(channel.xy);
    let stream = smoothstep(0.08, 0.28, speed) * (1.0 - ocean);
    let sheet = smoothstep(5.6, 7.7, speed) * (1.0 - ocean);
    let river_speed = min(speed, 4.5);
    let wind_vector = u.storm.xy;
    let wind_speed = length(wind_vector) * (1.0 + clamp(u.storm.z, 0.0, 1.0) * 0.25);
    let wind = smoothstep(0.4, 15.0, wind_speed);
    // Fixed world-space wave vectors and frequencies: weather changes their
    // energy, never rotates a phase field spanning hundreds of kilometres.
    let wind_direction = vec2<f32>(0.910366, -0.413803);
    let flow = water_unit(channel.xy, wind_direction);
    let across = vec2<f32>(-wind_direction.y, wind_direction.x);
    let rain = clamp(u.weather.y, 0.0, 1.0);
    let near = 1.0 - smoothstep(110.0, 520.0, distance);
    let fine = near * (1.0 - smoothstep(0.28, 1.5, footprint));
    let rain_detail = (1.0 - smoothstep(36.0, 115.0, distance))
                    * (1.0 - smoothstep(0.12, 0.48, footprint));
    let rapids = smoothstep(1.35, 3.45, river_speed) * stream;
    // Dual-phase downstream advection only stretches a pattern for six seconds.
    // The second sample takes over while the first resets invisibly.
    let cycle = fract(time / 6.0);
    let flow_weight = 0.5 - 0.5 * cos(cycle * 6.2831853);
    let advect = flow * river_speed * stream;
    let moving_a = p - advect * (cycle * 6.0);
    let moving_b = p - advect * (fract(cycle + 0.5) * 6.0);
    let local = local_water(world);
    let sim_detail = local.weight * (1.0 - smoothstep(0.7, 2.4, footprint));

    // Centimetre-scale surface waves keep calm-water reflections coherent.
    // Simulated neighbours supply local waves; analytic detail handles sub-cell
    // capillary ripples and the distant water outside the bounded grid.
    let wavelength = mix(5.6, 18.0, ocean);
    let broad = 1.0 - smoothstep(wavelength * 0.10, wavelength * 0.40, footprint);
    let wave_k = 6.2831853 / wavelength;
    let warp = noise(p * 0.021) * 0.55;
    let phase_a = dot(p, wind_direction) * wave_k - time * 1.05 + warp;
    let direction_b = wind_direction * 0.58 + across * 0.82;
    let phase_b = dot(p, direction_b) * wave_k * 1.72 - time * 1.39;
    let phase_c_a = dot(moving_a, flow * 1.9 + vec2<f32>(-flow.y, flow.x) * 0.52) + warp;
    let phase_c_b = dot(moving_b, flow * 1.9 + vec2<f32>(-flow.y, flow.x) * 0.52) + warp;
    let amplitude = mix(0.006, 0.014, ocean) + wind * mix(0.035, 0.075, ocean);
    var slope = (wind_direction * cos(phase_a) + direction_b * cos(phase_b) * 0.42)
              * amplitude * broad * (1.0 - sim_detail * 0.65);
    slope += flow * mix(cos(phase_c_b), cos(phase_c_a), flow_weight) * (rapids * 0.075 + wind * 0.009) * fine;
    slope += water_rain_slope(p, time, rain, rain_detail) * (1.0 - sim_detail * 0.75);
    slope += local.slope * (1.0 - smoothstep(0.7, 2.4, footprint));
    let normal = normalize(normalize(surface_normal) - vec3<f32>(slope.x, 0.0, slope.y));
    let view = normalize(u.camera.xyz - world);
    let facing = clamp(dot(normal, view), 0.0, 1.0);
    let roughness = clamp(0.035 + wind * mix(0.21, 0.31, ocean) + rain * 0.10 + rapids * 0.27, 0.035, 0.60);
    let fresnel = 0.0204 + 0.9796 * pow(1.0 - facing, 5.0);

    // Authored pigment is converted once to linear light. Clear shallows expose
    // warm gravel; deeper channels shift through jade toward blue-green.
    let sandstone = clamp(u.climate.z, 0.0, 1.0);
    let sediment = mix(vec3<f32>(0.39, 0.43, 0.285), vec3<f32>(0.52, 0.385, 0.215), sandstone * 0.75);
    let river_tone = mix(sediment, vec3<f32>(0.055, 0.275, 0.285), 1.0 - exp(-depth * 0.40));
    let sea_tone = mix(vec3<f32>(0.34, 0.57, 0.47), vec3<f32>(0.025, 0.205, 0.355), 1.0 - exp(-depth * 0.085));
    let pigment = pow(max(mix(river_tone, sea_tone, ocean), vec3<f32>(0.0)), vec3<f32>(2.2));
    let visibility = sun_visibility(world, normal) * weather_light_visibility(world);
    let sky_fill = u.ambient.rgb * 0.76;
    let direct = u.direct.rgb * u.direct.w * max(dot(normal, normalize(u.light.xyz)), 0.0) * 0.34 * visibility;
    var color = pigment * (sky_fill + direct) * (0.96 + noise(p * 0.012) * 0.08);

    // Transmit the actual bed/rocks below the surface with depth-dependent
    // absorption. Screen-space offsets reject foreground shoreline samples.
    color = refracted_water(world, normal, color, rain, ocean);

    var reflected: vec3<f32>;
    // A sloping river is not a planar lake: keep its cheap, physically coherent
    // environment fallback rather than stretching unrelated terrain over it.
    // Mutually exclusive branches avoid evaluating the procedural sky twice.
    if stream < 0.5 {
        reflected = reflected_environment(world, normal, view, roughness);
    } else {
        reflected = sky_radiance(reflect(-view, normal));
    }
    color = mix(color, reflected, fresnel * (1.0 - rapids * 0.30));

    // Shallow caustics stay gentle and disappear in rough, turbid rainwater.
    let shallow = exp(-depth * 0.72) * has_depth;
    let caustic_a = 1.0 - abs(mix(sin(phase_c_b + sin(phase_a) * 0.74), sin(phase_c_a + sin(phase_a) * 0.74), flow_weight));
    let caustic_b = 1.0 - abs(mix(sin(phase_c_b * 0.73 - phase_b * 0.57), sin(phase_c_a * 0.73 - phase_b * 0.57), flow_weight));
    let caustic = smoothstep(0.69, 0.94, min(caustic_a, caustic_b));
    color += vec3<f32>(0.045, 0.068, 0.032) * caustic * shallow * fine * visibility
           * daylight() * (1.0 - rain * 0.75) * (1.0 - wind * 0.58);

    // Finite-width microfacet light paths bloom only where sun/moon geometry
    // supports them. Clamp the distribution to suppress low-resolution fireflies.
    let light = normalize(u.light.xyz);
    let half_sum = light + view;
    let half_vector = half_sum / sqrt(max(dot(half_sum, half_sum), 0.00001));
    let nl = max(dot(normal, light), 0.0);
    let nh = max(dot(normal, half_vector), 0.0);
    let vh = max(dot(view, half_vector), 0.0);
    let alpha = max(roughness * roughness, 0.009);
    let alpha2 = alpha * alpha;
    let denominator = nh * nh * (alpha2 - 1.0) + 1.0;
    let distribution = min(alpha2 / max(3.14159265 * denominator * denominator, 0.000001), 90.0);
    let k = (roughness + 1.0) * (roughness + 1.0) * 0.125;
    let masking = nl / max(nl * (1.0 - k) + k, 0.0001)
                * facing / max(facing * (1.0 - k) + k, 0.0001);
    let specular_fresnel = 0.0204 + 0.9796 * pow(1.0 - vh, 5.0);
    let specular = distribution * masking * specular_fresnel / max(4.0 * nl * facing, 0.025);
    color += u.direct.rgb * u.direct.w * nl * specular * 2.8 * visibility;

    let broken = smoothstep(0.32, 0.76, mix(noise(moving_b * vec2<f32>(0.13, 0.23)), noise(moving_a * vec2<f32>(0.13, 0.23)), flow_weight));
    let riffle = smoothstep(0.52, 0.91, mix(sin(phase_c_b + sin(phase_b) * 0.80), sin(phase_c_a + sin(phase_b) * 0.80), flow_weight));
    let turbulent = rapids * riffle * (0.20 + broken * 0.80)
                  * (0.25 + (1.0 - smoothstep(1.2, 7.0, depth)) * 0.75) * fine;
    let shore_zone = smoothstep(0.05, 0.23, depth) * (1.0 - smoothstep(0.65, 2.8, depth));
    let wash = smoothstep(0.64, 0.94, sin(depth * 2.0 - time * 0.86 + warp * 2.3));
    let surf = ocean * shore_zone * wash * (0.12 + wind * 0.64) * broken * fine;
    let whitecap = ocean * smoothstep(0.48, 0.90, wind) * smoothstep(0.91, 0.995, sin(phase_a))
                 * broken * broad * (1.0 - smoothstep(900.0, 2800.0, distance));
    let cascade = sheet * (0.48 + 0.52 * riffle) * (0.42 + broken * 0.58) * near;
    let foam = clamp(turbulent * 0.64 + surf * 0.64 + whitecap * 0.28 + cascade * 0.84 + local.foam * fine * 0.72, 0.0, 0.92);
    let foam_light = (u.ambient.rgb * 0.64 + u.direct.rgb * u.direct.w * nl * 0.48 * visibility)
                   * vec3<f32>(0.80, 0.88, 0.84);
    color = mix(color, foam_light, foam);
    // Permanent contact darkening follows actual clipped depth, never a fake
    // screen-space shoreline that leaks across the terrain or floating props.
    let contact = (1.0 - smoothstep(0.018, 0.20, depth)) * has_depth;
    color *= 1.0 - contact * 0.12;
    return max(color, vec3<f32>(0.0));
}

// Sun visibility is supplied by lighting.wgsl; ambient stays lit.


// Persistent land-scale pigment sits beneath the fine pixels and actual plants.
// Rotated, softly warped fields have no relationship to terrain chunk/triangle
// boundaries. Their sizes span grass colonies through whole hillside clearings.
fn terrain_pigment(base: vec3<f32>, world: vec3<f32>, normal: vec3<f32>, footprint: f32) -> vec3<f32> {
    let rotation = mat2x2<f32>(vec2<f32>(0.80, 0.60), vec2<f32>(-0.60, 0.80));
    let q = rotation * world.xz;
    let broad = noise(q * 0.0027 + vec2<f32>(31.7, -19.4));
    let drift = noise(q * 0.0039 + vec2<f32>(-8.3, 43.1));
    let warped = q + (vec2<f32>(broad, drift) - vec2<f32>(0.5)) * 95.0;
    let field = noise(warped * 0.013 + vec2<f32>(4.6, 17.8));
    let broad_filter = 1.0 - smoothstep(65.0, 230.0, footprint);
    let field_filter = 1.0 - smoothstep(12.0, 55.0, footprint);
    // Multiplicative shades preserve sandstone, chalk, snow, and each biome's
    // authored-in-code vertex palette instead of tinting all hills green.
    var color = base * (1.0 + (broad - 0.5) * 0.20 * broad_filter
                            + (field - 0.5) * 0.19 * field_filter);
    let grass = smoothstep(0.018, 0.105, base.g - base.r);
    let meadow = smoothstep(0.38, 0.72, field) * field_filter * grass;
    color = mix(color, color * vec3<f32>(1.09, 1.035, 0.89), meadow * 0.55);

    // Low colonies remain visible after individual grass blades disappear.
    // Fade by projected pixel size, never by camera distance: there is no new
    // circular transition to reveal the streamed ground-cover radius.
    let colony_filter = 1.0 - smoothstep(2.2, 12.0, footprint);
    if colony_filter > 0.001 {
        let colony = noise(warped * 0.105 + vec2<f32>(11.2, -3.9));
        let clustered = smoothstep(0.46, 0.75, colony + (field - 0.5) * 0.24);
        let earth = 1.0 - grass;
        color *= 1.0 - clustered * colony_filter * (0.15 * grass + 0.065 * earth);
        color = mix(color, color * vec3<f32>(0.86, 1.035, 0.87), clustered * colony_filter * grass * 0.30);
    }

    // Elevation-stretched noise exposes broken strata on steep ground; its
    // irregular ends avoid contour rings. Existing pale/dark stone stays local.
    let exposure = smoothstep(0.075, 0.42, 1.0 - clamp(normal.y, 0.0, 1.0));
    let strata_filter = 1.0 - smoothstep(5.0, 32.0, footprint);
    if exposure * strata_filter > 0.001 {
        let strata = noise(vec2<f32>(q.x * 0.010 + q.y * 0.005,
                                    world.y * 0.052 + broad * 1.8));
        let outcrop = smoothstep(0.44, 0.73, strata + (field - 0.5) * 0.30) * exposure * strata_filter;
        let luminance = dot(base, vec3<f32>(0.30, 0.59, 0.11));
        let stone = mix(base, vec3<f32>(luminance) * vec3<f32>(1.09, 1.055, 0.96), grass * 0.58);
        color = mix(color, stone * (0.91 + strata * 0.18), outcrop * 0.54);
    }
    return color;
}

fn surface_plane(p: vec3<f32>, normal: vec3<f32>) -> vec2<f32> {
    if abs(normal.y) > max(abs(normal.x), abs(normal.z)) { return p.xz; }
    return select(p.zy, p.xy, abs(normal.z) > abs(normal.x));
}

// Centimetre grain is deliberately quiet; the 0.5-4m colonies below carry the
// useful detail of moss, humus and exposed sediment while walking. They remain
// tied to substrate pigments, slope, and sampled regional moisture.
fn surface_communities(base: vec3<f32>, world: vec3<f32>, normal: vec3<f32>, material: f32, footprint: f32, distance: f32) -> vec3<f32> {
    let detail = 1.0 - smoothstep(0.22, 1.6, footprint);
    if detail <= 0.001 { return base; }
    let ground = material < 0.5 || (material > 6.5 && material < 7.5);
    let stone = material > 1.5 && material < 2.5;
    if !ground && !stone { return base; }
    let regional = 1.0 - smoothstep(1800.0, 6500.0, distance);
    let moist = clamp(u.air.z, 0.0, 1.0) * regional;
    let forest = clamp(u.climate.x, 0.0, 1.0) * regional;
    let plane = surface_plane(world, normal);
    let patches = noise(plane * 0.41 + vec2<f32>(13.2, 8.7));
    let small = noise(plane * 1.73 + vec2<f32>(-7.8, 11.1));
    let top = smoothstep(0.18, 0.85, normal.y);
    var color = base;
    if ground {
        let grass = smoothstep(0.014, 0.11, base.g - base.r);
        let humus = smoothstep(0.43, 0.72, patches) * forest * grass * top;
        let soil = base * vec3<f32>(0.92, 0.79, 0.70);
        color = mix(color, soil, humus * 0.28);
        let moss = smoothstep(0.53, 0.75, patches + small * 0.13) * moist * grass * top;
        color = mix(color, base * vec3<f32>(0.80, 1.045, 0.86), moss * (0.16 + forest * 0.19));
        let litter = smoothstep(0.62, 0.80, small) * smoothstep(0.38, 0.65, patches) * forest * top;
        color = mix(color, vec3<f32>(0.34, 0.28, 0.16), litter * 0.20);
        let dry_soil = (1.0 - grass) * (1.0 - moist * 0.6);
        color *= 1.0 + (small - 0.5) * 0.055 * dry_soil;
    } else {
        let warm = smoothstep(0.04, 0.24, base.r - base.b);
        let strata = noise(vec2<f32>(dot(world.xz, vec2<f32>(0.21, 0.13)), world.y * 1.8 + patches * 0.8));
        let seam = smoothstep(0.56, 0.74, strata) * warm;
        color *= 1.0 - seam * 0.11;
        let lichen = smoothstep(0.55, 0.78, patches + small * 0.11) * (0.22 + top * 0.78);
        color = mix(color, base * vec3<f32>(1.09, 1.10, 0.84), lichen * 0.25);
        let pale = smoothstep(0.60, 0.82, base.r);
        let moss = smoothstep(0.52, 0.76, patches) * moist * top * (1.0 - pale);
        color = mix(color, vec3<f32>(0.26, 0.365, 0.20), moss * 0.30);
    }
    return mix(base, color, detail);
}

// Small world-anchored pigment blocks suggest authored pixel materials without
// textures. Derivative fading removes subpixel detail rather than making it swim.
fn surface_pigment(base: vec3<f32>, world: vec3<f32>, normal: vec3<f32>, material: f32, footprint: f32, distance: f32) -> vec3<f32> {
    var substrate = base;
    if material < 0.5 || (material > 6.5 && material < 7.5) {
        substrate = terrain_pigment(base, world, normal, footprint);
    }
    substrate = surface_communities(substrate, world, normal, material, footprint, distance);
    let detail = (1.0 - smoothstep(0.12, 0.65, footprint)) * (1.0 - smoothstep(90.0, 230.0, distance));
    if detail <= 0.001 { return substrate; }
    let p = floor(world * 8.0) / 8.0;
    var color = substrate;
    if material > 2.5 && material < 3.5 {
        // Long broken fibers follow upright bark; short knots stop a striped look.
        let face = surface_plane(p, normal);
        let fibers = noise(vec2<f32>(face.x * 11.0, face.y * 0.48));
        let breaks = noise(face * vec2<f32>(2.2, 1.5));
        let fissure = smoothstep(0.54, 0.76, fibers) * (0.35 + breaks * 0.65);
        let fleck = hash21(floor(face * vec2<f32>(5.0, 8.0))) - 0.5;
        color *= 1.06 - fissure * 0.34 + fleck * 0.12;
        let weathering = smoothstep(0.60, 0.82, noise(face * 0.75)) * max(normal.y, 0.0);
        color = mix(color, vec3<f32>(0.48, 0.46, 0.36), weathering * 0.18);
    } else if material > 1.5 && material < 2.5 {
        let face = surface_plane(p, normal);
        let mineral = hash21(floor(p.xz * 7.0) + vec2<f32>(floor(p.y * 8.0), 0.0));
        let warm_stone = clamp((base.r - base.b) * 5.0, 0.0, 1.0);
        let stratum = sin(p.y * 9.0 + noise(p.xz * 0.5) * 2.0);
        color *= 0.95 + mineral * 0.10 + stratum * warm_stone * 0.045;
        let lichen = smoothstep(0.58, 0.77, noise(face * 2.4)) * (0.25 + max(normal.y, 0.0) * 0.75);
        color = mix(color, vec3<f32>(0.58, 0.59, 0.36), lichen * 0.14);
        let moss = smoothstep(0.57, 0.77, noise(p.xz * 0.65 + vec2<f32>(9.1, 2.4))) * smoothstep(0.20, 0.80, normal.y);
        // Darker silicate rock takes a little moss; pale chalk stays chalk.
        color = mix(color, vec3<f32>(0.24, 0.34, 0.16), moss * (1.0 - smoothstep(0.55, 0.76, base.r)) * 0.20);
    } else if material < 0.5 || (material > 6.5 && material < 7.5) {
        let grain = hash21(floor(p.xz * 6.0));
        let grass = smoothstep(0.015, 0.12, base.g - base.r);
        let shade_floor = 1.0 - smoothstep(0.31, 0.45, base.g);
        let soil = noise(p.xz * 1.7);
        color *= 0.9825 + grain * 0.035;
        // Sandy gravel flecks and low-contrast needles/leaves share a restrained
        // scale; the biome palette, not noisy triangles, carries the broad forms.
        let gravel = step(0.90, grain) * (1.0 - grass);
        color = mix(color, base * vec3<f32>(0.80, 0.83, 0.77), gravel * 0.35);
        let litter = smoothstep(0.63, 0.79, soil) * grass * (0.10 + shade_floor * 0.36);
        color = mix(color, vec3<f32>(0.28, 0.235, 0.13), litter);
    }
    return mix(substrate, color, detail);
}

fn surface_lighting(base: vec3<f32>, normal: vec3<f32>, material: f32, visibility: f32, distance: f32, sky_access: f32) -> vec3<f32> {
    let sun = normalize(u.light.xyz);
    let diffuse = max(dot(normal, sun), 0.0);
    let up = clamp(normal.y * 0.5 + 0.5, 0.0, 1.0);
    let regional = 1.0 - smoothstep(1800.0, 6500.0, distance);
    let woodland = clamp(u.climate.x, 0.0, 1.0) * regional;
    let sandstone = clamp(u.climate.z, 0.0, 1.0) * regional;
    let sunlight = u.direct.rgb * mix(vec3<f32>(1.0), vec3<f32>(1.04, 0.97, 0.87), sandstone * daylight() * 0.48);
    var ambient = mix(vec3<f32>(0.42, 0.43, 0.35), vec3<f32>(0.60, 0.65, 0.68), up);
    var direct = floor(diffuse * 5.0 + 0.5) / 5.0 * 0.43;
    if material < 0.5 || (material > 6.5 && material < 7.5) {
        // Continuous terrain normals remain readable without hard light bands;
        // rock and architecture retain the faceted quantized response above.
        ambient = mix(vec3<f32>(0.49, 0.50, 0.42), vec3<f32>(0.66, 0.68, 0.60), up);
        direct = diffuse * 0.38;
    } else if (material > 0.5 && material < 1.5) || (material > 8.5 && material < 9.5) {
        // Distant canopy keeps the same light response as its nearby trees.
        ambient = mix(vec3<f32>(0.30, 0.38, 0.27), vec3<f32>(0.56, 0.62, 0.47), up);
        direct = diffuse * 0.44 + max(dot(-normal, sun), 0.0) * 0.12;
    } else if material > 5.5 && material < 6.5 {
        ambient = mix(vec3<f32>(0.52, 0.58, 0.46), vec3<f32>(0.65, 0.70, 0.56), up);
        direct = abs(dot(normal, sun)) * 0.34;
    }
    // Shade cools without becoming black. Only the sun term is shadowed;
    // retained sky illumination keeps forest paths and plant silhouettes clear.
    let shade = 1.0 - diffuse * visibility;
    ambient *= mix(vec3<f32>(1.0), vec3<f32>(0.79, 0.93, 1.16), woodland * (0.42 + shade * 0.58));
    direct *= smoothstep(-0.03, 0.06, sun.y);
    // Moonlight is cool and directional, with a low independent sky floor.
    // Do not multiply the surface by water exposure again: that would erase night detail.
    // Low-light vision loses pigment saturation before it loses silhouette.
    // Raising blue illumination alone keeps yellow-green grass murky because
    // its albedo contains very little blue. This gentle night-only adaptation
    // lets the cool hemisphere light describe the terrain's actual forms.
    let pigment_luma = dot(base, vec3<f32>(0.2126, 0.7152, 0.0722));
    let nocturne = (1.0 - daylight()) * 0.72;
    let reflectance = mix(base, vec3<f32>(pigment_luma), nocturne);
    ambient *= (0.58 + sky_access * 0.32) * (1.0 - u.weather.x * 0.16);
    direct *= 1.30;
    let illumination = ambient * u.ambient.rgb + sunlight * direct * visibility * u.direct.w;
    let albedo = pow(max(reflectance,vec3<f32>(0.0)),vec3<f32>(2.2));
    // Illuminate linear albedo. Gamma-converting the product would darken
    // forest shelter and moonlight twice, obscuring otherwise walkable ground.
    return albedo * illumination * mix(0.72,0.85,daylight())
        + albedo * vec3<f32>(0.48,0.64,0.95) * u.storm.w * sky_access;

}

// Mean density of a height-fog layer along the actual sightline. The integral
// has a finite limit for horizontal rays and clamps below its valley reference.
// Looking down from a ridge therefore reveals mist low in the valley, rather
// than whitening the ridge itself or placing a billboard in front of the eye.
fn height_column(eye_y: f32, target_y: f32, floor_y: f32, scale: f32) -> f32 {
    let a = (min(eye_y, target_y) - floor_y) / scale;
    let b = (max(eye_y, target_y) - floor_y) / scale;
    if b <= 0.0 { return 1.0; }
    if b - a < 0.01 { return exp(-max((a + b) * 0.5, 0.0)); }
    if a >= 0.0 { return (exp(-a) - exp(-b)) / (b - a); }
    return (-a + 1.0 - exp(-b)) / (b - a);
}

fn atmospheric_color(color: vec3<f32>, world: vec3<f32>, distance: f32) -> vec3<f32> {
    let direction = normalize(world - u.camera.xyz);
    let fog_distance = max(u.fog.w, 100.0);
    let moisture = clamp(u.air.z, 0.0, 1.0);
    let mountain = clamp(u.climate.w, 0.0, 1.0);
    var optical_depth = pow(distance / fog_distance, 1.34) * (0.92 + moisture * 0.12);
    // A much taller, dilute layer separates successive mountains while their
    // upper faces retain their pigment. No height bands or hard fog planes.
    let valley_air = height_column(u.camera.y, world.y, 120.0, 620.0);
    optical_depth += valley_air * min(distance / 11000.0, 0.8) * (0.10 + mountain * 0.24);
    // Precipitation occupies a volume, not a camera-facing grey overlay. Clear
    // weather retains the existing long visibility; blizzard snowfall closes it.
    optical_depth += distance * (u.weather.w * 0.0015 + u.weather.y * 0.00018 + u.weather.z * 0.0014);
    var result = mix(color, pow(max(horizon_color(direction),vec3<f32>(0.0)),vec3<f32>(2.2)), clamp(1.0 - exp(-optical_depth), 0.0, 0.995));

    let wetland = clamp(u.climate.y, 0.0, 1.0);
    if wetland > 0.001 {
        let column = height_column(u.camera.y, world.y, u.air.x + 1.5, 8.5);
        let mid = mix(u.camera.xz, world.xz, 0.40);
        let ribbons = noise(mid * 0.0031 + vec2<f32>(u.params.x * 0.00012, -u.params.x * 0.00006));
        let morning = 0.48 + (1.0 - smoothstep(0.12, 0.82, solar_elevation())) * 0.52;
        let mist_depth = min(distance, 3200.0) * column * wetland * morning
                       * (0.00015 + ribbons * 0.00009);
        let mist = min(1.0 - exp(-mist_depth), 0.34);
        let mist_day = mix(vec3<f32>(0.44, 0.53, 0.55), vec3<f32>(0.52, 0.565, 0.53), twilight() * 0.30);
        let mist_color = mix(vec3<f32>(0.045, 0.075, 0.12), mist_day, daylight());
        result = mix(result, pow(max(mist_color,vec3<f32>(0.0)),vec3<f32>(2.2)), mist);
    }
    return result;
}

fn shade_surface(v: VertexOut) -> vec4<f32> {
    if u.reflection_params.z > 0.5 && (v.world.y < u.reflection_params.x - 0.12
        || (v.material > 3.5 && v.material < 4.5) || (v.material > 7.5 && v.material < 8.5)) { discard; }
    let distance = length(v.world - u.camera.xyz);
    let water_footprint = max(length(dpdx(v.world.xz)), length(dpdy(v.world.xz)));
    let material_footprint = max(length(dpdx(v.world)), length(dpdy(v.world)));
    if v.material > 6.5 && v.material < 9.5 {
        // Near terrain replaces far patches exactly on the streamed chunk mask.
        let tile_delta = floor(v.world.xz / 192.0) - floor(u.camera.xz / 192.0);
        if length(tile_delta) <= u.settings.x + 0.5 {
            discard;
        }
    }
    if v.material > 8.5 && v.material < 9.5 {
        let ground_distance = length(v.world.xz - u.camera.xz);
        let start = max(u.distant.x, 0.0);
        let end = max(u.distant.y, start + 1.0);
        let fade = max(u.distant.z, 1.0);
        let keep = smoothstep(start, start + fade, ground_distance)
                 * (1.0 - smoothstep(max(start, end - fade), end, ground_distance));
        // A small screen-door fade remains band-limited at kilometre ranges;
        // world-space high-frequency hashes would shimmer on tiny crowns.
        if u.distant.y <= u.distant.x || keep <= bayer(v.clip.xy) + 0.5 {
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
    var color: vec3<f32>;
    if (v.material > 3.5 && v.material < 4.5) || (v.material > 7.5 && v.material < 8.5) {
        color = water_color(v.world, distance, v.color, water_footprint, v.normal);
    } else {
        var pigment = surface_pigment(v.color, v.world, normal, v.material, material_footprint, distance);
        let physical_sky = open_sky(v.world);
        let sky_access = select(1.0, physical_sky, u.shelter_params.z > 0.5);
        let deposition = smoothstep(0.15,0.72,normal.y) * physical_sky;
        let snow_pattern = noise(v.world.xz * 0.20) * 0.24 + 0.76;
        let snow = smoothstep(0.02,0.80,u.surface.y) * deposition * snow_pattern;
        pigment *= 1.0 - u.surface.x * 0.20 * physical_sky * (1.0-snow);
        pigment = mix(pigment, vec3<f32>(0.84,0.89,0.91),snow);
        let visibility = sun_visibility(v.world, normal) * weather_light_visibility(v.world);
        color = surface_lighting(pigment, normal, v.material, visibility, distance, sky_access);
        // Small wet-rock and soil glints follow actual surface orientation. No
        // global glossy overlay on grass or sheltered undersides.
        if v.material < 0.5 || (v.material > 1.5 && v.material < 3.5) {
            let view = normalize(u.camera.xyz - v.world);
            let halfway = normalize(view + normalize(u.light.xyz));
            let sheen = pow(max(dot(normal,halfway),0.0),mix(80.0,28.0,u.surface.x));
            color += u.direct.rgb * sheen * u.surface.x * deposition * visibility * u.direct.w * (1.0-snow) * 0.35;
        }
    }

    color = atmospheric_color(max(color,vec3<f32>(0.0)), v.world, distance);
    // Preserve HDR through fog and Bloom. Palette quantization happens once at
    // presentation, so bright sun, spray and lightning retain their energy.
    return vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
}

@fragment fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    return shade_surface(v);
}
@fragment fn fs_land(v: VertexOut) -> @location(0) vec4<f32> {
    if (v.material > 3.5 && v.material < 4.5) || (v.material > 7.5 && v.material < 8.5) { discard; }
    return shade_surface(v);
}
@fragment fn fs_water(v: VertexOut) -> @location(0) vec4<f32> {
    if !((v.material > 3.5 && v.material < 4.5) || (v.material > 7.5 && v.material < 8.5)) { discard; }
    return shade_surface(v);
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

// The same world-space cloud field is used by the visible sky, water reflections
// and projected terrain shadows. It does not follow the eye or restart at a
// streamed chunk boundary. CPU weather supplies the traveling front's local
// coverage; this field supplies the continuous billows within it.
fn weather_cloud_base(high: bool) -> f32 {
    if high { return 6200.0; }
    return 3300.0 - u.weather.y * 1250.0 - u.weather.z * 700.0;
}

fn weather_cloud_field(world_xz: vec2<f32>, high: bool) -> vec3<f32> {
    let seconds = u.surface.w;
    let scale = select(0.00042, 0.00020, high);
    // A fixed advection velocity prevents variable wind*time from teleporting
    // the whole cloud sheet when a gust or a manual transition changes wind.
    let drift = select(vec2<f32>(14.0, -6.0), vec2<f32>(22.0, -9.0), high) * seconds;
    let p = (world_xz - drift) * scale + select(vec2<f32>(0.0), vec2<f32>(21.0, -8.0), high);
    let regional = noise(p * 0.22 + vec2<f32>(13.4, 7.7));
    let warp = vec2<f32>(noise(p * 0.58), noise(p * 0.58 + vec2<f32>(9.1, 2.4))) * 0.68;
    let field = fbm(p + warp) + (regional - 0.5) * 0.18;
    let threshold = mix(0.86, 0.18, clamp(u.weather.x, 0.0, 1.0)) + select(0.0, 0.035, high);
    var density = smoothstep(threshold, threshold + 0.20, field);
    // A true overcast deck closes the holes instead of showing blue sky through
    // the middle of a tempest. Ordinary cloudy weather still has broken edges.
    let deck = smoothstep(0.87, 1.0, u.weather.x) * select(0.985, 0.80, high);
    density = mix(density, max(density, deck), deck);
    let thickness = smoothstep(threshold + 0.025, threshold + 0.30, field);
    let light_offset = normalize(u.light.xz + vec2<f32>(0.001)) * 0.25;
    let neighbor = noise((p + warp + light_offset) * 1.05);
    let facing = clamp((field - neighbor) * 2.8 + 0.5, 0.0, 1.0);
    return vec3<f32>(density, thickness, facing);
}

fn weather_cloud_ray(ray: vec3<f32>, high: bool) -> vec3<f32> {
    let height = weather_cloud_base(high) - u.camera.y;
    if ray.y <= 0.008 || height <= 0.0 { return vec3<f32>(0.0); }
    let intersection = u.camera.xz + ray.xz * (height / ray.y);
    let cloud = weather_cloud_field(intersection, high);
    return vec3<f32>(cloud.x * smoothstep(0.008, 0.075, ray.y), cloud.yz);
}

// Palette-space composition, shared by sky_radiance and optional cheap water
// fallbacks. sky_radiance performs the final conversion to linear HDR once.
fn shared_sky_weather(ray: vec3<f32>, base_color: vec3<f32>) -> vec3<f32> {
    if ray.y <= 0.0 { return base_color; }
    let day = daylight();
    let severity = clamp(u.weather.y * 0.60 + u.weather.w * 0.40, 0.0, 1.0);
    let overcast = smoothstep(0.64, 1.0, u.weather.x);
    let high = weather_cloud_ray(ray, true);
    let high_color = mix(vec3<f32>(0.045, 0.070, 0.12), vec3<f32>(0.73, 0.76, 0.78), day);
    var color = mix(base_color, high_color, high.x * mix(0.27, 0.45, overcast));
    let low = weather_cloud_ray(ray, false);
    let alignment = max(dot(ray, normalize(u.light.xyz)), 0.0);
    let edge_light = pow(alignment, 12.0);
    let lit_day = mix(vec3<f32>(0.90, 0.58, 0.34), vec3<f32>(0.86, 0.87, 0.82), smoothstep(0.04, 0.55, solar_elevation()));
    let cloud_lit = mix(vec3<f32>(0.085, 0.13, 0.215), lit_day, day);
    let shade_day = mix(vec3<f32>(0.34, 0.43, 0.51), vec3<f32>(0.17, 0.215, 0.27), severity);
    let cloud_shade = mix(vec3<f32>(0.018, 0.031, 0.057), shade_day, day);
    let shading = clamp(low.y * 0.84 + (1.0 - low.z) * 0.16 + severity * 0.13, 0.0, 1.0);
    var cloud_color = mix(cloud_lit, cloud_shade, shading);
    let silver = edge_light * (1.0 - smoothstep(0.15, 0.80, low.y)) * u.direct.w;
    cloud_color += u.direct.rgb * silver * mix(0.30, 0.18, day) * (1.0 - overcast * 0.75);
    cloud_color = mix(cloud_color, vec3<f32>(0.86, 0.40, 0.20), twilight() * edge_light * (1.0 - low.y) * (1.0 - overcast) * 0.55);
    // Snow scatters soft neutral light. Storm flashes illuminate the underside
    // coherently with root's surface light, not the sun/moon texture itself.
    cloud_color = mix(cloud_color, mix(vec3<f32>(0.12, 0.15, 0.21), vec3<f32>(0.58, 0.64, 0.68), day), u.weather.z * 0.34);
    cloud_color += vec3<f32>(0.55, 0.65, 0.84) * u.storm.w * (0.35 + low.y * 0.65);
    color = mix(color, cloud_color, low.x * mix(0.94, 0.998, overcast));
    return color;
}

fn weather_sky_transmission(ray: vec3<f32>) -> f32 {
    let overcast = smoothstep(0.64, 1.0, u.weather.x);
    let high = weather_cloud_ray(ray, true);
    let low = weather_cloud_ray(ray, false);
    return (1.0 - high.x * mix(0.27, 0.45, overcast))
        * (1.0 - low.x * mix(0.94, 0.998, overcast));
}

fn star_plane(direction: vec3<f32>) -> vec2<f32> {
    let a = abs(direction);
    if a.x > a.y && a.x > a.z {
        return direction.yz / a.x + vec2<f32>(select(-7.0, 7.0, direction.x > 0.0), 0.0);
    }
    if a.z > a.y {
        return direction.xy / a.z + vec2<f32>(select(-13.0, 13.0, direction.z > 0.0), 5.0);
    }
    return direction.xz / a.y + vec2<f32>(0.0, select(-9.0, 9.0, direction.y > 0.0));
}

fn star_layer(plane: vec2<f32>, scale: f32, cutoff: f32, aa: f32) -> vec3<f32> {
    let q = plane * scale;
    let cell = floor(q);
    let key = hash21(cell + vec2<f32>(11.13, -7.71));
    let center = vec2<f32>(hash21(cell + vec2<f32>(31.1, 2.8)), hash21(cell + vec2<f32>(-4.6, 19.7))) * 0.66 + 0.17;
    let distance = length(fract(q) - center);
    let radius = 0.035 + pow(hash21(cell + vec2<f32>(6.1, 28.5)), 9.0) * 0.11;
    let pixel = clamp(aa * scale, 0.012, 0.4);
    let intensity = (1.0 - smoothstep(max(radius - pixel, 0.0), radius + pixel, distance)) * step(cutoff, key);
    let temperature = hash21(cell + vec2<f32>(1.7, 43.1));
    let tint = mix(vec3<f32>(1.0, 0.70, 0.44), vec3<f32>(0.57, 0.77, 1.0), temperature);
    return tint * intensity * (0.65 + hash21(cell + vec2<f32>(-7.9, 1.1)) * 0.35);
}

fn night_heavens(ray: vec3<f32>, aa: f32) -> vec3<f32> {
    if u.ambient.w <= 0.001 { return vec3<f32>(0.0); }
    // Rotate the heavens with the clock; the field is fixed to the sky and never
    // follows camera yaw. The inclined pale band is lore's Ashen River.
    let angle = (u.params.w - 22.0) * 0.261799388;
    let s = sin(angle);
    let c = cos(angle);
    let direction = vec3<f32>(ray.x * c - ray.z * s, ray.y, ray.x * s + ray.z * c);
    let plane = star_plane(direction);
    let stars = star_layer(plane, 110.0, 0.995, aa) + star_layer(plane + vec2<f32>(21.7, 4.1), 265.0, 0.996, aa) * 0.72;
    let band_axis = normalize(vec3<f32>(0.42, 0.61, -0.67));
    let band_distance = abs(dot(direction, band_axis));
    let band = exp(-band_distance * band_distance * 145.0);
    let filaments = fbm(direction.xz * 6.0 + direction.y * vec2<f32>(-3.0, 7.0));
    let rifts = noise(direction.xz * 19.0 + direction.y * vec2<f32>(7.0, -13.0));
    let galaxy = vec3<f32>(0.065, 0.078, 0.14) * band * (0.23 + filaments * 0.95) * (0.55 + rifts * 0.45);
    // Seven clustered navigational stars form the Keeper's Crown; no drawn
    // connecting lines. Each is a true angular disc, not a screen-space icon.
    let crown = array<vec3<f32>, 7>(
        vec3<f32>(-0.55, 0.70, -0.45), vec3<f32>(-0.42, 0.80, -0.43),
        vec3<f32>(-0.27, 0.78, -0.55), vec3<f32>(-0.14, 0.84, -0.52),
        vec3<f32>(0.00, 0.77, -0.64), vec3<f32>(-0.31, 0.69, -0.65),
        vec3<f32>(-0.19, 0.67, -0.72));
    var clustered = 0.0;
    for (var i = 0u; i < 7u; i += 1u) {
        let distance = length(direction - normalize(crown[i]));
        clustered += 1.0 - smoothstep(0.0012, 0.0018 + aa, distance);
    }
    return (stars + galaxy + vec3<f32>(0.80, 0.88, 1.0) * clustered)
        * smoothstep(0.015, 0.25, ray.y) * u.ambient.w;
}

fn moon_disc(ray: vec3<f32>, body: vec4<f32>, copper: f32, aa: f32) -> vec4<f32> {
    let direction = body.xyz;
    let radius = body.w;
    let alignment = dot(ray, direction);
    // A conservative cap skips surface work for almost all sky fragments.
    if alignment < cos(radius + aa * 2.0) || ray.y < -0.02 {
        return vec4<f32>(0.0);
    }
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(direction.y) > 0.98);
    let right = normalize(cross(reference, direction));
    let up = cross(direction, right);
    let local = vec2<f32>(dot(ray, right), dot(ray, up)) / sin(radius);
    let r2 = dot(local, local);
    let softness = max(aa / radius, 0.008);
    let alpha = (1.0 - smoothstep(1.0 - softness, 1.0 + softness, sqrt(r2)))
        * smoothstep(-0.015, 0.025, ray.y);
    let relief = sqrt(max(1.0 - min(r2, 1.0), 0.0));
    let surface_normal = normalize(right * local.x + up * local.y - direction * relief);
    let incidence = dot(surface_normal, u.solenne.xyz);
    let sunlit = smoothstep(-0.065, 0.075, incidence);
    // Procedural maria and crater rings remain broad enough for a 20-40px moon.
    let maria = fbm(local * 3.8 + vec2<f32>(11.1, 7.9) + copper * 17.0);
    let q = local * 9.0;
    let cell = floor(q);
    let crater_center = vec2<f32>(hash21(cell + 13.0), hash21(cell + 37.0)) * 0.60 + 0.20;
    let crater_distance = length(fract(q) - crater_center);
    let crater_radius = 0.13 + hash21(cell + 5.1) * 0.19;
    let bowl = 1.0 - smoothstep(crater_radius * 0.55, crater_radius, crater_distance);
    let ring = (1.0 - smoothstep(0.035, 0.10, abs(crater_distance - crater_radius)));
    let rock = clamp(0.83 + (maria - 0.5) * 0.52 - bowl * 0.12 + ring * 0.08, 0.5, 1.0);
    let albedo = mix(vec3<f32>(0.87, 0.89, 0.80), vec3<f32>(0.88, 0.47, 0.255), copper);
    let illuminated = albedo * rock * (0.34 + max(incidence, 0.0) * 0.65);
    // Warm reflected light from the world's atmosphere reveals Vey's copper
    // unlit hemisphere while preserving the actual sun-driven terminator.
    let planetshine = mix(vec3<f32>(0.075, 0.095, 0.14), vec3<f32>(0.22, 0.16, 0.12), copper);
    let earthshine = albedo * rock * planetshine;
    var color = mix(earthshine, illuminated, sunlit);
    // Daylight scatters over the disc, but its entire silhouette still occludes
    // the stars before clouds are composited on top.
    color = mix(color, color * 0.55 + sky_gradient(ray) * 0.72, daylight());
    return vec4<f32>(color, alpha);
}

// Derivative-free: water may call this from its material branch. Internal
// dimensions live in the existing spare settings.z/w, avoiding another uniform.
// Sparse world-anchored forks. The 11-second event id is exactly the window used
// by weather.rs::lightning. storm.w carries both pulse envelopes; geometry never
// re-randomizes between the leading flash and its afterstroke.
fn lightning_segment(ray: vec3<f32>, world_a: vec3<f32>, world_b: vec3<f32>, aa: f32, thickness: f32) -> vec2<f32> {
    let a = normalize(world_a - u.camera.xyz);
    let b = normalize(world_b - u.camera.xyz);
    let edge = b - a;
    let along = clamp(dot(ray - a, edge) / max(dot(edge, edge), 0.0000001), 0.0, 1.0);
    let distance = length(ray - normalize(mix(a, b, along)));
    let width = thickness / max(length(mix(world_a, world_b, along) - u.camera.xyz), 1000.0);
    let core = 1.0 - smoothstep(width, width + aa * 1.1, distance);
    let halo_width = max(width * 10.0, aa * 2.3);
    let halo = exp(-distance * distance / (halo_width * halo_width));
    return vec2<f32>(core, halo);
}

fn lightning_radiance(ray: vec3<f32>, aa: f32) -> vec3<f32> {
    if u.storm.w <= 0.0001 { return vec3<f32>(0.0); }
    let event = floor(u.surface.w / 11.0);
    let cell_size = 14000.0;
    let local_cell = floor(u.camera.xz / cell_size);
    let bolt_top = weather_cloud_base(false) - 60.0;
    var emission = vec2<f32>(0.0);
    // Nine bounded candidate cells let the same absolute bolt remain visible
    // across a storm-cell boundary. Distances and a tight angular cap skip all
    // segment work for almost every pixel, even during the brief flash.
    for (var dz = -1; dz <= 1; dz += 1) {
        for (var dx = -1; dx <= 1; dx += 1) {
            let cell = local_cell + vec2<f32>(f32(dx), f32(dz));
            let key = cell + vec2<f32>(event * 3.17, event * -5.71);
            if hash21(key + vec2<f32>(8.7, 31.9)) < 0.35 { continue; }
            let offset = vec2<f32>(hash21(key + vec2<f32>(13.1, 5.8)), hash21(key + vec2<f32>(-3.1, 19.4)));
            let anchor = (cell + vec2<f32>(0.5)) * cell_size + (offset - vec2<f32>(0.5)) * 10000.0;
            let distance = length(anchor - u.camera.xz);
            let visibility = smoothstep(2800.0, 4400.0, distance) * (1.0 - smoothstep(17500.0, 20000.0, distance));
            if visibility <= 0.0 { continue; }
            let center = normalize(vec3<f32>(anchor.x, bolt_top * 0.5, anchor.y) - u.camera.xyz);
            let cap_radius = 1800.0 / max(distance, 2800.0);
            if length(ray - center) > cap_radius + aa * 5.0 { continue; }
            let side = normalize(vec2<f32>(offset.y - 0.5, 0.35 + offset.x));
            var previous = vec3<f32>(anchor.x, bolt_top, anchor.y);
            var branch_a = previous;
            var branch_b = previous;
            for (var i = 1u; i <= 6u; i += 1u) {
                let along = f32(i) / 6.0;
                let jitter = vec2<f32>(hash21(key + vec2<f32>(f32(i) * 7.9, 9.1)), hash21(key + vec2<f32>(13.7, f32(i) * 11.3))) - vec2<f32>(0.5);
                let p = anchor + jitter * (230.0 * sin(along * 3.14159265));
                let point = vec3<f32>(p.x, bolt_top * (1.0 - along), p.y);
                emission = max(emission, lightning_segment(ray, previous, point, aa, 2.2) * visibility);
                if i == 2u { branch_a = point; }
                if i == 3u { branch_b = point; }
                previous = point;
            }
            let fork_a = branch_a + vec3<f32>(side.x * 340.0, -430.0, side.y * 340.0);
            let tip_a = fork_a + vec3<f32>(side.x * 210.0, -580.0, side.y * 210.0);
            let fork_b = branch_b - vec3<f32>(side.x * 310.0, 260.0, side.y * 310.0);
            let tip_b = fork_b - vec3<f32>(side.x * 230.0, 430.0, side.y * 230.0);
            emission = max(emission, lightning_segment(ray, branch_a, fork_a, aa, 1.3) * visibility * 0.70);
            emission = max(emission, lightning_segment(ray, fork_a, tip_a, aa, 0.8) * visibility * 0.52);
            emission = max(emission, lightning_segment(ray, branch_b, fork_b, aa, 1.1) * visibility * 0.65);
            emission = max(emission, lightning_segment(ray, fork_b, tip_b, aa, 0.7) * visibility * 0.42);
        }
    }
    // The sky pass is behind terrain depth: every branch disappears into the
    // actual visible ridge/shore silhouette instead of ending above the ground.
    return (vec3<f32>(0.78, 0.87, 1.0) * emission.x * 15.0
        + vec3<f32>(0.25, 0.43, 0.82) * emission.y * 1.1) * u.storm.w;
}

fn sky_radiance(ray: vec3<f32>) -> vec3<f32> {
    let render_height = select(720.0, u.settings.w, u.settings.w >= 120.0);
    let aa = 1.453085 / render_height;
    var color = sky_gradient(ray);
    color += night_heavens(ray, aa);

    let sun_distance = length(ray - u.solenne.xyz);
    let sun_disk = (1.0 - smoothstep(max(0.0105 - aa, 0.0), 0.0105 + aa, sun_distance))
        * smoothstep(-0.020, 0.010, ray.y);
    let sun_color = mix(vec3<f32>(1.0, 0.48, 0.14), vec3<f32>(1.0, 0.96, 0.78), smoothstep(0.02, 0.50, solar_elevation()));
    color = mix(color, sun_color, sun_disk);
    let aster = moon_disc(ray, u.aster, 0.0, aa);
    color = mix(color, aster.rgb, aster.a);
    let vey = moon_disc(ray, u.vey, 1.0, aa);
    color = mix(color, vey.rgb, vey.a);
    color = shared_sky_weather(ray, color);
    var radiance = pow(max(color, vec3<f32>(0.0)), vec3<f32>(2.2));
    // Preserve true bright sunlight until the HDR tone map/Bloom stage. Stars
    // remain quiet. The extra radiance is occluded by both moons and the same
    // clouds used for the ordinary sky, rather than leaking through an overcast.
    if sun_disk > 0.0 {
        let transmission = weather_sky_transmission(ray) * (1.0 - aster.a) * (1.0 - vey.a);
        radiance += pow(sun_color, vec3<f32>(2.2)) * sun_disk * transmission * 7.0;
    }
    return radiance + lightning_radiance(ray, aa);
}

@fragment fn fs_sky(v: SkyOut) -> @location(0) vec4<f32> {
    return vec4<f32>(sky_radiance(sky_ray(v.uv)), 1.0);
}
