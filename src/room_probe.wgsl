// Shared analytic aperture contract for surfaces, local lights and indoor shafts.
fn probe_local(p:vec3<f32>,a:vec4<f32>,b:vec4<f32>)->vec3<f32>{let q=p-a.xyz;return vec3<f32>(q.x*b.z-q.z*b.w,q.y,q.x*b.w+q.z*b.z);}
fn probe_direction(d:vec3<f32>,b:vec4<f32>)->vec3<f32>{return vec3<f32>(d.x*b.z-d.z*b.w,d.y,d.x*b.w+d.z*b.z);}
fn probe_partition(q:vec3<f32>,d:vec3<f32>,limit:f32,c:vec4<f32>)->f32{
    if c.w<0.5||abs(d.z)<0.00001{return 1.0;}
    let t=(c.y-q.z)/d.z;
    if t<=0.001||t>=limit{return 1.0;}
    let hit=q+d*t;return select(0.0,1.0,abs(hit.x)<c.z-0.08&&hit.y<2.5);
}
fn probe_window(x:f32,y:f32)->f32 {
    if abs(x)>=0.80||y<=1.2||y>=2.49||abs(x)<0.052||abs(y-1.815)<0.04{return 0.0;}
    let uv=vec2<f32>((x+0.83)/1.66,(y-1.13)/1.35);
    let lead=min(abs(fract((uv.x+uv.y)*3.0)-0.5),abs(fract((uv.x-uv.y)*3.0)-0.5));
    return select(0.78,0.0,lead<0.016);
}
fn probe_aperture(q:vec3<f32>,d:vec3<f32>,a:vec4<f32>,b:vec4<f32>,c:vec4<f32>)->f32 {
    let raw=(vec3<f32>(select(-b.x,b.x,d.x>0.0),select(0.0,a.w,d.y>0.0),select(-b.y,b.y,d.z>0.0))-q)/select(vec3<f32>(0.00001),d,abs(d)>vec3<f32>(0.00001));
    let t=select(vec3<f32>(1e8),raw,abs(d)>vec3<f32>(0.00001));
    let travel=max(min(min(t.x,t.y),t.z),0.0);let hit=q+d*travel;
    if probe_partition(q,d,travel,c)<0.5||t.y<min(t.x,t.z){return 0.0;}
    if t.x<t.z{return probe_window(hit.z,hit.y);}
    if hit.z>=0.0{return probe_window(hit.x,hit.y);}
    if !(abs(hit.x)<0.84&&hit.y>0.0&&hit.y<2.53){return 0.0;}
    let angle=c.x*1.6022123;let cs=cos(angle);let sn=sin(angle);let relative=q-vec3<f32>(-0.86,0.0,-b.y);
    let lq=vec3<f32>(relative.x*cs+relative.z*sn,relative.y,-relative.x*sn+relative.z*cs);let ld=vec3<f32>(d.x*cs+d.z*sn,d.y,-d.x*sn+d.z*cs);
    if abs(ld.z)>0.00001{let lt=-lq.z/ld.z;let h=lq+ld*lt;if lt>=0.0&&h.x>=0.0&&h.x<=1.72&&h.y>0.03&&h.y<2.5{return 0.0;}}
    return 1.0;
}
