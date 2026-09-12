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
    return mix(haze, twilight_haze, twilight() * (0.38 + sunward * 0.40));
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
    let direction = vec2<f32>(0.911, -0.412);
    let across = vec2<f32>(0.412, 0.911);
    let wave = time * 0.70 - dot(world.xz, direction) * 0.026;
    let cross_wave = time * 0.39 + dot(world.xz, across) * 0.019;
    let gust = 0.53 + sin(wave) * 0.27 + sin(cross_wave) * 0.16;
    let flutter = sin(time * 1.9 + dot(world.xz, vec2<f32>(0.31, 0.23))) * 0.08;
    return (direction * (gust + flutter) + across * sin(cross_wave) * 0.12)
         * clamp(strength, 0.0, 1.0);
}

fn transform_vertex(v: VertexIn) -> VertexOut {
    var o: VertexOut;
    var p = v.position;
    if v.material > 0.5 && v.material < 1.5 {
        let weight = clamp((1.4 - v.material) / 0.4, 0.0, 1.0);
        let bend = vegetation_wind(p, u.params.x, u.air.y) * (0.14 * weight);
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
    // Positive payload is ocean (depth+1), negative is freshwater -(depth+1).
    // A zero payload from old distant freshwater meshes retains a deep tone.
    let ocean = step(0.5, channel.z);
    let has_depth = step(0.5, abs(channel.z));
    let depth = mix(4.0, max(abs(channel.z) - 1.0, 0.0), has_depth);
    let flow = normalize(channel.xy + vec2<f32>(0.00001));
    let velocity = mix(flow * 0.85, vec2<f32>(0.58, -0.26), ocean);
    let moving = p - velocity * t;
    let near_detail = 1.0 - smoothstep(90.0, 480.0, distance);
    let wave_detail = 1.0 - smoothstep(1.2, 8.0, footprint);
    let fine_filter = 1.0 - smoothstep(0.35, 1.7, footprint);
    let fine_detail = near_detail * fine_filter;
    let warp = noise(moving * 0.032);
    let phase_a = dot(moving, vec2<f32>(0.28, 0.13)) + warp * 2.7;
    let phase_b = dot(moving, vec2<f32>(-0.15, 0.37));
    let phase_c = dot(moving, vec2<f32>(1.31, 0.58)) + warp * 3.8;
    let a = sin(phase_a) * wave_detail;
    let b = sin(phase_b) * wave_detail;
    let c = sin(phase_c) * fine_detail;
    let wind = 0.72 + clamp(u.air.y, 0.0, 1.0) * 0.28;
    let slope_x = ((cos(phase_a) * 0.027 - cos(phase_b) * 0.015) * wave_detail
                 + cos(phase_c) * 0.018 * fine_detail) * wind;
    let slope_z = ((cos(phase_a) * 0.014 + cos(phase_b) * 0.036) * wave_detail
                 + cos(phase_c) * 0.008 * fine_detail) * wind;
    let normal = normalize(vec3<f32>(-slope_x, 1.0, -slope_z));
    let view = normalize(u.camera.xyz - world);
    let reflection = reflect(-view, normal);
    let fresnel = 0.06 + 0.60 * pow(1.0 - max(dot(normal, view), 0.0), 4.0);

    let broad_current = noise(moving * 0.011);
    let sandstone = clamp(u.climate.z, 0.0, 1.0);
    let sediment = mix(vec3<f32>(0.35, 0.39, 0.255), vec3<f32>(0.48, 0.355, 0.195), sandstone * 0.65);
    let river_deep = mix(vec3<f32>(0.055, 0.235, 0.255), vec3<f32>(0.105, 0.29, 0.26), clamp(u.air.z, 0.0, 1.0) * 0.35);
    let river = mix(sediment, river_deep, 1.0 - exp(-depth * 0.62));
    let sea_shallow = mix(vec3<f32>(0.30, 0.46, 0.37), vec3<f32>(0.10, 0.385, 0.40), smoothstep(0.0, 5.0, depth));
    let sea = mix(sea_shallow, vec3<f32>(0.025, 0.155, 0.285), smoothstep(3.0, 42.0, depth));
    var color = mix(river, sea, ocean) * (0.965 + broad_current * 0.07);
    color *= u.light.w * (0.985 + (a * 0.62 + b * 0.38) * 0.035);

    var reflected_sky = sky_gradient(reflection);
    let reflected_cloud = smoothstep(0.56, 0.78, fbm(reflection.xz / max(reflection.y, 0.12) * 1.2 + vec2<f32>(t * 0.0009, 4.1)));
    reflected_sky = mix(reflected_sky, vec3<f32>(0.63, 0.69, 0.67) * u.light.w, reflected_cloud * 0.18 * daylight());
    color = mix(color, reflected_sky, fresnel);

    // A few muted strokes and refracted-looking shallow bands, never a white
    // opaque shoreline. Their scale is filtered before it becomes subpixel.
    let breaks = mix(0.5, smoothstep(0.35, 0.72, noise(moving * vec2<f32>(0.11, 0.28))), fine_filter);
    let crest = smoothstep(0.71, 0.96, a * 0.72 + c * 0.28) * breaks * near_detail;
    color += vec3<f32>(0.065, 0.080, 0.065) * crest * u.light.w;
    let shallow = exp(-depth * 0.85) * has_depth;
    let caustic = smoothstep(0.82, 0.97, sin(phase_c + a * 1.4));
    color += vec3<f32>(0.028, 0.035, 0.016) * caustic * shallow * fine_detail * daylight();
    let half_sum = normalize(u.light.xyz) + view;
    let half_vector = half_sum / sqrt(max(dot(half_sum, half_sum), 0.00001));
    let specular = pow(max(dot(normal, half_vector), 0.0), 120.0);
    let glint = smoothstep(0.48, 0.88, specular) * (0.45 + breaks * 0.55);
    // One reflected light path: the same sun/moon that illuminates the bank.
    color += u.direct.rgb * 0.35 * glint * u.direct.w * (0.20 + near_detail * 0.32);

    // The wet contact line is subdued. Only broken, moving ocean wash gets a
    // little reflected sky; rivers and lakes do not inherit surf foam.
    let contact = (1.0 - smoothstep(0.025, 0.22, depth)) * has_depth;
    color *= 1.0 - contact * 0.055;
    let surf_phase = depth * 2.1 - t * 0.72 + warp * 3.0;
    let surf = smoothstep(0.12, 0.32, depth) * (1.0 - smoothstep(0.55, 1.8, depth))
             * smoothstep(0.76, 0.97, sin(surf_phase)) * breaks * ocean * fine_detail;
    color = mix(color, reflected_sky, surf * 0.13);
    return color;
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

fn surface_lighting(base: vec3<f32>, normal: vec3<f32>, material: f32, visibility: f32, distance: f32) -> vec3<f32> {
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
    return reflectance * (ambient * u.ambient.rgb + sunlight * direct * visibility * u.direct.w);
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
    var result = mix(color, horizon_color(direction), clamp(1.0 - exp(-optical_depth), 0.0, 0.97));

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
        result = mix(result, mist_color, mist);
    }
    return result;
}

@fragment fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
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
        color = water_color(v.world, distance, v.color, water_footprint);
    } else {
        let pigment = surface_pigment(v.color, v.world, normal, v.material, material_footprint, distance);
        let visibility = sun_visibility(v.world, normal);
        color = surface_lighting(pigment, normal, v.material, visibility, distance);
    }

    // Retain the low-poly palette while restoring rich midtones in daylight.
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    color = mix(vec3<f32>(luma), color, 1.12);
    color = pow(max(color, vec3<f32>(0.0)), vec3<f32>(0.94));
    color = atmospheric_color(color, v.world, distance);
    let dither = bayer(v.clip.xy) * 0.35;
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

// Bounded procedural sky. Two cloud planes approximate sunlit volumes; there
// are no textures, ray marches or temporal accumulation requirements.
fn cloud_layer(ray: vec3<f32>, altitude: f32, scale: f32, drift: vec2<f32>, threshold: f32) -> vec3<f32> {
    let horizon_fade = smoothstep(0.018, 0.13, ray.y);
    let plane_distance = max(altitude - u.camera.y, 100.0) / max(ray.y, 0.015);
    let p = (u.camera.xz + ray.xz * plane_distance) * scale + drift;
    let weather = noise(p * 0.23 + vec2<f32>(13.4, 7.7));
    let warp = vec2<f32>(noise(p * 0.6), noise(p * 0.6 + vec2<f32>(9.1, 2.4))) * 0.6;
    let field = fbm(p + warp) + (weather - 0.5) * 0.17;
    let density = smoothstep(threshold, threshold + 0.19, field);
    let thickness = smoothstep(threshold + 0.025, threshold + 0.25, field);
    // A cheap directional derivative gives billows a consistent illuminated side.
    let light_offset = normalize(u.light.xz + vec2<f32>(0.001)) * 0.25;
    let neighbor = noise((p + warp + light_offset) * 1.05);
    let facing = clamp((field - neighbor) * 2.8 + 0.5, 0.0, 1.0);
    return vec3<f32>(density * horizon_fade, thickness, facing);
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
    let galaxy = vec3<f32>(0.036, 0.042, 0.077) * band * (0.23 + filaments * 0.95) * (0.55 + rifts * 0.45);
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

@fragment fn fs_sky(v: SkyOut) -> @location(0) vec4<f32> {
    let ray = sky_ray(v.uv);
    let aa = max(length(dpdx(ray)), length(dpdy(ray)));
    let day = daylight();
    var color = sky_gradient(ray);
    color += night_heavens(ray, aa);

    // Solenne and both moons are painted before weather. Nothing shines through
    // opaque lunar silhouettes or the substantial part of a cloud bank.
    let sun_distance = length(ray - u.solenne.xyz);
    let sun_disk = (1.0 - smoothstep(0.0105 - aa, 0.0105 + aa, sun_distance))
        * smoothstep(-0.020, 0.010, ray.y);
    let sun_color = mix(vec3<f32>(1.0, 0.48, 0.14), vec3<f32>(1.0, 0.96, 0.78), smoothstep(0.02, 0.50, solar_elevation()));
    color = mix(color, sun_color, sun_disk);
    let aster = moon_disc(ray, u.aster, 0.0, aa);
    color = mix(color, aster.rgb, aster.a);
    let vey = moon_disc(ray, u.vey, 1.0, aa);
    color = mix(color, vey.rgb, vey.a);

    if ray.y > 0.0 {
        let t = u.params.x;
        let high = cloud_layer(ray, 5200.0, 0.00029, vec2<f32>(t * 0.00055 + 21.0, -t * 0.00022), 0.535);
        let high_color = mix(vec3<f32>(0.055, 0.080, 0.13), vec3<f32>(0.72, 0.76, 0.77), day);
        color = mix(color, high_color, high.x * 0.29);

        let low = cloud_layer(ray, 2800.0, 0.00048, vec2<f32>(t * 0.00095, t * 0.00024), 0.515);
        let alignment = max(dot(ray, normalize(u.light.xyz)), 0.0);
        let edge_light = pow(alignment, 12.0);
        let lit_day = mix(vec3<f32>(0.90, 0.58, 0.34), vec3<f32>(0.86, 0.87, 0.80), smoothstep(0.04, 0.55, solar_elevation()));
        let cloud_lit = mix(vec3<f32>(0.10, 0.145, 0.235), lit_day, day);
        let cloud_shade = mix(vec3<f32>(0.025, 0.041, 0.075), vec3<f32>(0.34, 0.43, 0.51), day);
        let shading = clamp(low.y * 0.86 + (1.0 - low.z) * 0.15, 0.0, 1.0);
        var cloud_color = mix(cloud_lit, cloud_shade, shading);
        let silver = edge_light * (1.0 - smoothstep(0.15, 0.80, low.y)) * u.direct.w;
        cloud_color += u.direct.rgb * silver * mix(0.32, 0.19, day);
        // Low sunlight paints cloud rims amber; their undersides keep a cool hue.
        cloud_color = mix(cloud_color, vec3<f32>(0.86, 0.40, 0.20), twilight() * edge_light * (1.0 - low.y) * 0.55);
        color = mix(color, cloud_color, low.x * 0.94);
    }

    let dither = bayer(v.clip.xy) * 0.45;
    // More night steps retain dim nebulae and moon phase detail without banding;
    // the screen's final CRT/ASCII stage still controls the retro presentation.
    let levels = mix(160.0, 96.0, day);
    color = floor(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)) * levels + dither) / levels;
    return vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
}
