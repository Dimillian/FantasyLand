//! Reproducible native visual fixtures. The same harness is copied into the
//! preserved main source; the configure callback is the only variant hook.
use fantasy_land::{
    citizens::Life,
    geometry,
    renderer::Renderer,
    settlements::{dist, Kind, Use},
    world::World,
};
use glam::Vec3;
use serde::Serialize;
use std::{fs, fs::File, io::BufWriter, time::Instant};
#[derive(Clone, Serialize)]
struct Fixture {
    id: String,
    eye: [f32; 3],
    yaw: f32,
    pitch: f32,
    hour: f32,
    weather: u32,
}
fn fixture(world: &World, id: &str) -> Fixture {
    let (x, z, yaw, pitch, hour, weather) = match id {
        "forest" => (-10879., 58547., 1.4, 0.08, 7.5, 1),
        "meadow" => (87851., 55519., 1.1780972, -0.035, 9.5, 1),
        "rocks" => (23512.3, 63468.41, -1.9067289, 0.27, 16., 1),
        "wet" => (87851., 55519., 1.1780972, -0.035, 11., 3),
        _ => {
            let e = world
                .settlements
                .entries
                .iter()
                .filter(|e| e.kind == Kind::City)
                .min_by(|a, b| {
                    dist([a.site.x, a.site.z], [0., 0.])
                        .total_cmp(&dist([b.site.x, b.site.z], [0., 0.]))
                })
                .unwrap();
            let l = world.settlements.layout(world, e.site.id).unwrap();
            if id == "capital" {
                return Fixture {
                    id: id.into(),
                    eye: [
                        e.site.x,
                        geometry::walk_height(world, e.site.x, e.site.z) + 1.72,
                        e.site.z,
                    ],
                    yaw: 0.75,
                    pitch: 0.05,
                    hour: 9.,
                    weather: 1,
                };
            }
            let inn = l.buildings.iter().find(|b| b.usage == Use::Inn).unwrap();
            let eye = inn.point(0., 1.72, -inn.half[1] + 2.2);
            return Fixture {
                id: id.into(),
                eye,
                yaw: std::f32::consts::PI - inn.yaw,
                pitch: 0.,
                hour: if id == "inn-night" { 22. } else { 11. },
                weather: 1,
            };
        }
    };
    Fixture {
        id: id.into(),
        eye: [x, geometry::walk_height(world, x, z) + 1.72, z],
        yaw,
        pitch,
        hour,
        weather,
    }
}
fn png(path: &str, pixels: &[u8]) {
    let mut e = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), 1280, 720);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(pixels).unwrap();
}
fn stats(v: &[f64]) -> serde_json::Value {
    if v.is_empty() {
        return serde_json::Value::Null;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    serde_json::json!({"meanMs":s.iter().sum::<f64>()/s.len() as f64,"medianMs":s[s.len()/2],"p95Ms":s[s.len()*95/100],"p99Ms":s[s.len()*99/100],"hitches":s.iter().filter(|&&t|t>33.4).count(),"frames":s.len()})
}
pub fn run(seed: u32, dir: &str, configure: impl Fn(&mut Renderer, u32)) {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(4).map(|s| s.as_str()).unwrap_or("baseline");
    let selected = args.get(5).map(|s| s.as_str()).unwrap_or("all");
    let frames: usize = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);
    let resolution: u32 = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(720);
    let mask = match mode {
        "ao" => 1,
        "shadows" => 3,
        "full" => 7,
        _ => 0,
    };
    let scenes: Vec<_> = [
        "forest",
        "capital",
        "inn-day",
        "inn-night",
        "meadow",
        "rocks",
        "wet",
    ]
    .into_iter()
    .filter(|id| selected == "all" || *id == selected)
    .collect();
    assert!(!scenes.is_empty(), "unknown scene");
    for id in scenes {
        let world = World::new(seed);
        let f = fixture(&world, id);
        let eye = Vec3::from_array(f.eye);
        let mut r = pollster::block_on(Renderer::headless(1280, 720)).unwrap();
        r.set_quality(1);
        r.set_render_resolution(resolution);
        r.set_antialiasing(1);
        r.set_filter(1, 0.85);
        r.set_ground_cover_density(4.);
        configure(&mut r, mask);
        r.set_weather_mode(f.weather);
        r.update_weather(&world, eye, f.hour, 0.);
        for _ in 0..30 {
            r.update_weather(&world, eye, f.hour, 1.);
        }
        r.set_weather_paused(true);
        r.update_chunks(&world, eye, true);
        let mut drain = 0;
        while r.pending_count() > 0 {
            r.update_chunks(&world, eye, false);
            drain += 1;
            assert!(drain < 100000, "streaming failed to drain");
        }
        let mut life = Life::new();
        life.clock = f.hour as f64 * 120.;
        life.update(&world, f.eye, 0.1);
        r.update_people(&world, &life, eye, f.yaw);
        for _ in 0..240 {
            r.render(eye, f.yaw, f.pitch, f.hour).unwrap();
            r.device.poll(wgpu::PollType::Wait).unwrap();
        }
        png(&format!("{dir}/{id}.png"), &r.capture_rgba().unwrap());
        let mut submitted = Vec::new();
        let mut completed = Vec::new();
        let mut gpu = Vec::new();
        for _ in 0..frames {
            let t = Instant::now();
            r.render(eye, f.yaw, f.pitch, f.hour).unwrap();
            submitted.push(t.elapsed().as_secs_f64() * 1000.);
            r.device.poll(wgpu::PollType::Wait).unwrap();
            completed.push(t.elapsed().as_secs_f64() * 1000.);
            if let Some(t) = r.gpu_render_ms() {
                gpu.push(t as f64);
            }
        }
        let report = serde_json::json!({"fixture":f,"seed":seed,"variant":mode,"mask":mask,"output":[1280,720],"internal":r.render_resolution(),"quality":1,"coverDensity":4,"antialiasing":"FXAA","bloom":0.85,"weatherWarmupSeconds":30,"frozenAnimationSeconds":0,"warmupFrames":240,"people":life.actors.len(),"triangles":r.triangle_count(),"meshBytes":(r.mesh_bytes()),"lightingBytes":r.lighting_bytes(),"lastGpuPasses":r.gpu_timings(),"nativeSubmission":stats(&submitted),"nativeCompleted":stats(&completed),"gpuSpan":stats(&gpu),"rawSubmission":submitted,"rawCompleted":completed,"rawGpu":gpu,"measurement":"Native wgpu completed frames. Not browser FPS. GPU samples are asynchronous and may repeat."});
        fs::write(
            format!("{dir}/{id}.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("fixture {id}: captured {mode} at {resolution}p");
    }
}

/// Fixed-camera sequences demonstrate wind on real foliage, with no camera bob.
/// Invocation: verify output/wind 1337 wind-motion [forest|meadow] [weather 1..8].
pub fn wind_motion(seed: u32, dir: &str) {
    let args: Vec<_> = std::env::args().collect();
    let id = args.get(4).map(String::as_str).unwrap_or("forest");
    assert!(["forest", "meadow"].contains(&id));
    let mode = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(2);
    let world = World::new(seed);
    let mut f = fixture(&world, id);
    f.hour = 11.;
    f.pitch = if id == "meadow" { -0.12 } else { 0.14 };
    let eye = Vec3::from_array(f.eye);
    let mut r = pollster::block_on(Renderer::headless(1280, 720)).unwrap();
    r.set_quality(1);
    r.set_render_resolution(720);
    r.set_antialiasing(1);
    r.set_ground_cover_density(4.);
    r.set_lighting_mode(7);
    r.set_filter(1, 1.);
    r.set_weather_mode(mode);
    for _ in 0..600 {
        r.update_weather(&world, eye, f.hour, 0.05);
        r.advance_time(0.05);
    }
    r.set_weather_paused(true);
    r.update_chunks(&world, eye, true);
    while r.pending_count() > 0 {
        r.update_chunks(&world, eye, false);
    }
    for _ in 0..120 {
        r.render(eye, f.yaw, f.pitch, f.hour).unwrap();
        r.device.poll(wgpu::PollType::Wait).unwrap();
    }
    let mut completed = Vec::new();
    for frame in 0..96 {
        for _ in 0..5 {
            r.advance_time(1. / 60.);
        }
        let t = Instant::now();
        r.render(eye, f.yaw, f.pitch, f.hour).unwrap();
        r.device.poll(wgpu::PollType::Wait).unwrap();
        completed.push(t.elapsed().as_secs_f64() * 1000.);
        png(
            &format!("{dir}/frame-{frame:03}.png"),
            &r.capture_rgba().unwrap(),
        );
    }
    fs::write(format!("{dir}/motion.json"),serde_json::to_string_pretty(&serde_json::json!({"scene":id,"weather":r.weather_state(),"frames":96,"fps":12,"seconds":8,"eye":f.eye,"yaw":f.yaw,"pitch":f.pitch,"lighting":7,"nativeCompleted":stats(&completed),"note":"Fixed camera, 720p, 400% cover. Capture pacing is not browser FPS."})).unwrap()).unwrap();
    println!("Wind motion captured: {id}, weather {mode}, 8 seconds");
}
