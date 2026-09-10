@group(0) @binding(0) var scene:texture_2d<f32>;
struct Out {@builtin(position) clip:vec4<f32>, @location(0) uv:vec2<f32>};
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Out {
    var p=array<vec2<f32>,3>(vec2<f32>(-1.,-1.),vec2<f32>(3.,-1.),vec2<f32>(-1.,3.));
    var o:Out;o.clip=vec4<f32>(p[i],0.,1.);o.uv=vec2<f32>(p[i].x*0.5+0.5,0.5-p[i].y*0.5);return o;
}
@fragment fn fs_main(o:Out)->@location(0) vec4<f32> {
    let size=textureDimensions(scene);
    let pixel=clamp(vec2<i32>(o.uv*vec2<f32>(size)),vec2<i32>(0),vec2<i32>(size)-vec2<i32>(1));
    return textureLoad(scene,pixel,0);
}
