// Shared by visible geometry, depth, reflections and both shadow cascades.
// No textures, per-plant state, CPU transforms, or additional draw calls.
struct WindField { flow:vec4<f32>, phase:vec4<f32> };
fn wind_displacement(p:vec3<f32>,material:f32,response:f32,field:WindField)->vec3<f32> {
    let leaf=material>0.5 && material<1.5;
    let grass=material>5.5 && material<6.5;
    let wood=material>2.5 && material<3.5 && response>0.0;
    if (!leaf && !grass && !wood) || field.flow.z<0.0001 {return vec3<f32>(0.0);}
    let speed=length(field.flow.xy);
    let direction=field.flow.xy/max(speed,0.0001);
    let across=vec2<f32>(-direction.y,direction.x);
    // Fixed world-space bases + integrated advection: weather turns never
    // rotate a phase grid about the world origin or about the moving camera.
    let a=sin(dot(p.xz,vec2<f32>(0.045,0.027))-field.phase.x);
    let b=sin(dot(p.xz,vec2<f32>(-0.021,0.051))-field.phase.y);
    let c=sin(dot(p.xz,vec2<f32>(0.013,-0.018))-field.phase.z);
    let pulse=0.5+0.5*a;
    let pressure=0.24+0.24*(b+1.0)+pulse*pulse*(0.30+field.flow.w*0.65);
    let broad=(direction*pressure+across*c*(0.12+field.flow.w*0.10))*field.flow.z;
    if grass {
        let weight=clamp((material-6.0)/0.4,0.0,1.0);
        let exposure=select(1.0,clamp(-response,0.2,1.0),response<0.0);
        let shiver=sin(dot(p.xz,vec2<f32>(0.73,0.91))-field.phase.w)*0.025;
        let bend=(broad+across*shiver*field.flow.z)*(0.24*weight*weight*exposure);
        return vec3<f32>(bend.x,-min(dot(bend,bend)*0.22,0.055)*weight,bend.y);
    }
    let weight=clamp((1.4-material)/0.4,0.0,1.0);
    // A common height envelope moves stems and their attached crowns together.
    // response is baked from height, species flexibility and local exposure.
    let flex=sin(dot(p.xz,vec2<f32>(0.16,0.11))-field.phase.w*0.25);
    var bend=(broad+across*flex*field.flow.z*0.16)*max(response,0.0)*0.48;
    if leaf {
        let flutter=sin(dot(p.xz,vec2<f32>(1.7,1.1))+p.y*1.3-field.phase.w);
        let attachment=select(weight,0.35+min(response,1.0)*0.65,response>0.0);
        bend+=broad*weight*0.10 + across*flutter*0.060*field.flow.z*attachment;
    }
    return vec3<f32>(bend.x,0.0,bend.y);
}
