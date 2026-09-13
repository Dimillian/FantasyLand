// time = elapsed seconds, shared regional wind strength, 0, 0.
struct ShadowUniform {
    matrix: mat4x4<f32>,
    origin: vec4<f32>,
    time: vec4<f32>,
};
@group(0) @binding(0) var<uniform> shadow: ShadowUniform;
struct ShadowVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) color: vec3<f32>,
    @location(3) material: f32,
    @location(4) uv: vec2<f32>,
    @location(5) texture: f32,
};
fn vegetation_wind(world: vec3<f32>, time: f32, strength: f32) -> vec2<f32> {
    let direction = normalize(shadow.time.zw + vec2<f32>(0.001,0.0));
    let across = vec2<f32>(-direction.y,direction.x);
    // Weather rotates displacement, never the absolute-coordinate phase field.
    // Otherwise a small wind turn becomes a large phase jump 60km from origin.
    let wave = time * 0.70 - dot(world.xz, vec2<f32>(0.82,0.57)) * 0.026;
    let cross_wave = time * 0.39 + dot(world.xz, vec2<f32>(-0.57,0.82)) * 0.019;
    let gust = 0.53 + sin(wave) * 0.27 + sin(cross_wave) * 0.16;
    let flutter = sin(time * 1.9 + dot(world.xz, vec2<f32>(0.31, 0.23))) * 0.08;
    return (direction * (gust + flutter) + across * sin(cross_wave) * 0.12)
         * clamp(strength, 0.0, 2.5);
}

@group(3) @binding(0) var leaf_atlas: texture_2d_array<f32>;
@group(3) @binding(3) var leaf_sampler: sampler;
struct ShadowOut {
    @builtin(position) clip:vec4<f32>,
    @location(0) uv:vec2<f32>,
    @location(1) @interpolate(flat) texture:f32,
};
@vertex fn vs_shadow(v: ShadowVertex) -> ShadowOut {
    var p = v.position;
    // Match canopy movement in the color pass; roots stay fixed.
    if v.material > 0.5 && v.material < 1.5 {
        let weight = clamp((1.4 - v.material) / 0.4, 0.0, 1.0);
        let bend = vegetation_wind(p, shadow.time.x, shadow.time.y) * (0.32 * weight);
        p.x += bend.x;
        p.z += bend.y;
    }
    return ShadowOut(shadow.matrix * vec4<f32>(p - shadow.origin.xyz, 1.0),v.uv,v.texture);
}

@fragment fn fs_shadow(v:ShadowOut) {
    let dx=dpdx(v.uv);let dy=dpdy(v.uv);
    if v.texture==16.0 || v.texture>=1000.0 {discard;}
    if v.texture>=5.0 && v.texture<10.0 {
        if textureSampleGrad(leaf_atlas,leaf_sampler,v.uv,i32(v.texture),dx,dy).a<0.4 {discard;}
    }
}
