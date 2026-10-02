// Append to world.wgsl. Shares Globals, VertexIn/Out, transform_vertex and fs_main.
// Root contract: eight kinds x eight variants, 72 vertices per padded template.
struct CoverTemplateVertex { position:vec4<f32>, normal:vec4<f32>, color:vec4<f32>, surface:vec4<f32> };
struct CoverTile { origin:vec4<f32>, grid:vec4<u32> };
@group(2) @binding(0) var<storage,read> cover_templates:array<CoverTemplateVertex>;
@group(2) @binding(1) var<storage,read> cover_heights:array<f32>;
@group(2) @binding(2) var<uniform> cover_tile:CoverTile;
struct CoverIn {
    @location(0) placement:vec3<f32>,
    @location(1) rotation:vec2<f32>,
    @location(2) data:vec2<u32>,
};
fn cover_hash(seed:u32,x:u32,z:u32)->u32 {
    var h=seed ^ (x * 0x9e3779b9u) ^ (z * 0x85ebca6bu);
    h=(h ^ (h >> 16u))*0x7feb352du;
    h=(h ^ (h >> 15u))*0x846ca68bu;
    return h ^ (h >> 16u);
}
// Exact alternating LOD0 triangle height, plus its two surface derivatives.
fn cover_surface(local:vec2<f32>)->vec3<f32> {
    let f=clamp((local+vec2<f32>(6.0))/6.0,vec2<f32>(0.0),vec2<f32>(9.99999));
    let cell=vec2<u32>(floor(f)); let q=fract(f);
    let offset=cell.y*11u+cell.x;
    let a=cover_heights[offset];let b=cover_heights[offset+11u];
    let c=cover_heights[offset+12u];let d=cover_heights[offset+1u];
    let h=cover_hash(cover_tile.grid.x,cover_tile.grid.y+cell.x,cover_tile.grid.z+cell.y);
    if (h&1u)==0u {
        if q.x+q.y<=1.0 {return vec3<f32>(a+(d-a)*q.x+(b-a)*q.y,(d-a)/6.0,(b-a)/6.0);}
        return vec3<f32>(c+(b-c)*(1.0-q.x)+(d-c)*(1.0-q.y),(c-b)/6.0,(c-d)/6.0);
    }
    if q.y>=q.x {return vec3<f32>(a+(c-b)*q.x+(b-a)*q.y,(c-b)/6.0,(b-a)/6.0);}
    return vec3<f32>(a+(d-a)*q.x+(c-d)*q.y,(d-a)/6.0,(c-d)/6.0);
}
@vertex
fn vs_cover(input:CoverIn,@builtin(vertex_index) vertex:u32)->VertexOut {
    let plant=cover_templates[input.data.y*72u+vertex];
    // Templates share a fixed draw budget; padded vertices need no terrain,
    // normal, wind or lighting transform work at 400% cover density.
    if plant.position.w<0.5 {
        var empty:VertexOut;
        empty.clip=vec4<f32>(0.0,0.0,2.0,1.0);
        return empty;
    }
    let p=plant.position.xyz*input.placement.z;
    let sine=input.rotation.x;let cosine=input.rotation.y;
    let local=vec2<f32>(p.x*cosine-p.z*sine,p.x*sine+p.z*cosine)+input.placement.xy;
    let world_xz=local+cover_tile.origin.xy;
    // Account for world-position float precision before looking up the surface.
    let surface=cover_surface(world_xz-cover_tile.origin.xy);
    let n=plant.normal.xyz;
    let rotated=vec3<f32>(n.x*cosine-n.z*sine,n.y,n.x*sine+n.z*cosine);
    let sheared=vec3<f32>(rotated.x-surface.y*rotated.y,rotated.y,rotated.z-surface.z*rotated.y);
    var normal=vec3<f32>(0.0,1.0,0.0);
    if dot(sheared,sheared)>0.00000001 {normal=normalize(sheared);}
    let packed=input.data.x;
    let tint=vec3<f32>(f32(packed&255u),f32((packed>>8u)&255u),f32((packed>>16u)&255u))/255.0;
    // Dark basal pigment joins the plant to soil; the upper leaves and flowers
    // retain their palette. This is local shading, not an extra shadow sample.
    let root_light=mix(0.60,1.0,smoothstep(0.0,0.32,p.y));
    let color=plant.color.rgb*mix(vec3<f32>(1.0),tint,plant.color.w)*root_light;
    let weight=clamp(p.y/0.65,0.0,1.0);
    return transform_vertex(VertexIn(vec3<f32>(world_xz.x,surface.x+p.y-0.018,world_xz.y),
        vec4<f32>(normal,-f32(packed>>24u)/255.0),color,6.0+weight*0.4,plant.surface.xy,plant.surface.z));
}

// Opaque ribbons: dense coverage without transparent card padding or per-blade
// normal/reflection textures. Original flowers, fern cards and litter overlay it.
struct MeadowIn { @location(0) root:vec4<f32>, @location(1) data:vec4<u32> };
@vertex fn vs_meadow(input:MeadowIn,@builtin(vertex_index) vertex:u32)->VertexOut {
    let ribbon=vertex/6u; let v=vertex%6u;
    let points=array<vec2<f32>,6>(vec2<f32>(-1.0,0.0),vec2<f32>(1.0,0.0),vec2<f32>(0.35,0.60),
        vec2<f32>(-1.0,0.0),vec2<f32>(0.35,0.60),vec2<f32>(0.0,1.0));
    let q=points[v];
    let rotation=unpack2x16snorm(input.data.y);
    let axis=select(rotation,vec2<f32>(-rotation.y,rotation.x),ribbon==1u);
    let widths=unpack2x16float(input.data.z);
    let blade_height=input.root.w*select(1.0,0.83,ribbon==1u);
    let bend=vec2<f32>(axis.y,-axis.x)*blade_height*0.30*q.y*q.y;
    let local=input.root.xy+axis*q.x*widths.x+bend;
    let world_xz=local+cover_tile.origin.xy;
    let ground=cover_surface(world_xz-cover_tile.origin.xy);
    let packed=input.data.x;
    let tint=vec3<f32>(f32(packed&255u),f32((packed>>8u)&255u),f32((packed>>16u)&255u))/255.0;
    let variation=0.90+f32(input.data.w&255u)/255.0*0.20;
    let pigment=tint*variation*mix(0.68,1.13,q.y);
    let n=normalize(vec3<f32>(-axis.y*0.26-ground.y,0.95,axis.x*0.26-ground.z));
    let root_clip=u.view_projection*vec4<f32>(world_xz.x-u.camera.x,ground.x-u.camera.y,world_xz.y-u.camera.z,1.0);
    let pixel_height=blade_height*u.settings.w*0.688191/max(root_clip.w,0.2);
    let wind_detail=mix(0.35,1.0,smoothstep(2.0,10.0,pixel_height));
    var o=transform_vertex(VertexIn(vec3<f32>(world_xz.x,ground.x+q.y*blade_height-0.012,world_xz.y),
        vec4<f32>(n,-f32(packed>>24u)/255.0),pigment,6.0+q.y*sqrt(widths.y*wind_detail*min(blade_height/0.65,1.0))*0.4,vec2<f32>(q.x*0.5+0.5,q.y),-1.0));
    // Part gently around the player. This is a local bend, never a simulation
    // over the entire continent; roots remain fixed and recover as we pass.
    let away=o.world.xz-u.camera.xz;let distance=length(away);
    let push=(1.0-smoothstep(0.25,1.15,distance))*q.y*q.y*widths.y;
    o.world.x+=away.x/max(distance,0.15)*push*0.32;
    o.world.z+=away.y/max(distance,0.15)*push*0.32;
    o.world.y-=push*blade_height*0.28;
    o.clip=u.view_projection*vec4<f32>(o.world-u.camera.xyz,1.0);
    return o;
}
@fragment fn fs_meadow(v:VertexOut)->@location(0) vec4<f32> {
    let distance=length(v.world-u.camera.xyz);
    let n=normalize(v.normal);
    let sky=open_sky(v.world);
    let visibility=sun_visibility(v.world,n)*weather_light_visibility(v.world);
    let snow=smoothstep(0.02,0.80,u.surface.y)*sky*smoothstep(0.15,0.80,v.uv.y);
    // Quantized pigment stripes keep a procedural pixel-art character while
    // avoiding an alpha test, normal map or full material BRDF for every blade.
    let texel=v.uv*vec2<f32>(4.0,16.0);
    let footprint=max(length(dpdx(texel)),length(dpdy(texel)));
    let pixel=mix(hash21(floor(texel)),0.5,smoothstep(0.6,1.6,footprint));
    let broad=smoothstep(0.52-max(fwidth(v.uv.x),0.02),0.52+max(fwidth(v.uv.x),0.02),v.uv.x);
    let stripe=mix(0.90,1.03,broad)*(0.95+pixel*0.10);
    var pigment=v.color*stripe*(1.0-u.surface.x*0.16*sky*(1.0-snow));
    pigment=mix(pigment,vec3<f32>(0.84,0.89,0.91),snow);
    let linear=pow(max(pigment,vec3<f32>(0.0)),vec3<f32>(2.2));
    let access=select(1.0,sky,u.shelter_params.z>0.5);
    var color=surface_lighting(pigment,linear,n,6.0,visibility,distance,access,v.world,ambient_occlusion(v.clip.xy),v.clip.xyz);
    let view=(u.camera.xyz-v.world)/max(distance,0.0001);
    let light=normalize(u.light.xyz);
    let transmission=pow(max(dot(-view,light),0.0),3.0);
    color+=linear*u.direct.rgb*u.direct.w*visibility*(0.10+transmission*0.60)*v.uv.y;
    let wet=u.surface.x*sky*(1.0-snow);
    let rough=mix(0.82,0.92,snow);
    color+=vegetation_highlight(n,view,light,rough,wet,visibility)*0.30;
    if u.hearths[0].w>0.0 { color+=hearth_illumination(v.world,n,view,linear,rough,0.0,true,-1); }
    return vec4<f32>(atmospheric_color(color,v.world,distance),1.0);
}

// Nearby ribbons retain their exact wind, terrain shear and player deformation
// in depth. Larger structures provide ambient contact beyond 20 m; the color
// pass retains every ribbon, with its existing dark basal pigment.
@fragment fn fs_meadow_depth(v:VertexOut) {
    let relative=v.world.xz-u.camera.xz;
    if dot(relative,relative)>20.0*20.0 {discard;}
}
