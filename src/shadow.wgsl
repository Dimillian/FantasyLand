// time = elapsed seconds, shared regional wind strength, 0, 0.
struct ShadowUniform {
    matrix: mat4x4<f32>,
    origin: vec4<f32>,
    time: vec4<f32>,
    wind_field:WindField,
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
@group(3) @binding(0) var leaf_atlas: texture_2d_array<f32>;
@group(3) @binding(3) var leaf_sampler: sampler;
struct ShadowOut {
    @builtin(position) clip:vec4<f32>,
    @location(0) uv:vec2<f32>,
    @location(1) @interpolate(flat) texture:f32,
};
@vertex fn vs_shadow(v: ShadowVertex) -> ShadowOut {
    var p = v.position;
    p += wind_displacement(p,v.material,v.normal.w,shadow.wind_field);
    return ShadowOut(shadow.matrix * vec4<f32>(p - shadow.origin.xyz, 1.0),v.uv,v.texture);
}

@fragment fn fs_shadow(v:ShadowOut) {
    let dx=dpdx(v.uv);let dy=dpdy(v.uv);
    if v.texture==16.0 || v.texture>=1000.0 {discard;}
    if ((v.texture>=5.0 && v.texture<10.0) || (v.texture>=28.0 && v.texture<33.0)) {
        if textureSampleGrad(leaf_atlas,leaf_sampler,v.uv,i32(v.texture),dx,dy).a<0.4 {discard;}
    }
}
