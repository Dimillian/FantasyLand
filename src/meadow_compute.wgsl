// Compact 1.5m ecological cells into only the nearby, visible grass instances.
struct Frame { projection:mat4x4<f32>, eye:vec4<f32>, settings:vec4<f32> };
struct Tile { origin:vec4<f32>, grid:vec4<u32> };
struct Instance { root:vec4<f32>, data:vec4<u32> };
struct Args { vertices:u32, count:atomic<u32>, first_vertex:u32, first_instance:u32 };
@group(0) @binding(0) var<uniform> frame:Frame;
@group(0) @binding(1) var<storage,read> cells:array<vec4<u32>>;
@group(0) @binding(2) var<storage,read> heights:array<f32>;
@group(0) @binding(3) var<uniform> tile:Tile;
@group(0) @binding(4) var<storage,read_write> instances:array<Instance>;
@group(0) @binding(5) var<storage,read_write> args:Args;
fn hash(seed:u32)->u32 {
    var h=seed; h=(h^(h>>16u))*0x7feb352du; h=(h^(h>>15u))*0x846ca68bu;
    return h^(h>>16u);
}
fn random(seed:u32)->f32 {return f32(hash(seed)&0xffffffu)/16777216.0;}
fn surface(local:vec2<f32>)->f32 {
    let f=clamp((local+6.0)/6.0,vec2<f32>(0.0),vec2<f32>(9.99999));
    let cell=vec2<u32>(floor(f));let q=fract(f);let offset=cell.y*11u+cell.x;
    let a=heights[offset];let b=heights[offset+11u];let c=heights[offset+12u];let d=heights[offset+1u];
    let h=hash(tile.grid.x ^ ((tile.grid.y+cell.x)*0x9e3779b9u) ^ ((tile.grid.z+cell.y)*0x85ebca6bu));
    if (h&1u)==0u {
        if q.x+q.y<=1.0 {return a+(d-a)*q.x+(b-a)*q.y;}
        return c+(b-c)*(1.0-q.x)+(d-c)*(1.0-q.y);
    }
    if q.y>=q.x {return a+(c-b)*q.x+(b-a)*q.y;}
    return a+(d-a)*q.x+(c-d)*q.y;
}
@compute @workgroup_size(64)
fn expand(@builtin(global_invocation_id) id:vec3<u32>) {
    let index=id.x/64u;let sub=id.x%64u;
    if index>=arrayLength(&cells) {return;}
    let cell=cells[index];
    if ((cell.w>>16u)&(1u<<(sub/4u)))==0u {return;}
    let seed=hash(cell.x ^ (sub*0x9e3779b9u));
    let grid=cell.w&65535u;
    let local=(vec2<f32>(f32(grid%32u),f32(grid/32u))*8.0
        +vec2<f32>(f32(sub%8u),f32(sub/8u))+0.5
        +vec2<f32>(random(seed+1u)-0.5,random(seed+2u)-0.5)*0.64)*0.1875;
    let world=local+tile.origin.xy;
    let delta=world-frame.eye.xz;
    let distance=length(delta);
    if distance>=frame.settings.y {return;}
    let coverage=f32((cell.z>>8u)&255u)/255.0*frame.settings.x;
    // A stratified permutation spreads ecological thinning within each cell;
    // no random rejection of entire metre-wide squares in lush meadows.
    let rank=(f32((sub*21u+(cell.x&63u))&63u)+0.5)/64.0;
    if rank>coverage {return;}
    // A stable, uniformly distributed quarter subset survives into the midground.
    let quarter=(sub%2u==0u && (sub/8u)%2u==0u);
    let mid=(sub%4u==0u && (sub/8u)%4u==0u);
    let jitter=random(seed+4u)*4.0;
    let near_fade=1.0-smoothstep(12.0+jitter,22.0+jitter,distance);
    let middle_fade=1.0-smoothstep(22.0+jitter,34.0+jitter,distance);
    let fade=select(near_fade,select(middle_fade,1.0,mid),quarter)
        *(1.0-smoothstep(38.0,frame.settings.y,distance));
    if fade<0.015 {return;}
    let base_height=f32(cell.z&255u)/255.0*(0.85+random(seed+5u)*0.30);
    let root=surface(world-tile.origin.xy)-0.012;
    let clip=frame.projection*vec4<f32>(world.x-frame.eye.x,root+base_height*0.5-frame.eye.y,world.y-frame.eye.z,1.0);
    let metres_per_pixel=max(clip.w,0.2)/max(frame.settings.z,1.0);
    let projected_height=base_height/metres_per_pixel;
    // Remove only unresolved remnants. Use the original height so LOD shrinkage
    // cannot feed back into this decision, and protect the full near meadow.
    let pixel_fade=mix(smoothstep(0.45,1.4,projected_height),1.0,1.0-smoothstep(6.0,10.0,distance));
    let height=base_height*fade*pixel_fade;
    if pixel_fade<0.015 {return;}
    // Conservative world-radius expansion includes bent tips and near-plane crossings.
    let margin=height+0.8;
    let rows=transpose(frame.projection);
    let left=length((rows[3]+rows[0]).xyz)*margin;
    let right=length((rows[3]-rows[0]).xyz)*margin;
    let bottom=length((rows[3]+rows[1]).xyz)*margin;
    let top=length((rows[3]-rows[1]).xyz)*margin;
    if clip.x < -clip.w-left || clip.x > clip.w+right
        || clip.y < -clip.w-bottom || clip.y > clip.w+top {return;}
    let angle=random(seed+6u)*6.2831853;
    let base_width=(0.026+random(seed+7u)*0.019)*mix(1.0,2.25,smoothstep(15.0,36.0,distance));
    // A blade has two half-widths. Aim for 0.6 pixels across, with a strict
    // expansion cap; distant blades never become broad billboard rectangles.
    let stable_width=max(base_width,min(metres_per_pixel*0.30,base_width*1.45));
    let width=mix(base_width,stable_width,smoothstep(7.0,22.0,distance))*sqrt(fade*pixel_fade);
    // One invocation appends at most one instance. Capacity is cells*64, so the
    // indirect count cannot overflow even if every candidate passes all tests.
    let output=atomicAdd(&args.count,1u);
    instances[output]=Instance(vec4<f32>(local,root,height),vec4<u32>(cell.y,
        pack2x16snorm(vec2<f32>(sin(angle),cos(angle))),pack2x16float(vec2<f32>(width,fade*pixel_fade)),seed));
}
