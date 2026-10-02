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
    // Choose current ecological ground, rather than a showcase coordinate that
    // can become water or another biome when the geography recipe changes.
    if id == "short-grass" || id == "lush-grass" {
        for z in (-90000i32..90000).step_by(2400) {
            for x in (-90000i32..90000).step_by(2400) {
                let s = world.sample(x as f32, z as f32);
                let biome = if id == "short-grass" {
                    fantasy_land::world::Biome::PineForest
                } else {
                    fantasy_land::world::Biome::Grassland
                };
                if s.ocean || s.biome != biome || s.road > 0. || s.water_height > s.height - 2. {
                    continue;
                }
                let r = fantasy_land::regions::sample(world.seed, x as f32, z as f32, &s);
                if r.slope > 0.08 {
                    continue;
                }
                let t = fantasy_land::cover::tile_data(world, x.div_euclid(48), z.div_euclid(48));
                let good = t
                    .meadow
                    .iter()
                    .filter(|c| {
                        if id == "short-grass" {
                            c.data[2] & 255 < 36
                        } else {
                            c.data[2] >> 8 > 220
                        }
                    })
                    .count();
                if good < 800 {
                    continue;
                }
                return Fixture {
                    id: id.into(),
                    eye: [
                        x as f32,
                        geometry::walk_height(world, x as f32, z as f32) + 1.72,
                        z as f32,
                    ],
                    yaw: 1.4,
                    pitch: -0.4,
                    hour: 15.,
                    weather: 1,
                };
            }
        }
        panic!("no ecological fixture found for {id}");
    }
    let (x, z, yaw, pitch, hour, weather) = match id {
        "forest-floor" => (-10879., 58547., 1.4, -0.40, 15., 1),
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
        "forest-floor",
        "short-grass",
        "lush-grass",
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

/// Same-camera weather contact sheet inputs; native GPU output, no image edits.
/// verify output/weather 1337 weather-studies [meadow|forest|inn]
pub fn weather_studies(seed: u32, dir: &str) {
    let id = std::env::args().nth(4).unwrap_or_else(|| "meadow".into());
    let world = World::new(seed);
    let mut f = fixture(&world, &id);
    f.hour = 11.0;
    let eye = Vec3::from_array(f.eye);
    let mut r = pollster::block_on(Renderer::headless(1280, 720)).unwrap();
    r.set_quality(1);
    r.set_render_resolution(720);
    r.set_antialiasing(1);
    r.set_ground_cover_density(4.0);
    r.set_lighting_mode(7);
    r.set_filter(1, 1.0);
    r.update_chunks(&world, eye, true);
    while r.pending_count() > 0 {
        r.update_chunks(&world, eye, false);
    }
    // Rooms, window glass and fire emitters are populated with nearby life.
    // Match the game loop even when the fixture camera has no visible actors.
    let mut life = Life::new();
    life.clock = f.hour as f64 * 120.0;
    life.update(&world, f.eye, 0.1);
    r.update_people(&world, &life, eye, f.yaw);
    let mut reports = Vec::new();
    for (name, mode) in [
        ("fair", 1),
        ("cloudy", 2),
        ("overcast", 8),
        ("rain", 3),
        ("storm", 4),
        ("tempest", 5),
        ("snow", 6),
        ("blizzard", 7),
    ] {
        r.set_weather_paused(false);
        r.set_weather_mode(mode);
        for _ in 0..800 {
            r.update_weather(&world, eye, f.hour, 0.05);
            r.advance_time(0.05);
        }
        while r.weather_state().lightning > 0.001 {
            r.update_weather(&world, eye, f.hour, 0.025);
            r.advance_time(0.025);
        }
        r.set_weather_paused(true);
        for _ in 0..24 {
            r.render(eye, f.yaw, f.pitch, f.hour).unwrap();
            r.device.poll(wgpu::PollType::Wait).unwrap();
        }
        png(&format!("{dir}/{name}.png"), &r.capture_rgba().unwrap());
        let mut completed = Vec::new();
        for _ in 0..30 {
            r.advance_time(1.0 / 60.0);
            let t = Instant::now();
            r.render(eye, f.yaw, f.pitch, f.hour).unwrap();
            r.device.poll(wgpu::PollType::Wait).unwrap();
            completed.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        reports.push(serde_json::json!({"name":name,"weather":r.weather_state(),"nativeCompleted":stats(&completed),"gpuPasses":r.gpu_timings()}));
        if mode == 5 {
            r.set_weather_paused(false);
            let mut found = false;
            for _ in 0..2400 {
                r.update_weather(&world, eye, f.hour, 1.0 / 60.0);
                r.advance_time(1.0 / 60.0);
                if r.weather_state().lightning > 0.85 {
                    found = true;
                    break;
                }
            }
            assert!(found, "tempest should produce a visible lightning stroke");
            r.render(eye, f.yaw, f.pitch, f.hour).unwrap();
            r.device.poll(wgpu::PollType::Wait).unwrap();
            png(&format!("{dir}/lightning.png"), &r.capture_rgba().unwrap());
        }
        println!("Weather captured: {name}");
    }
    fs::write(format!("{dir}/weather.json"),serde_json::to_string_pretty(&serde_json::json!({"fixture":f,"reports":reports,"note":"Native completed GPU frames, not browser FPS. Sequential fronts retain wetness/snow history."})).unwrap()).unwrap();
}

/// Capture leaders, return strokes and all three seeded discharge types.
pub fn lightning_studies(seed: u32, dir: &str) {
    let world = World::new(seed);
    let f = fixture(&world, "meadow");
    let eye = Vec3::from_array(f.eye);
    let mut r = pollster::block_on(Renderer::headless(1280, 720)).unwrap();
    r.set_quality(1);
    r.set_render_resolution(720);
    r.set_antialiasing(1);
    r.set_ground_cover_density(4.0);
    r.set_lighting_mode(7);
    r.set_filter(1, 1.0);
    r.set_weather_mode(5);
    for _ in 0..800 {
        r.update_weather(&world, eye, 11.0, 0.05);
        r.advance_time(0.05);
    }
    r.update_chunks(&world, eye, true);
    while r.pending_count() > 0 {
        r.update_chunks(&world, eye, false);
    }
    let mut life = Life::new();
    life.clock = 11.0 * 120.0;
    life.update(&world, f.eye, 0.1);
    r.update_people(&world, &life, eye, f.yaw);
    for _ in 0..24 {
        r.render(eye, f.yaw, 0.18, 11.0).unwrap();
        r.device.poll(wgpu::PollType::Wait).unwrap();
    }
    png(&format!("{dir}/tempest.png"), &r.capture_rgba().unwrap());
    let mut seen = [false; 3];
    let names = ["cloud-flash", "cloud-crawler", "ground-bolt"];
    let mut records = Vec::new();
    for _ in 0..(360 * 120) {
        r.update_weather(&world, eye, 11.0, 1.0 / 120.0);
        r.advance_time(1.0 / 120.0);
        let event = fantasy_land::weather::lightning_event(seed, r.weather_state().seconds as f64);
        let kind = if event[2] < 0.32 {
            0
        } else if event[2] < 0.62 {
            1
        } else {
            2
        };
        if seen[kind] || event[1] < 0.005 || event[1] > 0.02 || r.weather_state().lightning < 0.01 {
            continue;
        }
        seen[kind] = true;
        for (step, age) in [0.015, 0.045, 0.10, 0.36, 0.50].into_iter().enumerate() {
            while fantasy_land::weather::lightning_event(seed, r.weather_state().seconds as f64)[1]
                < age
            {
                r.update_weather(&world, eye, 11.0, 1.0 / 240.0);
                r.advance_time(1.0 / 240.0);
            }
            r.render(eye, f.yaw, 0.18, 11.0).unwrap();
            r.device.poll(wgpu::PollType::Wait).unwrap();
            png(
                &format!("{dir}/{}-{step}.png", names[kind]),
                &r.capture_rgba().unwrap(),
            );
            records.push(serde_json::json!({"type":names[kind],"stage":step,"event":fantasy_land::weather::lightning_event(seed,r.weather_state().seconds as f64),"flash":r.weather_state().lightning}));
        }
        println!("Captured {}", names[kind]);
        if seen.into_iter().all(|s| s) {
            break;
        }
    }
    assert!(
        seen.into_iter().all(|s| s),
        "all discharge families must appear"
    );
    fs::write(
        format!("{dir}/lightning.json"),
        serde_json::to_string_pretty(&records).unwrap(),
    )
    .unwrap();
}

pub fn regional_studies(seed: u32, dir: &str) {
    let w = World::new(seed);
    let scenes = fantasy_land::regional_tour::destinations(&w);
    fs::write(
        format!("{dir}/destinations.json"),
        serde_json::to_string_pretty(&scenes).unwrap(),
    )
    .unwrap();
    let mut r = pollster::block_on(Renderer::headless(1280, 720)).unwrap();
    r.set_quality(1);
    r.set_antialiasing(1);
    r.set_filter(1, 0.85);
    r.set_ground_cover_density(4.);
    r.set_lighting_mode(7);
    for d in &scenes {
        let eye = Vec3::new(d.x, geometry::walk_height(&w, d.x, d.z) + 1.72, d.z);
        r.clear_chunks();
        r.set_weather_mode(1);
        for _ in 0..30 {
            r.update_weather(&w, eye, 9., 1.);
        }
        r.update_chunks(&w, eye, true);
        while r.pending_count() > 0 {
            r.update_chunks(&w, eye, false);
        }
        for _ in 0..90 {
            r.advance_time(1. / 60.);
            r.render(eye, d.yaw, d.pitch, 9.).unwrap();
            r.device.poll(wgpu::PollType::Wait).unwrap();
        }
        png(&format!("{dir}/{}.png", d.name), &r.capture_rgba().unwrap());
        println!("captured {} at {},{}", d.name, d.x, d.z);
    }
    for (i, name) in ["landscape", "elevation", "geology"]
        .into_iter()
        .enumerate()
    {
        let pixels = w.map_layer_rgba(0., 0., 384000., 768, i as u32);
        let mut e = png::Encoder::new(
            BufWriter::new(File::create(format!("{dir}/{name}.png")).unwrap()),
            768,
            768,
        );
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
}
