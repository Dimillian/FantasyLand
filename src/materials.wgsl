// Immutable procedural material library: 17 layers, shared by every chunk.
@group(3) @binding(0) var material_color: texture_2d_array<f32>;
@group(3) @binding(1) var material_surface: texture_2d_array<f32>;
@group(3) @binding(2) var material_sampler: sampler;
@group(3) @binding(3) var foliage_sampler: sampler;
struct PixelMaterial { pigment:vec3<f32>, normal:vec3<f32>, roughness:f32, metal:f32, emission:f32, alpha:f32 };
fn pixel_material(v:VertexOut, footprint:f32,grad:SurfaceGrad) -> PixelMaterial {
    let n = normalize(v.normal);
    let a = abs(n);
    var uv = v.world.xz * 0.22;
    var dx=grad.world_x.xz*0.22; var dy=grad.world_y.xz*0.22;
    var tangent = vec3<f32>(1.0,0.0,0.0);
    var bitangent = vec3<f32>(0.0,0.0,1.0);
    if a.y < max(a.x,a.z) {
        if a.x > a.z { uv = v.world.zy * 0.28; dx=grad.world_x.zy*0.28;dy=grad.world_y.zy*0.28; tangent = vec3<f32>(0.0,0.0,1.0); }
        else { uv = v.world.xy * 0.28; dx=grad.world_x.xy*0.28;dy=grad.world_y.xy*0.28; }
        bitangent = vec3<f32>(0.0,1.0,0.0);
    }
    var layer = 0i;
    if v.material < 0.5 || (v.material > 6.5 && v.material < 7.5) {
        let green = v.color.g-v.color.r;
        layer = select(0i,1i,green > 0.008);
        if v.color.r > 0.53 && v.color.b < v.color.r*0.79 { layer = 10i; }
        if n.y < 0.64 {layer = 2i;}
    } else if v.material < 1.5 { layer = 1i; }
    else if v.material < 2.5 { layer = 2i; }
    else if v.material < 3.5 { layer = 4i; uv *= vec2<f32>(1.6,0.7); dx*=vec2<f32>(1.6,0.7);dy*=vec2<f32>(1.6,0.7); }
    else if v.material < 5.5 { layer = 11i; uv *= 2.0;dx*=2.0;dy*=2.0; }
    else if v.material < 6.5 { layer = 1i; }
    else if v.material > 8.5 {layer = 1i;}
    if v.texture >= 0.0 {layer=i32(v.texture+0.1);}
    if v.texture>=5.0 && v.texture<10.0 {uv=v.uv;dx=grad.uv_x;dy=grad.uv_y;}
    if v.material > 9.5 && v.material < 10.5 {
        layer=16i; uv=v.uv*vec2<f32>(1.0,0.9)+vec2<f32>(0.0,-u.params.x*0.47);
        dx=grad.uv_x*vec2<f32>(1.0,0.9);dy=grad.uv_y*vec2<f32>(1.0,0.9);
    }
    // Explicit gradients keep mip selection valid across material/alpha branches.

    let cutout=v.texture>=5.0 && v.texture<10.0;
    var tex:vec4<f32>;
    var packed:vec4<f32>;
    if cutout {
        tex=textureSampleGrad(material_color,foliage_sampler,uv,layer,dx,dy);
        if tex.a<0.4 {discard;}
        packed=textureSampleGrad(material_surface,foliage_sampler,uv,layer,dx,dy);
    } else {
        tex=textureSampleGrad(material_color,material_sampler,uv,layer,dx,dy);
        packed=textureSampleGrad(material_surface,material_sampler,uv,layer,dx,dy);
    }
    if layer==2i && n.y>0.35 {
        let moss_amount=smoothstep(0.55,0.80,noise(v.world.xz*0.31))*smoothstep(0.35,0.85,n.y)*u.air.z*0.55;
        let moss=textureSampleGrad(material_color,material_sampler,uv,15,dx,dy);
        tex=vec4<f32>(mix(tex.rgb,moss.rgb*vec3<f32>(0.64,0.91,0.44),moss_amount),tex.a);
    }
    if u.surface.y>0.04 && n.y>0.25 && !cutout {
        let snow=textureSampleGrad(material_surface,material_sampler,uv,13,dx,dy);
        // Snow crystal texture varies roughness; deposition stays in the weather shader.
        tex=vec4<f32>(tex.rgb*(0.98+snow.b*0.025),tex.a);
    }
    let detail=1.0-smoothstep(0.30,1.8,footprint);
    let strength=select(0.40,0.14,cutout)*detail;
    let bump=(packed.xy*2.0-vec2<f32>(1.0))*strength;
    // Stable world-space frame for opaque surfaces; foliage keeps its soft crown normal.
    let mapped=normalize(n + tangent*bump.x + bitangent*bump.y);
    var pigment=v.color*mix(vec3<f32>(1.0),tex.rgb*1.45,select(0.94,1.0,cutout));
    // Broad biome pigments remain legible; textured leaves introduce small cool/warm planes.
    if cutout { pigment *= vec3<f32>(0.94,1.06,0.96); }
    return PixelMaterial(pigment,mapped,packed.b,select(0.0,0.85,layer==12i),packed.a,select(1.0,tex.a,cutout));
}
fn material_highlight(albedo:vec3<f32>,n:vec3<f32>,view:vec3<f32>,light:vec3<f32>,rough:f32,metal:f32,wet:f32,visibility:f32)->vec3<f32> {
    let facing_normal=select(-n,n,dot(n,view)>=0.0);
    let h=normalize(view+light);
    let nv=max(dot(facing_normal,view),0.001); let nl=max(dot(facing_normal,light),0.0);
    let nh=max(dot(facing_normal,h),0.0); let vh=max(dot(view,h),0.0);
    let r=clamp(mix(rough,0.20,wet),0.18,1.0);
    let alpha=r*r; let a2=alpha*alpha;
    let denominator=nh*nh*(a2-1.0)+1.0;
    let distribution=min(a2/(3.14159265*denominator*denominator),20.0);
    let k=(r+1.0)*(r+1.0)*0.125;
    let geometry=(nv/(nv*(1.0-k)+k))*(nl/(nl*(1.0-k)+k));
    let f0=mix(vec3<f32>(0.035),albedo,metal);
    let fresnel=f0+(vec3<f32>(1.0)-f0)*pow(1.0-vh,5.0);
    let spec=distribution*geometry*fresnel/max(4.0*nv*nl,0.001);
    let reflected=reflect(-view,facing_normal);
    // Environment response uses the same sky as water; roughness softens its energy.
    let sky=pow(max(sky_gradient(reflected),vec3<f32>(0.0)),vec3<f32>(2.2));
    let env_f=f0+(vec3<f32>(1.0)-f0)*pow(1.0-nv,5.0);
    return spec*nl*u.direct.rgb*u.direct.w*visibility*0.68
        + sky*env_f*(1.0-r)*(0.12+wet*0.55+metal*0.45);
}

// Rough leaves retain sunlight and rain sheen without evaluating a complete
// reflected sky and microfacet BRDF over every distant grass pixel.
fn vegetation_highlight(n:vec3<f32>,view:vec3<f32>,light:vec3<f32>,rough:f32,wet:f32,visibility:f32)->vec3<f32> {
    let r=clamp(mix(rough,0.20,wet),0.18,1.0);
    let facing = select(-n,n,dot(n,view)>=0.0);
    let half_sum=view+light;
    let halfway=half_sum*inverseSqrt(max(dot(half_sum,half_sum),0.00001));
    let nh=max(dot(facing,halfway),0.0);
    let nh2=nh*nh; let nh4=nh2*nh2; let nh8=nh4*nh4;
    let lobe=mix(nh8,nh8*nh8*nh8*nh8,wet);
    let nl=max(dot(facing,light),0.0);
    let sheen=(0.009+wet*0.12)*(1.0-r*0.65);
    let sky=u.ambient.rgb*(0.20+max(facing.y,0.0)*0.28);
    return u.direct.rgb*u.direct.w*visibility*nl*lobe*sheen
        + sky*0.035*(1.0-r)*(0.12+wet*0.55);
}
