@group(0) @binding(0) var<uniform> gi: GiUniform;
@group(0) @binding(1) var<storage, read> gi_voxels: array<u32>;
@group(0) @binding(2) var<storage, read_write> gi_probes: array<GiProbe>;
@group(0) @binding(3) var<storage, read> gi_previous: array<GiProbe>;

fn voxel_inside(c: vec3<i32>) -> bool {
    return all(c >= vec3<i32>(0)) && all(c < vec3<i32>(gi.voxel_dims.xyz));
}
fn voxel_index(c: vec3<i32>) -> u32 {
    return u32(c.x) + u32(c.y) * gi.voxel_dims.x + u32(c.z) * gi.voxel_dims.x * gi.voxel_dims.y;
}
fn voxel_at(p: vec3<f32>) -> u32 {
    let c = vec3<i32>(floor(p / gi.origin_cell.w));
    if !voxel_inside(c) { return 0u; }
    return gi_voxels[voxel_index(c)];
}
struct GiHit { position: vec3<f32>, distance: f32, voxel: u32, visibility: f32 }

// Grid DDA visits every crossed voxel, including thin walls. Fixed-step marching
// can miss half-metre partitions at diagonal angles and is deliberately avoided.
fn gi_trace_ignoring_source_cell(start: vec3<f32>, direction: vec3<f32>, limit: f32, opaque_only: bool, source_cell: vec3<i32>) -> GiHit {
    let cell = gi.origin_cell.w;
    var c = vec3<i32>(floor(start / cell));
    let step = vec3<i32>(select(vec3<f32>(-1.0), vec3<f32>(1.0), direction >= vec3<f32>(0.0)));
    let safe = select(vec3<f32>(0.000001), abs(direction), abs(direction) > vec3<f32>(0.000001));
    let delta = vec3<f32>(cell) / safe;
    let boundary = (vec3<f32>(c) + select(vec3<f32>(0.0), vec3<f32>(1.0), direction >= vec3<f32>(0.0))) * cell;
    var next = select(vec3<f32>(1e20), abs(boundary - start) / safe, abs(direction) > vec3<f32>(0.000001));
    var distance = 0.0;
    var transmission = 1.0;
    for (var iteration = 0u; iteration < 320u; iteration += 1u) {
        if !voxel_inside(c) || distance >= limit { break; }
        let packed = select(gi_voxels[voxel_index(c)], 0u, all(c == source_cell));
        let opacity = f32((packed >> 27u) & 15u) / 15.0;
        if opacity > 0.0 {
            if !opaque_only { return GiHit(start + direction * distance, distance, packed, opacity); }
            transmission *= 1.0 - opacity;
            if transmission <= 0.025 { return GiHit(start + direction * distance, distance, packed, 0.0); }
        }
        // Stable axis ordering also handles simultaneous edge crossings without
        // skipping cells. Zero direction components never become the minimum.
        let axis = select(select(2u, 1u, next.y <= next.z), 0u, next.x <= min(next.y, next.z));
        distance = next[axis];
        next[axis] += delta[axis];
        c[axis] += step[axis];
    }
    return GiHit(start + direction * min(distance, limit), min(distance, limit), 0u, transmission);
}
fn gi_trace(start: vec3<f32>, direction: vec3<f32>, limit: f32, opaque_only: bool) -> GiHit {
    return gi_trace_ignoring_source_cell(start, direction, limit, opaque_only, vec3<i32>(-1000));
}
fn packed_normal(packed: u32) -> vec3<f32> {
    let code = (packed >> 24u) & 7u;
    var n = vec3<f32>(0.0);
    n[code / 2u] = select(1.0, -1.0, (code & 1u) == 1u);
    return n;
}
fn gi_sky(direction: vec3<f32>) -> vec3<f32> {
    let up = clamp(direction.y * 0.5 + 0.5, 0.0, 1.0);
    return gi.ambient.rgb * mix(vec3<f32>(0.20, 0.22, 0.18), vec3<f32>(0.56, 0.62, 0.69), up);
}
fn hit_radiance(hit: GiHit, incoming: vec3<f32>) -> vec3<f32> {
    let albedo = vec3<f32>(f32(hit.voxel & 255u), f32((hit.voxel >> 8u) & 255u), f32((hit.voxel >> 16u) & 255u)) / 255.0;
    let raw_normal = packed_normal(hit.voxel);
    let normal = select(raw_normal, -raw_normal, dot(raw_normal, incoming) > 0.0);
    // Move beyond the conservative voxel skin before tracing lighting. The ray
    // hit is at a cell entry, rather than an exact triangle intersection.
    let source = hit.position - incoming * (gi.origin_cell.w * 1.15 + 0.02);
    let sun = normalize(gi.sun.xyz);
    let sunlight = gi_trace(source, sun, 90.0, true).visibility;
    let up_sky = gi_trace(source, normalize(normal + vec3<f32>(0.0, 1.25, 0.0)), 90.0, true).visibility;
    var light = gi.direct.rgb * max(dot(normal, sun), 0.0) * gi.sun.w * sunlight * 1.08;
    light += gi.ambient.rgb * vec3<f32>(0.33, 0.37, 0.39) * up_sky;
    for (var source_index = 0u; source_index < 8u; source_index += 1u) {
        let emitter = gi.hearths[source_index];
        if emitter.w <= 0.0 { continue; }
        let vector = emitter.xyz - gi.origin_cell.xyz - source;
        let distance2 = dot(vector, vector);
        if distance2 >= emitter.w * emitter.w { continue; }
        let distance = sqrt(max(distance2, 0.0001));
        let direction = vector / distance;
        // A conservative half-metre cell may contain both the actual air-space
        // flame and its firebox/candle holder. That enclosing SOURCE cell cannot
        // shadow its own emitter. Every intervening cell still blocks light.
        let source_cell = vec3<i32>(floor((emitter.xyz - gi.origin_cell.xyz) / gi.origin_cell.w));
        let visibility = gi_trace_ignoring_source_cell(source, direction, max(distance - 0.01, 0.0), true, source_cell).visibility;
        let fade = 1.0 - smoothstep(emitter.w * 0.45, emitter.w, distance);
        let power = select(select(34.0, 11.0, emitter.w < 10.0), 2.8, emitter.w < 3.5);
        light += vec3<f32>(1.0, 0.54, 0.23) * power * fade * fade * visibility * max(dot(normal, direction), 0.0) / (1.0 + distance2);
    }
    // Foliage captures reflected green light and transmits some sky light.
    return albedo * light * hit.visibility + gi_sky(incoming) * (1.0 - hit.visibility);
}
fn probe_coordinates(index: u32) -> vec3<u32> {
    return vec3<u32>(index % gi.probe_dims.x, (index / gi.probe_dims.x) % gi.probe_dims.y, index / (gi.probe_dims.x * gi.probe_dims.y));
}
fn probe_index(c: vec3<u32>) -> u32 {
    return c.x + c.y * gi.probe_dims.x + c.z * gi.probe_dims.x * gi.probe_dims.y;
}
@compute @workgroup_size(64)
fn scroll_probes(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= gi.probe_dims.w { return; }
    let destination = probe_coordinates(id.x);
    let source = vec3<i32>(destination) + gi.scroll.xyz;
    var result: GiProbe;
    if gi.scroll.w > 0 && all(source >= vec3<i32>(0)) && all(source < vec3<i32>(gi.probe_dims.xyz)) {
        result = gi_previous[probe_index(vec3<u32>(source))];
        result.position = vec4<f32>(result.position.xyz - vec3<f32>(gi.scroll.xyz) * gi.parameters.x, result.position.w);
    }
    gi_probes[id.x] = result;
}
@compute @workgroup_size(32)
fn update_probes(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= gi.update.y { return; }
    let index = gi.update.x + id.x;
    let coordinate = probe_coordinates(index);
    var position = vec3<f32>(coordinate) * gi.parameters.x + vec3<f32>(0.75);
    var result: GiProbe;
    // Invalid probes inside solids contribute nothing. Try six local relocation
    // directions first; the sampling shader uses the actual relocated position.
    if ((voxel_at(position) >> 27u) & 15u) >= 12u {
        var found = false;
        for (var axis = 0u; axis < 3u; axis += 1u) {
            for (var sign_index = 0u; sign_index < 2u; sign_index += 1u) {
                var offset = vec3<f32>(0.0);
                offset[axis] = select(-0.75, 0.75, sign_index == 1u);
                let alternative = position + offset;
                if !found && all(alternative >= vec3<f32>(0.1)) && all(alternative < vec3<f32>(gi.voxel_dims.xyz) * gi.origin_cell.w - vec3<f32>(0.1)) && ((voxel_at(alternative) >> 27u) & 15u) < 12u {
                    position = alternative;
                    found = true;
                }
            }
        }
        if !found { gi_probes[index] = result; return; }
    }
    result.position = vec4<f32>(position, 1.0);
    var sh0 = vec3<f32>(0.0);
    var shx = vec3<f32>(0.0);
    var shy = vec3<f32>(0.0);
    var shz = vec3<f32>(0.0);
    var distances: array<f32, 6>;
    var moments: array<f32, 6>;
    var counts: array<f32, 6>;
    // A deterministic sphere sequence means camera captures have no temporal
    // randomness and each probe converges after a single scheduled update.
    const RAYS: u32 = 32u;
    for (var ray = 0u; ray < RAYS; ray += 1u) {
        let z = 1.0 - 2.0 * (f32(ray) + 0.5) / f32(RAYS);
        let radius = sqrt(max(1.0 - z * z, 0.0));
        let angle = f32(ray) * 2.39996323 + f32(index % 17u) * 0.37;
        let direction = vec3<f32>(cos(angle) * radius, z, sin(angle) * radius);
        let hit = gi_trace(position, direction, 72.0, false);
        var radiance = gi_sky(direction);
        if hit.voxel != 0u { radiance = hit_radiance(hit, direction); }
        sh0 += radiance;
        shx += radiance * direction.x * 2.0;
        shy += radiance * direction.y * 2.0;
        shz += radiance * direction.z * 2.0;
        let absolute = abs(direction);
        let axis = select(select(2u, 1u, absolute.y >= absolute.z), 0u, absolute.x >= max(absolute.y, absolute.z));
        let bin = axis * 2u + select(0u, 1u, direction[axis] < 0.0);
        // Projection onto the major axis gives directional distance moments
        // compatible with the interpolation shader's probe-to-surface distance.
        let projected = max(hit.distance * absolute[axis], 0.25);
        distances[bin] += projected;
        moments[bin] += projected * projected;
        counts[bin] += 1.0;
    }
    result.sh0 = vec4<f32>(sh0 / f32(RAYS), 0.0);
    result.shx = vec4<f32>(shx / f32(RAYS), 0.0);
    result.shy = vec4<f32>(shy / f32(RAYS), 0.0);
    result.shz = vec4<f32>(shz / f32(RAYS), 0.0);
    for (var bin = 0u; bin < 6u; bin += 1u) {
        let mean = distances[bin] / max(counts[bin], 1.0);
        let moment = moments[bin] / max(counts[bin], 1.0);
        if bin < 4u { result.distance0[bin] = mean; result.moment0[bin] = moment; }
        else { result.distance1[bin - 4u] = mean; result.moment1[bin - 4u] = moment; }
    }
    gi_probes[index] = result;
}
