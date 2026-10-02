// Eight compact local sources from actual generated hearth geometry. Most
// frames have no sources: the zero radius ends the loop immediately. Sources
// use a finite radius, inverse-square attenuation and stable bounded flicker.
fn hearth_flicker(origin: vec3<f32>) -> f32 {
    let phase = fract(dot(floor(origin.xz),vec2<f32>(0.1031,0.11369))) * 6.2831853;
    return 0.88 + 0.075*sin(u.params.x*8.3+phase) + 0.045*sin(u.params.x*17.1+phase*1.7);
}

fn hearth_illumination(world: vec3<f32>, normal: vec3<f32>, view: vec3<f32>,
    albedo: vec3<f32>, roughness: f32, metal: f32, two_sided: bool, room:i32) -> vec3<f32> {
    var illumination = vec3<f32>(0.0);
    for (var i = 0u; i < 8u; i += 1u) {
        let source = u.hearths[i];
        if source.w <= 0.0 { break; }
        let delta = source.xyz-world;
        let distance_squared = dot(delta,delta);
        if distance_squared >= source.w*source.w { continue; }
        let distance = sqrt(max(distance_squared,0.0001));
        let light = delta/distance;
        let radius_fade = 1.0-smoothstep(source.w*0.45,source.w,distance);
        let power_scale = select(select(34.0,11.0,source.w<10.0),2.8,source.w<3.5);
        let intensity = power_scale*radius_fade*radius_fade*hearth_flicker(source.xyz)/(1.0+distance_squared);
        let warm = vec3<f32>(1.0,0.54,0.23)*intensity*room_fire_visibility(world,source.xyz,i32(u.hearth_rooms[i/4u][i%4u]),room);
        let nl = select(max(dot(normal,light),0.0),abs(dot(normal,light))*0.78+0.14,two_sided);
        let diffuse = albedo*(1.0-metal)*nl;
        let facing_normal = select(-normal,normal,dot(normal,view)>=0.0);
        let half_sum = light+view;
        let halfway = half_sum*inverseSqrt(max(dot(half_sum,half_sum),0.00001));
        let power = mix(112.0,7.0,clamp(roughness,0.0,1.0));
        let fresnel = mix(vec3<f32>(0.035),albedo,metal);
        let highlight = pow(max(dot(facing_normal,halfway),0.0),power)
            * (power+2.0)*0.125 * max(dot(facing_normal,light),0.0);
        let specular = fresnel*highlight*select(1.0,0.13,two_sided);
        illumination += (diffuse+specular)*warm;
    }
    return illumination;
}

fn flame_emission(pigment: vec3<f32>, emission: f32, world: vec3<f32>) -> vec3<f32> {
    // Flames themselves are independent of direct sky light and weather snow.
    // The dedicated texture's hot filaments drive HDR bloom without ever
    // interpreting canvas/banner material5 as a light source.
    return pow(max(pigment,vec3<f32>(0.0)),vec3<f32>(2.2))
        * (3.0+emission*12.0)*hearth_flicker(world);
}
