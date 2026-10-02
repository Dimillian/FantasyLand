struct AoUniform { inverse_projection:mat4x4<f32>, viewport:vec4<f32>, parameters:vec4<f32> };
@group(0) @binding(0) var<uniform> ao:AoUniform;
@group(0) @binding(1) var source:texture_2d<f32>;
@group(0) @binding(2) var output:texture_storage_2d<rgba16float,write>;
fn oct_decode(packed:vec2<f32>)->vec3<f32> {
    let p=packed*2.0-1.0;
    var n=vec3<f32>(p,1.0-abs(p.x)-abs(p.y));
    let fold=clamp(-n.z,0.0,1.0);
    n.x+=select(fold,-fold,n.x>=0.0);n.y+=select(fold,-fold,n.y>=0.0);
    return normalize(n);
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=vec2<u32>(ao.viewport.zw)) {return;}
    let pixel=vec2<i32>(id.xy);
    let center=textureLoad(source,pixel,0);
    if center.g>=ao.parameters.z {textureStore(output,pixel,center);return;}
    let normal=oct_decode(center.ba);
    var sum=0.0;var total=0.0;
    for(var y=-1;y<=1;y+=1) {
        for(var x=-1;x<=1;x+=1) {
            let p=clamp(pixel+vec2<i32>(x,y),vec2<i32>(0),vec2<i32>(ao.viewport.zw)-vec2<i32>(1));
            let sample=textureLoad(source,p,0);
            let spatial=select(1.0,2.0,x==0)*select(1.0,2.0,y==0);
            let depth_weight=exp(-abs(sample.g-center.g)/max(0.025,center.g*0.012));
            let normal_weight=pow(max(dot(normal,oct_decode(sample.ba)),0.0),8.0);
            let weight=spatial*depth_weight*normal_weight;
            sum+=sample.r*weight;total+=weight;
        }
    }
    textureStore(output,pixel,vec4<f32>(sum/max(total,0.0001),center.gba));
}
