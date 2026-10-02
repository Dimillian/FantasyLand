@group(0) @binding(0) var scene:texture_2d<f32>;
@group(0) @binding(1) var linear_sampler:sampler;
struct Settings { controls:vec2<f32>, output:vec2<f32>, weather:vec4<f32>, atmosphere:vec4<f32> };
@group(0) @binding(2) var<uniform> settings:Settings;
struct Out { @builtin(position) clip:vec4<f32>, @location(0) uv:vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Out {
    let uv=vec2<f32>(f32(i>>1u),f32(i&1u))*2.0;
    return Out(vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),0.0,1.0),uv);
}
fn nearest(uv:vec2<f32>)->vec3<f32> {
    let size=textureDimensions(scene);
    return textureLoad(scene,clamp(vec2<i32>(uv*vec2<f32>(size)),vec2<i32>(0),vec2<i32>(size)-1),0).rgb;
}
@fragment fn fs_main(o:Out)->@location(0) vec4<f32> {
    let strength=settings.controls.y;
    if settings.controls.x<1.5 || strength<=0.0 {return vec4<f32>(retro_palette(nearest(o.uv),o.clip.xy),1.0);}
    let q=o.uv*2.0-1.0;let bowed=q*(1.0+dot(q,q)*0.035*strength);let uv=bowed*0.5+0.5;
    let edge_distance=min(min(uv.x,uv.y),min(1.0-uv.x,1.0-uv.y));
    let edge=smoothstep(0.0,1.5/max(settings.output.x,settings.output.y),edge_distance);
    let size=vec2<f32>(textureDimensions(scene));
    let fringe=vec2<f32>(0.48*strength*(0.3+dot(q,q))/size.x,0.0);
    let beam_color=vec3<f32>(textureSampleLevel(scene,linear_sampler,uv+fringe,0.0).r,
        textureSampleLevel(scene,linear_sampler,uv,0.0).g,textureSampleLevel(scene,linear_sampler,uv-fringe,0.0).b);
    var color=retro_palette(mix(nearest(uv),beam_color,min(strength*0.72,1.0)),o.clip.xy);
    let lines=min(size.y,settings.output.y/3.0);
    let beam=pow(max(sin(fract(uv.y*lines)*3.14159265),0.0),0.65);
    color*=1.0-min(strength*0.30,0.45)*(1.0-beam);
    let phosphor=u32(o.clip.x)%3u;var mask=vec3<f32>(0.86);mask[phosphor]=1.12;
    color*=mix(vec3<f32>(1.0),mask,min(strength*0.68,1.0));
    color*=(1.0-smoothstep(0.3,1.8,dot(q,q))*0.16*strength)*(1.0+strength*0.065);
    return vec4<f32>(mix(vec3<f32>(0.004,0.006,0.008),color,edge),1.0);
}
