// Bounded analytic room probes. Windows and door apertures match the mesh.
fn room_local(p:vec3<f32>,i:u32)->vec3<f32>{let a=u.rooms_a[i];let b=u.rooms_b[i];let q=p-a.xyz;return vec3<f32>(q.x*b.z-q.z*b.w,q.y,q.x*b.w+q.z*b.z);}
fn room_at(p:vec3<f32>)->i32{
    if distance(p.xz,u.camera.xz)>95.0{return -1;}
    for(var i=0u;i<12u;i+=1u){let a=u.rooms_a[i];if a.w<=0.0{break;}let q=room_local(p,i);let b=u.rooms_b[i];if abs(q.x)<b.x-0.10&&abs(q.z)<b.y-0.10&&q.y>=-0.015&&q.y<a.w+0.005{return i32(i);}}
    return -1;
}
fn room_aperture(p:vec3<f32>,direction:vec3<f32>,i:u32)->f32{return probe_aperture(room_local(p,i),probe_direction(direction,u.rooms_b[i]),u.rooms_a[i],u.rooms_b[i],u.rooms_c[i]);}
fn room_ambient(p:vec3<f32>,i:u32)->f32{let q=room_local(p,i);let b=u.rooms_b[i];let side=length(vec2<f32>(b.x-abs(q.x),q.z));let back=length(vec2<f32>(q.x,b.y-q.z));let front=length(vec2<f32>(q.x,b.y+q.z));let window=min(side,back);return 0.035+0.43/(1.0+window*window*0.28)+u.rooms_c[i].x*0.22/(1.0+front*front*0.3);}
fn room_fire_visibility(p:vec3<f32>,source:vec3<f32>,source_room:i32,to:i32)->f32{if source_room==to{if to<0{return 1.0;}let i=u32(to);return probe_partition(room_local(source,i),probe_direction(normalize(p-source),u.rooms_b[i]),length(p-source),u.rooms_c[i]);}if source_room>=0 {if room_aperture(source,normalize(p-source),u32(source_room))<0.5{return 0.0;}}if to>=0 {if room_aperture(p,normalize(source-p),u32(to))<0.5{return 0.0;}}return 1.0;}
