// The global scene group binds these two resources in both main and reflected
// cameras. This file is appended after gi_common.wgsl by the renderer.
@group(0) @binding(10) var<uniform> gi_state: GiUniform;
@group(0) @binding(11) var<storage, read> gi_irradiance: array<GiProbe>;

fn gi_lookup(world: vec3<f32>, normal: vec3<f32>) -> vec4<f32> {
    if gi_state.parameters.y < 0.5 { return vec4<f32>(0.0); }
    let local = world - gi_state.origin_cell.xyz;
    let extent = vec3<f32>(gi_state.voxel_dims.xyz) * gi_state.origin_cell.w;
    let margin = min(min(local.x, local.y), local.z);
    let far_margin = min(min(extent.x - local.x, extent.y - local.y), extent.z - local.z);
    let boundary_weight = smoothstep(1.0, 5.0, min(margin, far_margin));
    if boundary_weight <= 0.0 { return vec4<f32>(0.0); }
    // Normal bias avoids asking visibility moments to cross the receiving wall.
    let point = local + normal * 0.22;
    let grid = (point - vec3<f32>(0.75)) / gi_state.parameters.x;
    let base = vec3<i32>(floor(grid));
    let fraction = fract(grid);
    // Split each probe cell into six tetrahedra. Sorting the fractional axes
    // chooses the containing tetrahedron; its four barycentric weights exactly
    // reproduce affine fields and stay continuous where two fractions tie.
    var axes = vec3<u32>(0u, 1u, 2u);
    if fraction[axes.x] < fraction[axes.y] { axes = axes.yxz; }
    if fraction[axes.y] < fraction[axes.z] { axes = axes.xzy; }
    if fraction[axes.x] < fraction[axes.y] { axes = axes.yxz; }
    let ordered = vec3<f32>(fraction[axes.x], fraction[axes.y], fraction[axes.z]);
    let barycentric = vec4<f32>(1.0 - ordered.x, ordered.x - ordered.y, ordered.y - ordered.z, ordered.z);
    var irradiance = vec3<f32>(0.0);
    var total_weight = 0.0;
    var available_weight = 0.0;
    for (var corner = 0u; corner < 4u; corner += 1u) {
        var bit = vec3<u32>(0u);
        if corner >= 1u { bit[axes.x] = 1u; }
        if corner >= 2u { bit[axes.y] = 1u; }
        if corner >= 3u { bit[axes.z] = 1u; }
        let coordinate = base + vec3<i32>(bit);
        if any(coordinate < vec3<i32>(0)) || any(coordinate >= vec3<i32>(gi_state.probe_dims.xyz)) { continue; }
        let index = u32(coordinate.x) + u32(coordinate.y) * gi_state.probe_dims.x + u32(coordinate.z) * gi_state.probe_dims.x * gi_state.probe_dims.y;
        let probe = gi_irradiance[index];
        if probe.position.w < 0.5 { continue; }
        let interpolation = barycentric[corner];
        let delta = point - probe.position.xyz;
        let absolute = abs(delta);
        let axis = select(select(2u, 1u, absolute.y >= absolute.z), 0u, absolute.x >= max(absolute.y, absolute.z));
        let bin = axis * 2u + select(0u, 1u, delta[axis] < 0.0);
        var mean: f32;
        var moment: f32;
        if bin < 4u { mean = probe.distance0[bin]; moment = probe.moment0[bin]; }
        else { mean = probe.distance1[bin - 4u]; moment = probe.moment1[bin - 4u]; }
        let distance = absolute[axis];
        let variance = max(moment - mean * mean, 0.03);
        let beyond = max(distance - mean - 0.18, 0.0);
        let chebyshev = variance / (variance + beyond * beyond);
        let visibility = select(chebyshev * chebyshev * chebyshev, 1.0, distance <= mean + 0.18);
        let toward_probe = normalize(probe.position.xyz - point + vec3<f32>(0.0001));
        let backface = pow(clamp(dot(normal, toward_probe) * 0.5 + 0.5, 0.05, 1.0), 2.0);
        let weight = interpolation * visibility * backface;
        let incident = max(probe.sh0.rgb + probe.shx.rgb * normal.x + probe.shy.rgb * normal.y + probe.shz.rgb * normal.z, vec3<f32>(0.0));
        irradiance += incident * weight;
        total_weight += weight;
        available_weight += interpolation;
    }
    if total_weight < 0.00001 { return vec4<f32>(0.0); }
    let result = irradiance / total_weight;
    // Coverage fades while new worker data/probes arrive, and at the volume
    // boundary. Outside coverage the original ambient lighting remains exact.
    return vec4<f32>(result, boundary_weight * clamp(available_weight, 0.0, 1.0) * gi_state.parameters.z);
}

fn gi_indirect(world: vec3<f32>, normal: vec3<f32>, legacy: vec3<f32>) -> vec3<f32> {
    let value = gi_lookup(world, normal);
    return mix(legacy, value.rgb, value.a);
}
