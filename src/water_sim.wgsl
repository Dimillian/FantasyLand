// Staggered finite-volume linear shallow-water waves around generated rest
// levels. State = [surface displacement, east discharge, north discharge, foam].
// The momentum pass reads old heights and writes face fluxes. A separate
// continuity dispatch exchanges those exact fluxes, making internal exchanges
// conservative. Bathymetry supplies reflecting land walls and wave shoaling.
struct WaterParams {
    grid: vec4<f32>, timing: vec4<f32>, wind: vec4<f32>, wake: vec4<f32>, activity: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: WaterParams;
@group(0) @binding(1) var<storage, read> before: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> after: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> bed: array<vec4<f32>>;

fn index(c:vec2<i32>)->u32 { return u32(c.y)*u32(p.grid.w)+u32(c.x); }
fn inside(c:vec2<i32>)->bool { return all(c>=vec2<i32>(0)) && all(c<vec2<i32>(i32(p.grid.w))); }
fn wet(c:vec2<i32>)->bool { if(!inside(c)){return false;} return bed[index(c)].x>0.0; }
fn face_depth(a:vec2<i32>,b:vec2<i32>)->f32 {
    if(!wet(a)||!wet(b)){return 0.0;}
    let aa=bed[index(a)]; let bb=bed[index(b)];
    if(abs(aa.y-bb.y)>1.5){return 0.0;}
    return min(aa.x,bb.x);
}
fn hash(v:vec3<u32>)->f32 {
    var h=v.x*1597334677u+v.y*3812015801u+v.z*2798796415u;
    h=(h^(h>>16u))*2246822519u; h=(h^(h>>13u))*3266489917u;
    return f32(h^(h>>16u))/4294967295.0;
}
fn wind_pressure(world:vec2<f32>)->f32 {
    let wind_speed=min(length(p.wind.xy),22.0);
    let travel=p.wind.zw;
    let f=(world-travel)/6.0;
    let c=vec2<i32>(floor(f)); let t=smoothstep(vec2<f32>(0.0),vec2<f32>(1.0),fract(f));
    let a=hash(vec3<u32>(bitcast<vec2<u32>>(c),191u));
    let b=hash(vec3<u32>(bitcast<vec2<u32>>(c+vec2<i32>(1,0)),191u));
    let d=hash(vec3<u32>(bitcast<vec2<u32>>(c+vec2<i32>(0,1)),191u));
    let e=hash(vec3<u32>(bitcast<vec2<u32>>(c+vec2<i32>(1,1)),191u));
    return (mix(mix(a,b,t.x),mix(d,e,t.x),t.y)-0.5)*wind_speed*0.002;
}
fn pressure(c:vec2<i32>)->f32 {
    if(!wet(c)){return 0.0;}
    let world=p.grid.xy+vec2<f32>(c)*p.grid.z;
    let tick=u32(round(p.timing.y/p.timing.x));
    let world_cell=bitcast<vec2<u32>>(vec2<i32>(round(world/p.grid.z)));
    let impact=select(0.0,0.12,hash(vec3<u32>(world_cell,tick))<p.timing.z*p.timing.x*0.085);
    let offset=world-p.wake.xy;
    let speed=length(p.wake.zw);
    let direction=p.wake.zw/max(speed,0.01);
    let along=dot(offset,direction);
    let across=offset-direction*along;
    let wake_pressure=p.activity.x*0.030*exp(-dot(across,across)/0.45-along*along/1.1);
    return impact+wind_pressure(world)+wake_pressure;
}
fn edge_damping(c:vec2<i32>)->f32 {
    let edge=min(min(f32(c.x),f32(c.y)),min(p.grid.w-1.0-f32(c.x),p.grid.w-1.0-f32(c.y)));
    return (1.0-smoothstep(0.0,9.0,edge))*2.5;
}

@compute @workgroup_size(8,8)
fn velocity_step(@builtin(global_invocation_id) gid:vec3<u32>){
    let c=vec2<i32>(gid.xy); if(!inside(c)){return;}
    let i=index(c); if(!wet(c)){after[i]=vec4<f32>(0.0);return;}
    let center=before[i]; let rest=bed[i];
    let right=c+vec2<i32>(1,0); let north=c+vec2<i32>(0,1);
    let dx=p.grid.z; let dt=p.timing.x;
    let d_e=face_depth(c,right); let d_n=face_depth(c,north);
    let center_pressure=center.x+pressure(c);
    var flux=vec2<f32>(0.0);
    if(d_e>0.0){
        flux.x=center.y-9.81*d_e*dt/dx*(before[index(right)].x+pressure(right)-center_pressure);
    }
    if(d_n>0.0){
        flux.y=center.z-9.81*d_n*dt/dx*(before[index(north)].x+pressure(north)-center_pressure);
    }
    let damping=exp(-dt*(0.20+edge_damping(c)+0.02/max(rest.x,0.05)));
    flux*=damping;
    // Positivity/energy guard for abrupt player contact or changing banks. In
    // normal operation these bounds never engage; no wet/dry flooding occurs.
    flux=clamp(flux,-vec2<f32>(d_e,d_n)*1.4,vec2<f32>(d_e,d_n)*1.4);
    after[i]=vec4<f32>(center.x,flux,center.w);
}
fn exchange(a:vec2<i32>,b:vec2<i32>,axis:u32)->f32 {
    if(face_depth(a,b)<=0.0){return 0.0;}
    let ai=index(a); let bi=index(b);
    let velocity=clamp((bed[ai][axis+2u]+bed[bi][axis+2u])*0.5,-4.5,4.5);
    let advected=select(before[bi].x,before[ai].x,velocity>=0.0);
    return before[ai][axis+1u]+velocity*advected;
}
@compute @workgroup_size(8,8)
fn surface_step(@builtin(global_invocation_id) gid:vec3<u32>){
    let c=vec2<i32>(gid.xy); if(!inside(c)){return;}
    let i=index(c); if(!wet(c)){after[i]=vec4<f32>(0.0);return;}
    let center=before[i]; let rest=bed[i];
    let east=exchange(c,c+vec2<i32>(1,0),0u);
    let west=exchange(c-vec2<i32>(1,0),c,0u);
    let north=exchange(c,c+vec2<i32>(0,1),1u);
    let south=exchange(c-vec2<i32>(0,1),c,1u);
    let change=-(east-west+north-south)*p.timing.x/p.grid.z;
    let ceiling=min(0.24,rest.x*0.24);
    let height=clamp((center.x+change)*exp(-p.timing.x*edge_damping(c)), -ceiling,ceiling);
    let agitation=abs(change)/p.timing.x;
    let shallow=1.0-smoothstep(0.15,1.6,rest.x);
    let foam=clamp(center.w*exp(-p.timing.x*0.95)+max(agitation-0.065,0.0)*p.timing.x*(1.1+shallow*4.0),0.0,1.0);
    after[i]=vec4<f32>(height,center.yz,foam);
}
@compute @workgroup_size(8,8)
fn remap_step(@builtin(global_invocation_id) gid:vec3<u32>){
    let c=vec2<i32>(gid.xy); if(!inside(c)){return;}
    let old=c+vec2<i32>(p.activity.yz); let i=index(c);
    if(p.activity.w>0.5 || !inside(old) || !wet(c)){after[i]=vec4<f32>(0.0);return;}
    after[i]=before[index(old)];
}
