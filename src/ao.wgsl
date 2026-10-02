// GTAO cosine-weighted horizon integration, reconstructed geometric normals.
// Analytical slice integral follows Jimenez et al., Practical Real-Time
// Strategies for Accurate Indirect Occlusion (2016). Implementation reference:
// https://github.com/GameTechDev/XeGTAO (Intel, MIT). This is a small independent
// WGSL implementation without XeGTAO's temporal noise, depth MIPs or bent normals.
struct AoUniform { inverse_projection:mat4x4<f32>, viewport:vec4<f32>, parameters:vec4<f32> };
@group(0) @binding(0) var<uniform> ao:AoUniform;
@group(0) @binding(1) var source_depth:texture_depth_2d;
@group(0) @binding(2) var output:texture_storage_2d<rgba16float,write>;
const PI:f32=3.141592653589793;

fn depth_at(pixel:vec2<i32>)->f32 {
    return textureLoad(source_depth,clamp(pixel,vec2<i32>(0),vec2<i32>(ao.viewport.xy)-vec2<i32>(1)),0);
}
fn position_at(pixel:vec2<i32>,depth:f32)->vec3<f32> {
    let uv=(vec2<f32>(pixel)+vec2<f32>(0.5))/ao.viewport.xy;
    // The renderer supplies an ordinary perspective inverse without camera
    // rotation. Its only depth-dependent homogeneous term is W, and Z is -1.
    // Avoid a general 4x4 multiply for every horizon/normal depth sample.
    let reciprocal_depth=ao.inverse_projection[2][3]*depth+ao.inverse_projection[3][3];
    let linear_depth=1.0/max(abs(reciprocal_depth),0.0000001);
    let ray=vec3<f32>((uv.x*2.0-1.0)*ao.inverse_projection[0][0],
        (1.0-uv.y*2.0)*ao.inverse_projection[1][1],-1.0);
    return ray*linear_depth;
}
fn geometric_normal(pixel:vec2<i32>,center:vec3<f32>)->vec3<f32> {
    let left=position_at(pixel+vec2<i32>(-1,0),depth_at(pixel+vec2<i32>(-1,0)));
    let right=position_at(pixel+vec2<i32>(1,0),depth_at(pixel+vec2<i32>(1,0)));
    let top=position_at(pixel+vec2<i32>(0,-1),depth_at(pixel+vec2<i32>(0,-1)));
    let bottom=position_at(pixel+vec2<i32>(0,1),depth_at(pixel+vec2<i32>(0,1)));
    let dx=select(center-left,right-center,abs(right.z-center.z)<abs(center.z-left.z));
    let dy=select(center-top,bottom-center,abs(bottom.z-center.z)<abs(center.z-top.z));
    let cross_normal=cross(dx,dy);
    if dot(cross_normal,cross_normal)<0.0000000001 {return normalize(-center);}
    let n=normalize(cross_normal);
    return select(-n,n,dot(n,-center)>=0.0);
}
fn oct_encode(n:vec3<f32>)->vec2<f32> {
    let p=n.xy/(abs(n.x)+abs(n.y)+abs(n.z));
    let folded=(vec2<f32>(1.0)-abs(p.yx))*select(vec2<f32>(-1.0),vec2<f32>(1.0),p>=vec2<f32>(0.0));
    return select(folded,p,n.z>=0.0)*0.5+0.5;
}
fn noise(pixel:vec2<u32>)->f32 {
    var h=(pixel.x*0x9e3779b9u) ^ (pixel.y*0x85ebca6bu);
    h=(h^(h>>16u))*0x7feb352du;
    h=(h^(h>>15u))*0x846ca68bu;
    return f32(h^(h>>16u))/4294967295.0;
}
fn horizon_sample(pixel:vec2<i32>,center:vec3<f32>,view:vec3<f32>,low:f32,radius:f32)->f32 {
    if any(pixel<vec2<i32>(0)) || any(pixel>=vec2<i32>(ao.viewport.xy)) {return low;}
    let d=depth_at(pixel);
    if d>=0.999999 {return low;}
    let delta=position_at(pixel,d)-center;
    let distance=length(delta);
    let weight=1.0-smoothstep(radius*0.30,radius,distance);
    // Finite local radius avoids the infinitely thick depth silhouettes that
    // otherwise cast broad halos from leaves and people onto distant scenery.
    return mix(low,dot(delta/max(distance,0.0001),view),weight);
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=vec2<u32>(ao.viewport.zw)) {return;}
    // Select the actual closest sample in this 2x2 footprint. Thin cutouts are
    // retained instead of averaging foreground geometry into background depth.
    let first=vec2<i32>(id.xy*2u);
    var pixel=first;
    var depth=depth_at(pixel);
    for(var k=1u;k<4u;k+=1u) {
        let p=first+vec2<i32>(i32(k&1u),i32(k>>1u));
        let d=depth_at(p);
        if d<depth {depth=d;pixel=p;}
    }
    if depth>=0.999999 {
        textureStore(output,vec2<i32>(id.xy),vec4<f32>(1.0,65504.0,0.5,0.5));return;
    }
    let center=position_at(pixel,depth);
    let linear_depth=-center.z;
    let focal=ao.viewport.x/(2.0*max(abs(ao.inverse_projection[0][0]),0.0001));
    let screen_radius=min(96.0,ao.parameters.x*focal/max(linear_depth,0.08));
    if linear_depth>ao.parameters.z || screen_radius<1.5 {
        textureStore(output,vec2<i32>(id.xy),vec4<f32>(1.0,min(linear_depth,65504.0),0.5,0.5));return;
    }
    let normal=geometric_normal(pixel,center);
    let packed_normal=oct_encode(normal);
    let view=normalize(-center);
    let jitter=noise(id.xy);
    var visibility=0.0;
    for(var slice=0u;slice<3u;slice+=1u) {
        let phi=(f32(slice)+jitter)*PI/3.0;
        let direction=vec3<f32>(cos(phi),sin(phi),0.0);
        let orthogonal=direction-view*dot(direction,view);
        let axis=normalize(cross(orthogonal,view));
        let projected=normal-axis*dot(normal,axis);
        let projected_length=max(length(projected),0.0001);
        let cosine_normal=clamp(dot(projected,view)/projected_length,0.0,1.0);
        let sign_normal=select(-1.0,1.0,dot(orthogonal,projected)>=0.0);
        let normal_angle=sign_normal*acos(cosine_normal);
        let sine_normal=sin(normal_angle);
        let low_positive=-sine_normal;
        let low_negative=sine_normal;
        var positive=low_positive;
        var negative=low_negative;
        let screen_direction=direction.xy*vec2<f32>(1.0,-1.0);
        for(var step=0u;step<4u;step+=1u) {
            let s=(f32(step)+0.65)/4.0;
            let offset=vec2<i32>(round(screen_direction*max(1.5,s*s*screen_radius)));
            positive=max(positive,horizon_sample(pixel+offset,center,view,low_positive,ao.parameters.x));
            negative=max(negative,horizon_sample(pixel-offset,center,view,low_negative,ao.parameters.x));
        }
        let h0=-acos(clamp(negative,-1.0,1.0));
        let h1=acos(clamp(positive,-1.0,1.0));
        let arc0=(cosine_normal+2.0*h0*sine_normal-cos(2.0*h0-normal_angle))*0.25;
        let arc1=(cosine_normal+2.0*h1*sine_normal-cos(2.0*h1-normal_angle))*0.25;
        visibility+=mix(projected_length,1.0,0.05)*(arc0+arc1);
    }
    visibility=clamp(visibility/3.0,0.0,1.0);
    visibility=mix(visibility,1.0,smoothstep(ao.parameters.z*0.65,ao.parameters.z,linear_depth));
    textureStore(output,vec2<i32>(id.xy),vec4<f32>(visibility,min(linear_depth,65504.0),packed_normal));
}
