struct ShadowUniform {
    matrix: mat4x4<f32>,
    origin: vec4<f32>,
    time: vec4<f32>,
};
@group(0) @binding(0) var<uniform> shadow: ShadowUniform;
struct ShadowVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) material: f32,
};
@vertex fn vs_shadow(v: ShadowVertex) -> @builtin(position) vec4<f32> {
    var p = v.position;
    // Match canopy movement in the color pass; roots stay fixed.
    if v.material > 0.5 && v.material < 1.5 {
        let weight = clamp((1.4 - v.material) / 0.4, 0.0, 1.0);
        p.x += sin(shadow.time.x * 0.85 + p.x * 0.13 + p.z * 0.17) * 0.06 * weight;
    }
    return shadow.matrix * vec4<f32>(p - shadow.origin.xyz, 1.0);
}
