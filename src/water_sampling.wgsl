// Append once to the world shader. Binding group 1 is also bound for reflected
// terrain/cover, keeping shared fragment entry points compatible.
struct LocalWaterParams {
    grid: vec4<f32>, timing: vec4<f32>, wind: vec4<f32>, wake: vec4<f32>, activity: vec4<f32>,
};
@group(1) @binding(0) var<uniform> local_water_params: LocalWaterParams;
@group(1) @binding(1) var<storage,read> local_water_state: array<vec4<f32>>;
@group(1) @binding(2) var<storage,read> local_water_bed: array<vec4<f32>>;
struct LocalWater {height:f32,slope:vec2<f32>,foam:f32,weight:f32};
fn local_cell(c:vec2<i32>,level:f32)->vec2<f32>{
    let n=i32(local_water_params.grid.w);
    if(any(c<vec2<i32>(0)) || any(c>=vec2<i32>(n))){return vec2<f32>(0.0);}
    let i=u32(c.y*n+c.x); let bed=local_water_bed[i];
    if(bed.x<0.01 || abs(bed.y-level)>1.2){return vec2<f32>(0.0);}
    let s=local_water_state[i];return vec2<f32>(s.x,s.w);
}
fn local_water(world:vec3<f32>)->LocalWater{
    var result:LocalWater; result.height=0.0;result.slope=vec2<f32>(0.0);result.foam=0.0;result.weight=0.0;
    if(local_water_params.timing.w<0.5){return result;}
    let pos=(world.xz-local_water_params.grid.xy)/local_water_params.grid.z;
    let n=local_water_params.grid.w;
    if(any(pos<vec2<f32>(1.0)) || any(pos>vec2<f32>(n-2.0))){return result;}
    let c=vec2<i32>(floor(pos)); let f=fract(pos);
    let a=local_cell(c,world.y); let b=local_cell(c+vec2<i32>(1,0),world.y);
    let d=local_cell(c+vec2<i32>(0,1),world.y); let e=local_cell(c+vec2<i32>(1,1),world.y);
    let edge=min(min(pos.x,pos.y),min(n-1.0-pos.x,n-1.0-pos.y));
    let nearest=u32(c.y)*u32(n)+u32(c.x);
    let valid=select(0.0,1.0,local_water_bed[nearest].x>0.01 && abs(local_water_bed[nearest].y-world.y)<1.2);
    result.weight=smoothstep(2.0,12.0,edge)*valid;
    result.height=mix(mix(a.x,b.x,f.x),mix(d.x,e.x,f.x),f.y)*result.weight;
    result.foam=mix(mix(a.y,b.y,f.x),mix(d.y,e.y,f.x),f.y)*result.weight;
    // Derivative of the same bilinear evolving surface used for displacement.
    result.slope=vec2<f32>(mix(b.x-a.x,e.x-d.x,f.y),mix(d.x-a.x,e.x-b.x,f.x))/local_water_params.grid.z*result.weight;
    return result;
}
