struct GiScreenUniform {
    inverse_view_projection: mat4x4<f32>,
    camera: vec4<f32>,
    resolution: vec4<u32>, // scene XY, half-resolution cache ZW
}
@group(0) @binding(2) var gi_scene_depth: texture_depth_2d;
@group(0) @binding(3) var<uniform> gi_screen_state: GiScreenUniform;
@group(0) @binding(4) var gi_screen_output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(5) var gi_screen_depth_output: texture_storage_2d<r32float, write>;

fn gi_screen_depth_at(pixel: vec2<i32>) -> f32 {
    let size = vec2<i32>(gi_screen_state.resolution.xy);
    let p = clamp(pixel, vec2<i32>(0), size - vec2<i32>(1));
    return textureLoad(gi_scene_depth, p, 0);
}

fn gi_screen_position(pixel: vec2<i32>, depth: f32) -> vec3<f32> {
    let size = vec2<i32>(gi_screen_state.resolution.xy);
    let p = clamp(pixel, vec2<i32>(0), size - vec2<i32>(1));
    let uv = (vec2<f32>(p) + vec2<f32>(0.5)) / vec2<f32>(size);
    let clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let relative = gi_screen_state.inverse_view_projection * clip;
    return relative.xyz / max(abs(relative.w), 0.0000001);
}

@compute @workgroup_size(8, 8)
fn cache_indirect(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= gi_screen_state.resolution.zw) { return; }
    let pixel = min(vec2<i32>(id.xy) * 2 + vec2<i32>(1), vec2<i32>(gi_screen_state.resolution.xy) - vec2<i32>(1));
    let depth = gi_screen_depth_at(pixel);
    // The main scene pass writes its depth attachment, so interpolation reads
    // this independent cache instead of sampling that attached texture.
    textureStore(gi_screen_depth_output, vec2<i32>(id.xy), vec4<f32>(depth, 0.0, 0.0, 0.0));
    if depth >= 0.9999999 || gi_state.parameters.y < 0.5 {
        textureStore(gi_screen_output, vec2<i32>(id.xy), vec4<f32>(0.0));
        return;
    }
    let point = gi_screen_position(pixel, depth);
    let world = point + gi_screen_state.camera.xyz;
    let local = world - gi_state.origin_cell.xyz;
    let extent = vec3<f32>(gi_state.voxel_dims.xyz) * gi_state.origin_cell.w;
    // The lookup's boundary fade is exactly zero here; skip depth derivatives
    // and probe reads for scenery beyond the local lighting volume.
    if any(local <= vec3<f32>(1.0)) || any(local >= extent - vec3<f32>(1.0)) {
        textureStore(gi_screen_output, vec2<i32>(id.xy), vec4<f32>(0.0));
        return;
    }
    // Select the adjacent sample on the same surface on each axis. Blind
    // central derivatives pull building/foliage normals toward distant scenery.
    let left = gi_screen_depth_at(pixel - vec2<i32>(1, 0));
    let right = gi_screen_depth_at(pixel + vec2<i32>(1, 0));
    let up = gi_screen_depth_at(pixel - vec2<i32>(0, 1));
    let down = gi_screen_depth_at(pixel + vec2<i32>(0, 1));
    let prefer_right = abs(right - depth) <= abs(left - depth);
    let prefer_down = abs(down - depth) <= abs(up - depth);
    // Comparing scalar depths first reconstructs only the winning neighbor on
    // each axis, preserving the same normal and tie-breaking behavior.
    let adjacent_x = gi_screen_position(pixel + select(vec2<i32>(-1, 0), vec2<i32>(1, 0), prefer_right), select(left, right, prefer_right));
    let adjacent_y = gi_screen_position(pixel + select(vec2<i32>(0, -1), vec2<i32>(0, 1), prefer_down), select(up, down, prefer_down));
    let dx = select(point - adjacent_x, adjacent_x - point, prefer_right);
    let dy = select(point - adjacent_y, adjacent_y - point, prefer_down);
    let raw = cross(dy, dx);
    let length2 = dot(raw, raw);
    if length2 < 0.0000000001 {
        textureStore(gi_screen_output, vec2<i32>(id.xy), vec4<f32>(0.0));
        return;
    }
    let direction = raw * inverseSqrt(length2);
    let normal = select(-direction, direction, dot(direction, -point) >= 0.0);
    textureStore(gi_screen_output, vec2<i32>(id.xy), gi_lookup(world, normal));
}
