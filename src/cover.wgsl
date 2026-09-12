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
        vec4<f32>(normal,0.0),color,6.0+weight*0.4,plant.surface.xy,plant.surface.z));
}
