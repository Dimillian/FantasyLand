struct AoUniform { inverse_projection:mat4x4<f32>, viewport:vec4<f32>, parameters:vec4<f32> };
@group(0) @binding(0) var<uniform> ao:AoUniform;
@group(0) @binding(1) var source_depth:texture_depth_2d;
@group(0) @binding(2) var source:texture_2d<f32>;
@group(0) @binding(3) var output:texture_storage_2d<rgba8unorm,write>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=vec2<u32>(ao.viewport.xy)) {return;}
    let pixel=vec2<i32>(id.xy);
    let depth=textureLoad(source_depth,pixel,0);
    if depth>=0.999999 {textureStore(output,pixel,vec4<f32>(1.0));return;}
    let reciprocal_depth=ao.inverse_projection[2][3]*depth+ao.inverse_projection[3][3];
    let linear_depth=1.0/max(abs(reciprocal_depth),0.0000001);
    let half_pixel=(vec2<f32>(id.xy)+0.5)*0.5-0.5;
    let base=vec2<i32>(floor(half_pixel));let fraction=fract(half_pixel);
    var visibility=0.0;var total=0.0;
    for(var k=0u;k<4u;k+=1u) {
        let corner=vec2<i32>(i32(k&1u),i32(k>>1u));
        let p=clamp(base+corner,vec2<i32>(0),vec2<i32>(ao.viewport.zw)-vec2<i32>(1));
        let sample=textureLoad(source,p,0);
        let weights=select(vec2<f32>(1.0)-fraction,fraction,corner==vec2<i32>(1));
        let spatial=weights.x*weights.y;
        let depth_weight=exp(-abs(sample.g-linear_depth)/max(0.035,linear_depth*0.015));
        let weight=spatial*depth_weight;
        visibility+=sample.r*weight;total+=weight;
    }
    // If every half-resolution sample is from another surface, leave ambient
    // intact rather than smearing a dark silhouette across a disocclusion.
    let result=select(1.0,visibility/max(total,0.0001),total>0.001);
    textureStore(output,pixel,vec4<f32>(clamp(result,0.0,1.0),0.0,0.0,1.0));
}
