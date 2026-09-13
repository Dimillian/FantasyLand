// Bounded analytic room probes. Windows and door apertures match the mesh.
fn room_local(p:vec3<f32>,i:u32)->vec3<f32>{let a=u.rooms_a[i];let b=u.rooms_b[i];let q=p-a.xyz;return vec3<f32>(q.x*b.z-q.z*b.w,q.y,q.x*b.w+q.z*b.z);}
fn room_at(p:vec3<f32>)->i32{
    if distance(p.xz,u.camera.xz)>95.0{return -1;}
    for(var i=0u;i<12u;i+=1u){let a=u.rooms_a[i];if a.w<=0.0{break;}let q=room_local(p,i);let b=u.rooms_b[i];if abs(q.x)<b.x-0.10&&abs(q.z)<b.y-0.10&&q.y>=-0.015&&q.y<a.w+0.005{return i32(i);}}
    return -1;
}
fn room_aperture(p:vec3<f32>,direction:vec3<f32>,i:u32)->f32{
    let b=u.rooms_b[i];let a=u.rooms_a[i];let q=room_local(p,i);let d=vec3<f32>(direction.x*b.z-direction.z*b.w,direction.y,direction.x*b.w+direction.z*b.z);
    let raw_t=(vec3<f32>(select(-b.x,b.x,d.x>0.0),select(0.0,a.w,d.y>0.0),select(-b.y,b.y,d.z>0.0))-q)/select(vec3<f32>(0.00001),d,abs(d)>vec3<f32>(0.00001));
    let t=select(vec3<f32>(1e8),raw_t,abs(d)>vec3<f32>(0.00001));
    let travel=max(min(min(t.x,t.y),t.z),0.0);let hit=q+d*travel;
    if t.y<min(t.x,t.z){return 0.0;}
    if t.x<t.z {return select(0.0,1.0,abs(hit.z)<0.83&&hit.y>1.12&&hit.y<2.48);}
    if hit.z<0.0 {
        if !(abs(hit.x)<0.84&&hit.y>0.0&&hit.y<2.53){return 0.0;}
        // Intersect the actual hinged leaf, so daylight follows the opening
        // continuously instead of switching on halfway through its animation.
        let angle=u.rooms_c[i].x*1.6022123;let cs=cos(angle);let sn=sin(angle);
        let relative=q-vec3<f32>(-0.86,0.0,-b.y);
        let leaf_q=vec3<f32>(relative.x*cs+relative.z*sn,relative.y,-relative.x*sn+relative.z*cs);
        let leaf_d=vec3<f32>(d.x*cs+d.z*sn,d.y,-d.x*sn+d.z*cs);
        if abs(leaf_d.z)>0.00001 {let lt=-leaf_q.z/leaf_d.z;let lh=leaf_q+leaf_d*lt;if lt>=0.0&&lh.x>=0.0&&lh.x<=1.72&&lh.y>0.03&&lh.y<2.5{return 0.0;}}
        return 1.0;
    }
    return select(0.0,1.0,abs(hit.x)<0.83&&hit.y>1.12&&hit.y<2.48);
}
fn room_ambient(p:vec3<f32>,i:u32)->f32{let q=room_local(p,i);let b=u.rooms_b[i];let side=length(vec2<f32>(b.x-abs(q.x),q.z));let back=length(vec2<f32>(q.x,b.y-q.z));let front=length(vec2<f32>(q.x,b.y+q.z));let window=min(side,back);return 0.035+0.43/(1.0+window*window*0.28)+u.rooms_c[i].x*0.22/(1.0+front*front*0.3);}
fn room_fire_visibility(p:vec3<f32>,source:vec3<f32>,source_room:i32,to:i32)->f32{if source_room==to{return 1.0;}if source_room>=0 {if room_aperture(source,normalize(p-source),u32(source_room))<0.5{return 0.0;}}if to>=0 {if room_aperture(p,normalize(source-p),u32(to))<0.5{return 0.0;}}return 1.0;}
