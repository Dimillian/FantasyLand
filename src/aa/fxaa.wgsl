// Spatial FXAA: directional edge search plus a restrained subpixel resolve.
@group(0) @binding(0) var color_texture: texture_2d<f32>;
struct Metrics { rt:vec4<f32> };
@group(0) @binding(1) var<uniform> metrics:Metrics;
@group(1) @binding(0) var linear_sampler:sampler;
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Out {
    let uv=vec2<f32>(f32(i>>1u),f32(i&1u))*2.0;
    return Out(vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),0.0,1.0),uv);
}
fn sample_color(uv:vec2<f32>)->vec3<f32> {
    return textureSampleLevel(color_texture,linear_sampler,uv,0.0).rgb;
}
fn luma(uv:vec2<f32>)->f32 {return dot(sample_color(uv),vec3<f32>(0.299,0.587,0.114));}
@fragment fn fs_main(o:Out)->@location(0) vec4<f32> {
    let px=metrics.rt.xy; let uv=o.uv; let color=sample_color(uv);
    let m=dot(color,vec3<f32>(0.299,0.587,0.114));
    let n=luma(uv-vec2<f32>(0.0,px.y)); let s=luma(uv+vec2<f32>(0.0,px.y));
    let w=luma(uv-vec2<f32>(px.x,0.0)); let e=luma(uv+vec2<f32>(px.x,0.0));
    let low=min(m,min(min(n,s),min(w,e))); let high=max(m,max(max(n,s),max(w,e)));
    let contrast=high-low;
    if contrast<max(0.045,high*0.125) {return vec4<f32>(color,1.0);}
    let nw=luma(uv-px); let ne=luma(uv+vec2<f32>(px.x,-px.y));
    let sw=luma(uv+vec2<f32>(-px.x,px.y)); let se=luma(uv+px);
    let horizontal=abs(nw+sw-2.0*w)+2.0*abs(n+s-2.0*m)+abs(ne+se-2.0*e);
    let vertical=abs(nw+ne-2.0*n)+2.0*abs(w+e-2.0*m)+abs(sw+se-2.0*s);
    let h=horizontal>=vertical;
    let a=select(w,n,h); let b=select(e,s,h);
    let first=abs(a-m)>=abs(b-m);
    let gradient=max(abs(a-m),abs(b-m));
    let average=(m+select(b,a,first))*0.5;
    let normal=select(vec2<f32>(px.x,0.0),vec2<f32>(0.0,px.y),h)*select(1.0,-1.0,first);
    let tangent=select(vec2<f32>(0.0,px.y),vec2<f32>(px.x,0.0),h);
    let origin=uv+normal*0.5;
    var negative=origin-tangent;var positive=origin+tangent;
    var ln=luma(negative)-average;var lp=luma(positive)-average;
    var done_n=abs(ln)>=gradient*0.25;var done_p=abs(lp)>=gradient*0.25;
    let steps=array<f32,8>(1.0,1.5,2.0,2.0,2.0,4.0,8.0,8.0);
    for(var i=0u;i<8u;i++) {
        if done_n && done_p {break;}
        if !done_n {negative-=tangent*steps[i];ln=luma(negative)-average;done_n=abs(ln)>=gradient*0.25;}
        if !done_p {positive+=tangent*steps[i];lp=luma(positive)-average;done_p=abs(lp)>=gradient*0.25;}
    }
    let dn=select(uv.y-negative.y,uv.x-negative.x,h);
    let dp=select(positive.y-uv.y,positive.x-uv.x,h);
    let nearer=dn<dp;
    let end_delta=select(lp,ln,nearer);
    let valid=(end_delta<0.0)!=(m-average<0.0);
    let edge=select(0.0,0.5-min(dn,dp)/max(dn+dp,0.00001),valid);
    let neighborhood=(2.0*(n+s+w+e)+nw+ne+sw+se)/12.0;
    let sub=clamp(abs(neighborhood-m)/contrast,0.0,1.0);
    let smooth_sub=sub*sub*(3.0-2.0*sub);
    let offset=max(edge,smooth_sub*smooth_sub*0.40);
    return vec4<f32>(sample_color(uv+normal*offset),1.0);
}
