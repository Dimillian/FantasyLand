#[cfg(not(target_arch = "wasm32"))]
mod render_checks;
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use fantasy_land::{
        geometry,
        renderer::Renderer,
        world::{World, WORLD_SIZE},
    };
    use std::{
        fs::{self, File},
        io::BufWriter,
        time::Instant,
    };
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "output/verification".into());
    fs::create_dir_all(&dir).unwrap();
    let seed = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1337);
    let check = std::env::args().nth(3);
    if check.as_deref() == Some("weather-studies") {
        render_checks::weather_studies(seed, &dir);
        return;
    }
    if check.as_deref() == Some("wind-motion") {
        render_checks::wind_motion(seed, &dir);
        return;
    }
    if check.as_deref() == Some("render-compare") {
        render_checks::run(seed, &dir, |r, mask| r.set_lighting_mode(mask));
        return;
    }
    let filters_only = check.as_deref() == Some("filters");
    let grounding_only = check.as_deref() == Some("grounding");
    let roads_only = check.as_deref() == Some("roads");
    let coasts_only = check.as_deref() == Some("coasts");
    let cover_only = check.as_deref() == Some("cover");
    let regions_only = check.as_deref() == Some("regions");
    let vista_only = check.as_deref() == Some("vista");
    let foliage_only = check.as_deref() == Some("foliage");
    let forests_only = check.as_deref() == Some("forests");
    let geography_only = check.as_deref() == Some("geography");
    let generation_time = Instant::now();
    let world = World::new(seed);
    println!(
        "Hydrology generation {:?}: {:?}",
        generation_time.elapsed(),
        world.hydrology_stats()
    );
    if check.as_deref() == Some("settlement-data") {
        use fantasy_land::settlements::{dist, segment_rect, Kind};
        let mut summaries = vec![];
        for kind in [
            Kind::Camp,
            Kind::Hamlet,
            Kind::Fort,
            Kind::Village,
            Kind::Town,
            Kind::City,
        ] {
            let e = world
                .settlements
                .entries
                .iter()
                .filter(|e| e.kind == kind)
                .min_by(|a, b| {
                    dist([a.site.x, a.site.z], [0., 0.])
                        .total_cmp(&dist([b.site.x, b.site.z], [0., 0.]))
                })
                .unwrap();
            let start = Instant::now();
            let l = world.settlements.layout(&world, e.site.id).unwrap();
            let mut crossings = vec![];
            let mut wet = 0;
            for (a, links) in l.links.iter().enumerate() {
                for &b in links {
                    if b < a {
                        continue;
                    }
                    let pa = l.nodes[a];
                    let pb = l.nodes[b];
                    for building in &l.buildings {
                        if (a == building.room_node && b == building.porch_node)
                            || (b == building.room_node && a == building.porch_node)
                        {
                            continue;
                        }
                        if segment_rect(
                            building.local(pa[0], pa[2]),
                            building.local(pb[0], pb[2]),
                            [building.half[0] + 0.3, building.half[1] + 0.3],
                        ) {
                            crossings.push((a, b, building.id));
                        }
                    }
                    for t in [0., 0.25, 0.5, 0.75, 1.] {
                        let p = [pa[0] + (pb[0] - pa[0]) * t, pa[2] + (pb[2] - pa[2]) * t];
                        let ground = world.natural_sample(p[0], p[1]);
                        if ground.ocean || ground.height < ground.water_height {
                            wet += 1;
                        }
                    }
                }
            }
            let data = serde_json::json!({"kind":kind,"id":e.site.id,"name":e.site.name,"x":e.site.x,"z":e.site.z,"buildings":l.buildings.len(),"population":l.population,"radius":l.entry.radius,"generationMs":start.elapsed().as_millis(),"crossingCount":crossings.len(),"crossings":crossings.iter().take(12).collect::<Vec<_>>(),"wet":wet});
            println!("{data}");
            summaries.push(data);
            fs::write(
                format!("{dir}/{}.json", kind.name()),
                serde_json::to_string(&*l).unwrap(),
            )
            .unwrap();
        }
        let mut counts = std::collections::BTreeMap::new();
        for e in &world.settlements.entries {
            *counts.entry(e.kind.name()).or_insert(0) += 1;
        }
        fs::write(
            format!("{dir}/settlements.json"),
            serde_json::to_string_pretty(&serde_json::json!({"counts":counts,"samples":summaries}))
                .unwrap(),
        )
        .unwrap();
        return;
    }
    if check.as_deref() == Some("map-relief") {
        let cx = std::env::args()
            .nth(4)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.);
        let cz = std::env::args()
            .nth(5)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.);
        for (name, span) in [
            ("detail", 16000.),
            ("region", 42000.),
            ("continent", WORLD_SIZE),
        ] {
            let (x, z) = if name == "continent" {
                (0., 0.)
            } else {
                (cx, cz)
            };
            save_png(
                &format!("{dir}/{name}.png"),
                768,
                768,
                &world.map_background_rgba(x, z, span, 768),
            );
        }
        return;
    }
    if check.as_deref() == Some("atlas") {
        let mut biomes = std::collections::BTreeMap::new();
        let (mut land, mut wet, mut alpine, mut high, mut peak) = (0u32, 0u32, 0u32, 0u32, 0f32);
        let n = 512u32;
        let mut map = Vec::new();
        for iz in 0..n {
            for ix in 0..n {
                let x = (ix as f32 + 0.5) / n as f32 * WORLD_SIZE - WORLD_SIZE * 0.5;
                let z = (iz as f32 + 0.5) / n as f32 * WORLD_SIZE - WORLD_SIZE * 0.5;
                let t = world.natural_sample(x, z);
                let color = if t.ocean {
                    [0.055, 0.17, 0.26]
                } else {
                    land += 1;
                    *biomes.entry(t.biome.name()).or_insert(0u32) += 1;
                    peak = peak.max(t.height);
                    high += u32::from(t.height > 2000.);
                    alpine += u32::from(t.height > 3000.);
                    if t.water_height > t.height {
                        wet += 1;
                        [0.16, 0.48, 0.64]
                    } else {
                        let light = (0.80 + t.height / 6500.).min(1.3);
                        t.biome.color().map(|v| v * light)
                    }
                };
                map.extend(color.map(|v| (v.clamp(0., 1.) * 255.) as u8));
                map.push(255);
            }
        }
        save_png(&format!("{dir}/climate-atlas.png"), n, n, &map);
        let data = serde_json::json!({"seed":seed,"landSamples":land,"inlandWaterPercent":100.*wet as f32/land as f32,"above2000Percent":100.*high as f32/land as f32,"above3000Percent":100.*alpine as f32/land as f32,"sampledPeakM":peak,"biomes":biomes,"hydrology":world.hydrology_stats(),"lakes":world.lakes()});
        fs::write(
            format!("{dir}/atlas.json"),
            serde_json::to_string_pretty(&data).unwrap(),
        )
        .unwrap();
        println!("{data}");
        return;
    }
    if check.as_deref() == Some("mountains") {
        let start = Instant::now();
        let trails = world.mountain_trails();
        let summary:Vec<_>=trails.iter().map(|r| {
            let a=r.points[0];let b=*r.points.last().unwrap();
            serde_json::json!({"id":r.id,"start":a,"end":b,"gain":world.natural_sample(b[0],b[1]).height-world.natural_sample(a[0],a[1]).height,"length":fantasy_land::traversal::length(&r.points),"points":r.points})
        }).collect();
        let data = serde_json::json!({"seed":seed,"seconds":start.elapsed().as_secs_f32(),"trails":summary});
        fs::write(
            format!("{dir}/mountain-trails.json"),
            serde_json::to_string_pretty(&data).unwrap(),
        )
        .unwrap();
        println!("{data}");
        return;
    }
    let (spawn, yaw) = world.spawn_view();
    println!(
        "World: {} x {} km, seed {}, spawn {:?}",
        WORLD_SIZE / 1000.,
        WORLD_SIZE / 1000.,
        world.seed,
        spawn
    );
    if check.as_deref() != Some("materials")
        && check.as_deref() != Some("fluid")
        && !filters_only
        && !grounding_only
        && !roads_only
        && !coasts_only
        && !cover_only
        && !regions_only
        && !vista_only
        && !foliage_only
        && !forests_only
        && !geography_only
    {
        let map_time = Instant::now();
        let map = world.map_rgba(0., 0., WORLD_SIZE, 512);
        save_png(&format!("{dir}/world-map.png"), 512, 512, &map);
        println!("512px world map: {:?}", map_time.elapsed());
        let local = world.map_rgba(spawn[0], spawn[1], 6000., 512);
        save_png(&format!("{dir}/local-map.png"), 512, 512, &local);
    }
    let mut renderer =
        pollster::block_on(Renderer::headless(1280, 720)).expect("create native wgpu renderer");
    if matches!(
        check.as_deref(),
        Some("settlement-scenes" | "settlement-art" | "town-flow")
    ) {
        use fantasy_land::settlements::{dist, Kind, Use};
        let mut life = fantasy_land::citizens::Life::new();
        renderer.set_quality(if check.as_deref() == Some("town-flow") {
            1
        } else {
            0
        });
        renderer.set_render_resolution(if check.as_deref() == Some("town-flow") {
            720
        } else {
            540
        });
        renderer.set_weather_mode(1);
        renderer.set_antialiasing(1);
        let chosen: Vec<_> = world
            .settlements
            .entries
            .iter()
            .filter(|e| e.kind == Kind::City)
            .collect();
        let e = chosen
            .iter()
            .min_by(|a, b| {
                dist([a.site.x, a.site.z], [0., 0.])
                    .total_cmp(&dist([b.site.x, b.site.z], [0., 0.]))
            })
            .unwrap();
        let l = world.settlements.layout(&world, e.site.id).unwrap();
        let inn = l.buildings.iter().find(|b| b.usage == Use::Inn).unwrap();
        let mut scenes = vec![
            (
                "capital",
                [e.site.x, world.height(e.site.x, e.site.z) + 1.72, e.site.z],
                0.75,
                0.05,
                9.,
            ),
            (
                "inn-day",
                inn.point(0., 1.72, -inn.half[1] + 2.2),
                std::f32::consts::PI - inn.yaw,
                0.,
                11.,
            ),
            (
                "inn-night",
                inn.point(0., 1.72, -inn.half[1] + 2.2),
                std::f32::consts::PI - inn.yaw,
                0.,
                22.,
            ),
        ];
        if check.as_deref() == Some("town-flow") {
            let home = l.buildings.iter().find(|b| b.usage == Use::Home).unwrap();
            let p = home.point(-home.half[0] - 5., 2.0, -home.half[1] * 0.75);
            let target = home.point(-home.half[0], 2.7, home.window_z(-1.));
            let dx = target[0] - p[0];
            let dz = target[2] - p[2];
            scenes.push((
                "window-and-chimney",
                p,
                dx.atan2(-dz),
                -(target[1] - p[1]).atan2(dx.hypot(dz)),
                11.,
            ));
            let p = home.point(0., 1.65, -home.half[1] + 1.5);
            let target = home.point(-home.half[0] + 0.7, 1.5, home.hearth_z());
            scenes.push((
                "home-layout",
                p,
                (target[0] - p[0]).atan2(-(target[2] - p[2])),
                0.0,
                15.,
            ));
            let street = l
                .streets
                .iter()
                .filter(|s| s.width > 2.5)
                .max_by(|a, b| {
                    dist(a.points[0], [e.site.x, e.site.z])
                        .total_cmp(&dist(b.points[0], [e.site.x, e.site.z]))
                })
                .unwrap();
            let a = street.points[0];
            let b = street.points[1];
            scenes.push((
                "neighborhood-lane",
                [
                    a[0],
                    fantasy_land::geometry::walk_height(&world, a[0], a[1]) + 1.72,
                    a[1],
                ],
                (b[0] - a[0]).atan2(-(b[1] - a[1])),
                0.10,
                10.,
            ));
        }
        if check.as_deref() == Some("settlement-art") {
            let look = |a: [f32; 3], b: [f32; 3]| {
                let d = glam::Vec3::from_array(b) - glam::Vec3::from_array(a);
                (d.x.atan2(-d.z), (d.y / d.length()).asin())
            };
            for (material, name) in [(0, "brick-house"), (1, "timber-house"), (2, "stone-house")] {
                if let Some(home) = l
                    .buildings
                    .iter()
                    .find(|b| b.usage == Use::Home && b.id % 5 == material)
                {
                    let p = home.point(home.half[0] * 0.9, 1.9, -home.half[1] - 9.0);
                    let (yaw, pitch) = look(p, home.point(0., 2.4, 0.));
                    scenes.push((name, p, yaw, pitch, 9.));
                }
            }
            let p = inn.point(0., 1.55, inn.half[1] * 0.47);
            let (yaw, pitch) = look(p, inn.point(3.5, 0.8, inn.half[1] - 1.2));
            scenes.push(("bedroom", p, yaw, pitch, 10.));
            let sun = fantasy_land::celestial::state(7.6).sun;
            let home = l
                .buildings
                .iter()
                .filter(|b| b.usage == Use::Home)
                .max_by(|a, b| {
                    let score = |b: &fantasy_land::settlements::Building| {
                        (sun.x * b.yaw.cos() - sun.z * b.yaw.sin()).abs()
                    };
                    score(a).total_cmp(&score(b))
                })
                .unwrap();
            let side = (sun.x * home.yaw.cos() - sun.z * home.yaw.sin()).signum();
            let p = home.point(-side * 0.8, 1.45, -home.half[1] + 1.6);
            let (yaw, pitch) = look(p, home.point(side * home.half[0], 1.9, 0.));
            scenes.push(("window-light", p, yaw, pitch, 7.6));
            let p = inn.point(0., 1.67, -inn.half[1] + 2.3);
            let (yaw, pitch) = look(p, inn.point(-inn.half[0] + 1.5, 1.25, -inn.half[1] + 3.2));
            scenes.push(("tavern-bar", p, yaw, pitch, 20.));
            let p = inn.point(-0.2, 1.67, 0.0);
            let (yaw, pitch) = look(p, inn.point(inn.half[0] - 1.6, 0.85, -inn.half[1] + 2.5));
            scenes.push(("tavern-table", p, yaw, pitch, 20.));
            for (usage, name) in [
                (Use::Arcane, "arcanist-study"),
                (Use::Smithy, "smithy"),
                (Use::Temple, "temple"),
                (Use::Home, "home-evening"),
            ] {
                if let Some(b) = l.buildings.iter().find(|b| b.usage == usage) {
                    let p = b.point(0., 1.62, -b.half[1] + 1.0);
                    let (yaw, pitch) = look(p, b.point(-0.5, 1.1, 0.));
                    scenes.push((name, p, yaw, pitch, 19.5));
                }
            }
            let (fw, fh) = (
                fantasy_land::people_sprites::FRAME_W,
                fantasy_land::people_sprites::FRAME_H,
            );
            let (sw, sh) = (fw * 16, fh * 4);
            let mut sheet = vec![0u8; sw * sh * 4];
            for variant in 0..4 {
                for role in 0..16 {
                    let plate =
                        fantasy_land::people_sprites::preview(role as u32, variant as u32, 0, 0);
                    for y in 0..fh {
                        for x in 0..fw {
                            let src = (y * fw + x) * 4;
                            let dst = ((variant * fh + y) * sw + role * fw + x) * 4;
                            sheet[dst..dst + 4].copy_from_slice(&plate[src..src + 4]);
                        }
                    }
                }
            }
            save_png(
                &format!("{dir}/citizen-sprites.png"),
                sw as u32,
                sh as u32,
                &sheet,
            );
        }
        for (name, p, yaw, pitch, hour) in scenes {
            let eye = glam::Vec3::from_array(p);
            renderer.clear_chunks();
            renderer.update_chunks(&world, eye, true);
            while renderer.pending_count() > 0 {
                renderer.update_chunks(&world, eye, false);
            }
            life.clock = hour as f64 * 120.;
            life.invalidate();
            life.update(&world, p, 0.1);
            renderer.update_weather(&world, eye, hour, 0.);
            renderer.update_people(&world, &life, eye, yaw);
            for _ in 0..if check.as_deref() == Some("town-flow") {
                100
            } else {
                4
            } {
                renderer.advance_time(1. / 60.);
                renderer.render(eye, yaw, pitch, hour).unwrap();
                renderer.device.poll(wgpu::PollType::Wait).unwrap();
            }
            save_png(
                &format!("{dir}/{name}.png"),
                1280,
                720,
                &renderer.capture_rgba().unwrap(),
            );
            println!("captured {name}: {} people", life.actors.len());
            let mut cpu = Vec::new();
            for _ in 0..240 {
                let start = Instant::now();
                life.update(&world, p, 1. / 60.);
                let mesh = life.mesh(&world, p, yaw);
                std::hint::black_box(mesh);
                cpu.push(start.elapsed().as_secs_f64() * 1000.);
            }
            cpu.sort_by(f64::total_cmp);
            let report = serde_json::json!({"scene":name,"platform":"native Metal, CPU simulation/mesh only; not browser FPS","people":life.actors.len(),"frames":cpu.len(),"meanMs":cpu.iter().sum::<f64>()/cpu.len() as f64,"p95Ms":cpu[cpu.len()*95/100],"p99Ms":cpu[cpu.len()*99/100]});
            fs::write(
                format!("{dir}/{name}-cpu.json"),
                serde_json::to_string_pretty(&report).unwrap(),
            )
            .unwrap();
            println!("{report}");
        }
        return;
    }

    if check.as_deref() == Some("materials") || foliage_only || forests_only || geography_only {
        renderer.set_quality(if foliage_only { 2 } else { 1 });
        renderer.set_render_resolution(720);
        renderer.set_ground_cover_density(4.0);
        renderer.set_filter(1, 1.0);
        if foliage_only || forests_only {
            renderer.set_antialiasing(1);
        }
        let mut reports = Vec::new();
        let mut scenes = vec![
            (
                "orins-woodland",
                109764.,
                -15885.,
                -0.9424778,
                0.04,
                12.0,
                1,
            ),
            ("amberwood", -10879., 58547., 1.4, 0.08, 7.5, 1),
            (
                "silverwater",
                -8909.148,
                -77660.938,
                -0.2618,
                -0.0438,
                9.3,
                1,
            ),
            ("sunstone", 23512.3, 63468.41, -1.9067289, 0.27, 16.0, 1),
            (
                "hearth",
                -16211.261,
                -12684.719,
                -1.4056476,
                -0.08339161,
                22.0,
                1,
            ),
            ("moonwater", -8909.148, -77660.938, -0.2618, 0.12, 23.0, 1),
        ];
        if forests_only {
            // Select actual forests from the seed, without changing their
            // species, planting density, terrain or rendering settings.
            let mut found = std::collections::BTreeMap::new();
            for z in -55..55 {
                for x in -55..55 {
                    let px = x as f32 * 2400. + 97.;
                    let pz = z as f32 * 2400. + 173.;
                    let s = world.sample(px, pz);
                    if s.ocean || s.road > 0.01 || s.water_height > s.height - 3. {
                        continue;
                    }
                    let e = fantasy_land::ecology::sample(seed, px, pz, &s);
                    if e.tree_density < 0.74 {
                        continue;
                    }
                    let region = fantasy_land::regions::sample(seed, px, pz, &s);
                    if region.slope > 0.32 || geometry::blocks_player(&world, px, pz) {
                        continue;
                    }
                    let stand = fantasy_land::ecology::forest_in(seed, px, pz, &s, &region);
                    let score = e.ferns + e.shrubs + e.tree_density * 0.4;
                    let entry = found.entry(stand.kind.name()).or_insert((0., px, pz));
                    if score > entry.0 {
                        *entry = (score, px, pz);
                    }
                }
            }
            scenes.clear();
            for (name, (_, x, z)) in found {
                let view = fantasy_land::exploration::forest_view(
                    &world,
                    fantasy_land::exploration::Destination {
                        name,
                        x,
                        z,
                        yaw: 1.4,
                        pitch: 0.12,
                    },
                );
                scenes.push((name, view.x, view.z, view.yaw, view.pitch, 8.2, 1));
            }
            println!("Forest capture locations: {scenes:?}");
            fs::write(
                format!("{dir}/locations.json"),
                serde_json::to_string_pretty(&scenes).unwrap(),
            )
            .unwrap();
            scenes.push(("orins-woodland", 109764., -15885., -0.9424778, 0.04, 12., 1));
        }
        if geography_only {
            let destinations = fantasy_land::exploration::destinations(&world);
            fs::write(
                format!("{dir}/destinations.json"),
                serde_json::to_string_pretty(&destinations).unwrap(),
            )
            .unwrap();
            let wanted = [
                "Lowland swamp",
                "Golden savanna",
                "Tropical rainforest",
                "Tropical coast",
                "Desert & badlands",
                "Frozen mountain tarn",
                "Alpine highlands",
                "Grassland",
                "Coastal palm grove",
                "Umbrella acacia savanna",
                "Alpine heights",
                "Mountain ascent · I",
            ];
            scenes = destinations
                .iter()
                .filter(|d| wanted.contains(&d.name))
                .map(|d| {
                    let (hour, weather) = match d.name {
                        "Lowland swamp" => (8.5, 2),
                        "Tropical rainforest" => (8.0, 1),
                        "Frozen mountain tarn" => (10.0, 1),
                        "Alpine highlands" => (15.5, 6),
                        _ => (9.0, 1),
                    };
                    (d.name, d.x, d.z, d.yaw, d.pitch, hour, weather)
                })
                .collect();
            renderer.set_antialiasing(1);
        }
        for (name, x, z, yaw, pitch, hour, weather) in scenes {
            if foliage_only && name != "orins-woodland" {
                continue;
            }
            if !foliage_only && !forests_only && name == "orins-woodland" {
                continue;
            }
            renderer.set_quality(if foliage_only || name == "orins-woodland" {
                2
            } else {
                1
            });
            let eye = glam::Vec3::new(x, geometry::walk_height(&world, x, z) + 1.72, z);
            renderer.clear_chunks();
            renderer.set_weather_mode(weather);
            for _ in 0..90 {
                renderer.update_weather(&world, eye, hour, 1.0);
            }
            renderer.update_chunks(&world, eye - glam::Vec3::Y * 1.72, true);
            while renderer.pending_count() > 0 {
                renderer.update_chunks(&world, eye - glam::Vec3::Y * 1.72, false);
            }
            let mut times = Vec::new();
            for _ in 0..16 {
                let start = Instant::now();
                renderer.advance_time(1.0 / 60.0);
                renderer.render(eye, yaw, pitch, hour).unwrap();
                renderer.device.poll(wgpu::PollType::Wait).unwrap();
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            save_png(
                &format!("{dir}/{name}.png"),
                1280,
                720,
                &renderer.capture_rgba().unwrap(),
            );
            let report = serde_json::json!({"name":name,"eye":eye.to_array(),"yaw":yaw,"pitch":pitch,"hour":hour,"completedNativeMs":times[4..].iter().sum::<f64>()/12.0,"cover":renderer.cover_stats(),"meshBytes":renderer.mesh_bytes()});
            println!("{report}");
            reports.push(report);
        }
        fs::write(
            format!("{dir}/materials-report.json"),
            serde_json::to_string_pretty(&reports).unwrap(),
        )
        .unwrap();
        return;
    }
    if check.as_deref() == Some("fluid") {
        let probe =
            fantasy_land::water_sim::WaterSim::verify_gpu(&renderer.device, &renderer.queue)
                .expect("physical GPU water probes");
        fs::write(
            format!("{dir}/fluid-probe.json"),
            serde_json::to_string_pretty(&probe).unwrap(),
        )
        .unwrap();
        println!("Physical GPU water: {probe:?}");
        let x = -8909.148;
        let z = -77660.938;
        let eye = glam::Vec3::new(x, geometry::walk_height(&world, x, z) + 1.72, z);
        let yaw = -0.2618038;
        renderer.set_render_resolution(450);
        renderer.set_weather_mode(1);
        renderer.update_weather(&world, eye, 10., 0.);
        for _ in 0..120 {
            renderer.update_weather(&world, eye, 10., 0.1);
        }
        renderer.update_chunks(&world, eye - glam::Vec3::Y * 1.72, true);
        while renderer.pending_count() > 0 {
            renderer.update_chunks(&world, eye - glam::Vec3::Y * 1.72, false);
        }
        let mut reports = Vec::new();
        for (mode, label) in [(1, "clear"), (3, "rain"), (5, "tempest")] {
            renderer.set_weather_mode(mode);
            for _ in 0..120 {
                renderer.update_weather(&world, eye, 10., 0.1);
            }
            let mut frames = Vec::new();
            for i in 0..180 {
                renderer.advance_time(1.0 / 30.0);
                renderer.update_weather(&world, eye, 10., 1.0 / 30.0);
                let start = Instant::now();
                renderer.render(eye, yaw, -0.0438113, 10.).unwrap();
                renderer.device.poll(wgpu::PollType::Wait).unwrap();
                if i >= 30 {
                    frames.push(start.elapsed().as_secs_f64() * 1000.);
                }
                if i % 30 == 0 {
                    save_png(
                        &format!("{dir}/{label}-{:02}.png", i / 30),
                        1280,
                        720,
                        &renderer.capture_rgba().unwrap(),
                    );
                }
            }
            let frozen = renderer.capture_rgba().unwrap();
            renderer.render(eye, yaw, -0.0438113, 10.).unwrap();
            assert_pixels_equal(
                &frozen,
                &renderer.capture_rgba().unwrap(),
                "rendering alone must not advance water/rain",
            );
            reports.push(serde_json::json!({"mode":label,"meanNativeCompletedFrameMs":frames.iter().sum::<f64>()/frames.len() as f64,"frozenFrameExact":true}));
        }
        fs::write(
            format!("{dir}/fluid-render.json"),
            serde_json::to_string_pretty(&reports).unwrap(),
        )
        .unwrap();
        println!("GPU fluid rendering passed: evolving clear/rain/tempest water, unchanged frozen frames, depth refraction and reflection");
        return;
    }
    if check.as_deref() == Some("lightning") {
        renderer.set_quality(1);
        renderer.set_render_resolution(720);
        renderer.set_filter(1, 1.0);
        let x = -8909.148;
        let z = -77660.938;
        let hour = 18.7;
        let eye = glam::Vec3::new(x, geometry::walk_height(&world, x, z) + 1.72, z);
        renderer.set_weather_mode(5);
        renderer.update_weather(&world, eye, hour, 0.0);
        for _ in 0..6000 {
            renderer.update_weather(&world, eye, hour, 0.025);
            if renderer.weather_state().lightning > 0.90 {
                break;
            }
        }
        assert!(
            renderer.weather_state().lightning > 0.90,
            "a real scheduled storm strike must occur"
        );
        renderer.update_chunks(&world, eye, true);
        while renderer.pending_count() > 0 {
            renderer.update_chunks(&world, eye, false);
        }
        for i in 0..8 {
            let yaw = i as f32 * std::f32::consts::FRAC_PI_4;
            renderer.render(eye, yaw, 0.18, hour).unwrap();
            save_png(
                &format!("{dir}/strike-{i}.png"),
                1280,
                720,
                &renderer.capture_rgba().unwrap(),
            );
        }
        fs::write(
            format!("{dir}/strike.json"),
            serde_json::to_string_pretty(renderer.weather_state()).unwrap(),
        )
        .unwrap();
        return;
    }
    if check.as_deref() == Some("climate") {
        renderer.set_quality(1);
        renderer.set_render_resolution(720);
        renderer.set_ground_cover_density(4.0);
        renderer.set_filter(1, 1.0);
        let views = [
            (
                "forest-lake",
                -8909.148,
                -77660.938,
                -0.2618038,
                -0.0438113,
                9.3,
                2,
                25,
            ),
            (
                "lake-storm",
                -8909.148,
                -77660.938,
                -0.2618038,
                -0.0438113,
                15.0,
                5,
                45,
            ),
            (
                "lake-winter",
                -8909.148,
                -77660.938,
                -0.2618038,
                -0.0438113,
                10.0,
                6,
                540,
            ),
            (
                "mountain-water",
                -56764.645,
                -42213.234,
                -2.0944018,
                -0.0126998,
                8.0,
                2,
                25,
            ),
            (
                "sunrise-arches",
                23512.3,
                63468.41,
                -1.9067289,
                0.27,
                7.2,
                2,
                25,
            ),
            (
                "winter-arches",
                23512.3,
                63468.41,
                -1.9067289,
                0.27,
                10.3,
                7,
                540,
            ),
            (
                "ancient-spires",
                -10503.36,
                -46323.387,
                0.7956443,
                0.26,
                15.4,
                2,
                25,
            ),
            (
                "basalt-organ",
                -150293.66,
                57163.99,
                0.0805893,
                0.30,
                15.8,
                2,
                25,
            ),
            (
                "copper-moons",
                -93242.0,
                43300.0,
                2.493867,
                0.36,
                19.4,
                1,
                25,
            ),
            (
                "sea-cliffs",
                -82546.12,
                -92297.91,
                0.16122533,
                0.22,
                8.1,
                2,
                25,
            ),
        ];
        let mut reports = Vec::new();
        for (label, x, z, yaw, pitch, hour, mode, seconds) in views {
            let eye = glam::Vec3::new(x, geometry::walk_height(&world, x, z) + 1.72, z);
            renderer.clear_chunks();
            renderer.set_weather_mode(mode);
            renderer.set_weather_speed(1.0);
            renderer.set_weather_paused(false);
            renderer.update_weather(&world, eye, hour, 0.0);
            for _ in 0..seconds {
                renderer.update_weather(&world, eye, hour, 1.0);
            }
            renderer.update_chunks(&world, eye, true);
            while renderer.pending_count() > 0 {
                renderer.update_chunks(&world, eye, false);
            }
            renderer.advance_time(0.1);
            renderer.render(eye, yaw, pitch, hour).unwrap();
            save_png(
                &format!("{dir}/{label}.png"),
                1280,
                720,
                &renderer.capture_rgba().unwrap(),
            );
            reports.push(serde_json::json!({"label":label,"eye":eye.to_array(),"yaw":yaw,"pitch":pitch,"hour":hour,"weather":renderer.weather_state(),"meshMB":renderer.mesh_bytes() as f64/1e6}));
            if label == "forest-lake" || label == "lake-storm" {
                renderer.set_render_resolution(450);
                let mut timings = Vec::new();
                for i in 0..70 {
                    renderer.advance_time(1.0 / 60.0);
                    renderer.update_weather(&world, eye, hour, 1.0 / 60.0);
                    let start = Instant::now();
                    renderer
                        .render(eye, yaw + (i as f32 * 0.001), pitch, hour)
                        .unwrap();
                    renderer.device.poll(wgpu::PollType::Wait).unwrap();
                    if i >= 10 {
                        timings.push(start.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                timings.sort_by(f64::total_cmp);
                let mean = timings.iter().sum::<f64>() / timings.len() as f64;
                let report = serde_json::json!({"nativeGpuCompletedFrameMeanMs":mean,"p95Ms":timings[56],"internal":[800,450],"output":[1280,720],"groundCover":4.0,"reflections":true,"camera":"slow pan", "browserFps":null});
                println!("{label} native timing: {report}");
                reports.push(report);
                renderer.set_render_resolution(720);
            }
            println!("Captured {label}");
        }
        fs::write(
            format!("{dir}/climate-report.json"),
            serde_json::to_string_pretty(&reports).unwrap(),
        )
        .unwrap();
        return;
    }
    if check.as_deref() == Some("weather") {
        renderer.set_quality(1);
        renderer.set_render_resolution(720);
        renderer.set_ground_cover_density(4.0);
        renderer.set_filter(1, 1.0);
        // Native photographs use the same ground height, mesh and shader as WASM.
        let views = [
            (
                "high-lake",
                -38055.543,
                -11834.824,
                -0.58905,
                -0.041625,
                9.2,
            ),
            (
                "whisper-arch",
                23552.793,
                63454.277,
                -1.9067289,
                0.34824243,
                16.3,
            ),
            ("woodland", -10869.0, 58539.0, 1.9634955, -0.035, 15.0),
            (
                "river-cascade",
                -45970.496,
                63174.215,
                -0.26180425,
                0.18912803,
                10.5,
            ),
        ];
        let mut reports = Vec::new();
        for (label, x, z, yaw, pitch, hour) in views {
            let eye = glam::Vec3::new(x, geometry::walk_height(&world, x, z) + 1.72, z);
            renderer.clear_chunks();
            renderer.update_chunks(&world, eye, true);
            while renderer.pending_count() > 0 {
                renderer.update_chunks(&world, eye, false);
            }
            for (mode, name) in [
                (1, "clear"),
                (2, "cloudy"),
                (3, "rain"),
                (5, "tempest"),
                (6, "snow"),
                (7, "blizzard"),
            ] {
                if label == "river-cascade" && ![1, 3].contains(&mode) {
                    continue;
                }
                renderer.set_weather_paused(false);
                renderer.set_weather_speed(1.0);
                renderer.set_weather_mode(mode);
                for _ in 0..24 {
                    renderer.update_weather(&world, eye, hour, 1.0);
                }
                if mode >= 6 {
                    for _ in 0..420 {
                        renderer.update_weather(&world, eye, hour, 1.0);
                    }
                }
                renderer.advance_time(0.1);
                renderer.render(eye, yaw, pitch, hour).unwrap();
                let pixels = renderer.capture_rgba().unwrap();
                assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
                save_png(&format!("{dir}/{label}-{name}.png"), 1280, 720, &pixels);
                reports.push(serde_json::json!({"place":label,"weather":renderer.weather_state(),"eye":eye.to_array(),"yaw":yaw,"pitch":pitch,"hour":hour,"reflectionDraws":renderer.reflection_draws(),"meshMB":renderer.mesh_bytes() as f64 /1e6}));
                if label == "high-lake" && mode == 1 {
                    renderer.set_reflections(false);
                    renderer.render(eye, yaw, pitch, hour).unwrap();
                    let off = renderer.capture_rgba().unwrap();
                    let changed = pixels
                        .chunks_exact(4)
                        .zip(off.chunks_exact(4))
                        .filter(|(a, b)| a != b)
                        .count();
                    assert!(
                        changed > 500,
                        "actual water reflection should affect a visible lake: {changed}"
                    );
                    save_png(
                        &format!("{dir}/{label}-reflections-off.png"),
                        1280,
                        720,
                        &off,
                    );
                    renderer.set_reflections(true);
                    println!("Actual lake reflection changes {changed} pixels");
                }
                if mode == 5 && label == "whisper-arch" {
                    // Find an actual scheduled pulse; do not force a fake flash.
                    for _ in 0..6000 {
                        renderer.update_weather(&world, eye, hour, 0.025);
                        if renderer.weather_state().lightning > 0.80 {
                            break;
                        }
                    }
                    renderer.render(eye, yaw, pitch, hour).unwrap();
                    save_png(
                        &format!("{dir}/{label}-lightning.png"),
                        1280,
                        720,
                        &renderer.capture_rgba().unwrap(),
                    );
                    println!(
                        "Actual scheduled flash {}",
                        renderer.weather_state().lightning
                    );
                }
                println!(
                    "Captured {label} / {name}: {}",
                    renderer.weather_state().label
                );
            }
            if label == "high-lake" {
                renderer.set_weather_mode(1);
                for _ in 0..120 {
                    renderer.update_weather(&world, eye, 21.0, 1.0);
                }
                renderer.render(eye, yaw, 0.2, 21.0).unwrap();
                save_png(
                    &format!("{dir}/{label}-moonlight.png"),
                    1280,
                    720,
                    &renderer.capture_rgba().unwrap(),
                );
            }
        }
        fs::write(
            format!("{dir}/weather-report.json"),
            serde_json::to_string_pretty(&reports).unwrap(),
        )
        .unwrap();
        return;
    }
    if check.as_deref() == Some("epic") {
        renderer.set_quality(1);
        renderer.set_render_resolution(720);
        renderer.set_ground_cover_density(4.);
        renderer.set_filter(1, 0.90);
        let time = Instant::now();
        let wonders = fantasy_land::natural::destinations(&world);
        println!(
            "Found {} natural wonder viewpoints in {:?}",
            wonders.len(),
            time.elapsed()
        );
        fs::write(
            format!("{dir}/wonders.json"),
            serde_json::to_string_pretty(&wonders).unwrap(),
        )
        .unwrap();
        let mut cameras = Vec::new();
        for w in &wonders {
            let label = format!("{:?}", w.kind).to_lowercase();
            cameras.push((label, w.x, w.z, w.yaw, w.pitch, 10.2));
        }
        let views = fantasy_land::exploration::destinations(&world);
        for v in views
            .iter()
            .filter(|v| ["Ancient woodland", "Granite highlands", "Meadowlands"].contains(&v.name))
        {
            cameras.push((
                v.name.to_lowercase().replace(' ', "-"),
                v.x,
                v.z,
                v.yaw,
                v.pitch,
                if v.name == "Ancient woodland" {
                    16.1
                } else {
                    9.5
                },
            ));
        }
        // Find a real dry edge on the highest retained lake, facing its water.
        if let Some(lake) = world
            .lakes()
            .iter()
            .max_by(|a, b| a.surface.total_cmp(&b.surface))
        {
            let mut best = None;
            let mut score = f32::NEG_INFINITY;
            for radius in [150., 300., 500., 800., 1200.] {
                for i in 0..32 {
                    let a = i as f32 * std::f32::consts::TAU / 32.;
                    let x = lake.center[0] + a.sin() * radius;
                    let z = lake.center[1] + a.cos() * radius;
                    let s = world.natural_sample(x, z);
                    let h = geometry::walk_height(&world, x, z);
                    if s.ocean || h < s.water_height + 0.5 || geometry::blocks_player(&world, x, z)
                    {
                        continue;
                    }
                    let v = -(h - lake.surface - 35.).abs() - (radius - 500.).abs() * 0.015;
                    if v > score {
                        score = v;
                        best = Some((
                            x,
                            z,
                            (lake.center[0] - x).atan2(z - lake.center[1]),
                            ((lake.surface - h - 1.72) / radius)
                                .atan()
                                .clamp(-0.24, 0.08),
                        ));
                    }
                }
            }
            if let Some((x, z, yaw, pitch)) = best {
                cameras.push(("mountain-lake".into(), x, z, yaw, pitch, 9.2));
            }
        }
        if let Some(w) = wonders.first() {
            cameras.push(("solenne-sunset".into(), w.x, w.z, w.yaw, w.pitch, 17.1));
        }
        for (label, hour) in [("aster-and-vey", 19.4)] {
            let sky = fantasy_land::celestial::state(hour);
            let d = if hour > 19. {
                (sky.aster + sky.vey).normalize()
            } else {
                sky.sun
            };
            let horizontal = glam::Vec2::new(d.x, d.z).normalize();
            let mut chosen = None;
            for w in wonders.iter().filter(|w| {
                matches!(
                    w.kind,
                    fantasy_land::natural::NaturalKind::Arch
                        | fantasy_land::natural::NaturalKind::StoneWindow
                        | fantasy_land::natural::NaturalKind::Pinnacles
                )
            }) {
                for radius in [70., 100., 140.] {
                    let x = w.landmark_x - horizontal.x * radius;
                    let z = w.landmark_z - horizontal.y * radius;
                    let s = world.natural_sample(x, z);
                    let y = geometry::walk_height(&world, x, z);
                    if s.ocean || y < s.water_height + 0.5 || geometry::blocks_player(&world, x, z)
                    {
                        continue;
                    }
                    let grade = (world.height(x + 3., z) - world.height(x - 3., z))
                        .hypot(world.height(x, z + 3.) - world.height(x, z - 3.))
                        / 6.;
                    if grade > 0.55 {
                        continue;
                    }
                    chosen = Some((
                        label.into(),
                        x,
                        z,
                        horizontal.x.atan2(-horizontal.y),
                        if hour > 19. { 0.36 } else { 0.10 },
                        hour,
                    ));
                    break;
                }
                if chosen.is_some() {
                    break;
                }
            }
            if let Some(c) = chosen {
                cameras.push(c);
            }
        }
        fs::write(
            format!("{dir}/cameras.json"),
            serde_json::to_string_pretty(&cameras).unwrap(),
        )
        .unwrap();
        for (label, x, z, yaw, pitch, hour) in cameras {
            let eye = glam::Vec3::new(x, geometry::walk_height(&world, x, z) + 1.72, z);
            renderer.clear_chunks();
            renderer.update_chunks(&world, eye, true);
            while renderer.pending_count() > 0 {
                renderer.update_chunks(&world, eye, false);
            }
            renderer.render(eye, yaw, pitch, hour).unwrap();
            let pixels = renderer.capture_rgba().unwrap();
            assert_image(&pixels, 1280, 720, &label);
            save_png(&format!("{dir}/{label}.png"), 1280, 720, &pixels);
            println!(
                "Captured {label}: {x:.1},{z:.1} hour{hour:.1} mesh{:.1}MB",
                renderer.mesh_bytes() as f64 / 1e6
            );
        }
        return;
    }
    if check.as_deref() == Some("journeys") {
        let journeys = fantasy_land::journeys::discover(&world);
        for j in &journeys {
            assert!((5.0..=10.0).contains(&j.minutes));
            assert!(j.points.len() >= 2);
            for p in &j.points {
                let s = world.natural_sample(p[0], p[1]);
                assert!(!s.ocean && s.height > s.water_height + 0.3);
            }
            println!(
                "Journey {}: {:.1} minutes, {} turns",
                j.name,
                j.minutes,
                j.points.len()
            );
        }
        fs::write(
            format!("{dir}/journeys.json"),
            serde_json::to_string_pretty(&journeys).unwrap(),
        )
        .unwrap();
        assert_eq!(
            journeys.len(),
            3,
            "all three walking landscapes should produce a route"
        );
        return;
    }
    if check.as_deref() == Some("vista") {
        // Exact position/orientation from the user's empty-valley feedback.
        // Keep this camera stable even when the default spawn selection changes.
        renderer.set_quality(1);
        renderer.set_render_resolution(450);
        renderer.set_ground_cover_density(4.0);
        renderer.set_filter(1, 1.0);
        let eye = glam::Vec3::new(3215.626953125, 384.6331787109375, -14435.6171875);
        renderer.update_chunks(&world, eye, true);
        while renderer.pending_count() > 0 {
            renderer.update_chunks(&world, eye, false);
        }
        renderer
            .render(eye, 1.3643598556518555, -0.18, 11.)
            .unwrap();
        save_png(
            &format!("{dir}/feedback-vista.png"),
            1280,
            720,
            &renderer.capture_rgba().unwrap(),
        );
        println!(
            "Feedback vista: mesh{}MB triangles{}",
            renderer.mesh_bytes() as f64 / 1e6,
            renderer.triangle_count()
        );
        return;
    }
    if check.as_deref() == Some("lakes") {
        verify_lakes(&world, &mut renderer, &dir);
        return;
    }
    if check.as_deref() == Some("portraits") {
        renderer.set_quality(1);
        renderer.set_render_resolution(720);
        renderer.set_filter(1, 0.85);
        renderer.set_ground_cover_density(4.);
        let views = fantasy_land::exploration::destinations(&world);
        for view in &views {
            let eye = glam::Vec3::new(
                view.x,
                geometry::walk_height(&world, view.x, view.z) + 1.72,
                view.z,
            );
            renderer.clear_chunks();
            renderer.update_chunks(&world, eye, true);
            while renderer.pending_count() > 0 {
                renderer.update_chunks(&world, eye, false);
            }
            let label = view.name.to_lowercase().replace(' ', "-");
            let hour = if label.contains("woodland") {
                15.0
            } else {
                9.5
            };
            renderer.render(eye, view.yaw, view.pitch, hour).unwrap();
            save_png(
                &format!("{dir}/{label}.png"),
                1280,
                720,
                &renderer.capture_rgba().unwrap(),
            );
            println!(
                "Portrait {} at {},{} yaw{} mesh{}MB",
                label,
                view.x,
                view.z,
                view.yaw,
                renderer.mesh_bytes() as f64 / 1e6
            );
        }
        fs::write(
            format!("{dir}/views.json"),
            serde_json::to_string_pretty(&views).unwrap(),
        )
        .unwrap();
        return;
    }
    if check.as_deref() == Some("formations") {
        renderer.set_quality(1);
        renderer.set_render_resolution(720);
        renderer.set_filter(1, 0.85);
        renderer.set_ground_cover_density(4.);
        let mut views = Vec::new();
        for (name, x, z) in [
            ("granite-tors", -74414., 70544.),
            ("sandstone-ledges", 49881., 27098.),
            ("coastal-rocks", 47049., 124785.),
        ] {
            let mut best: Option<(f32, glam::Vec3, f32, f32)> = None;
            for p in geometry::formation_locations(&world, x, z, 420.)
                .iter()
                .take(24)
            {
                for i in 0..16 {
                    let a = i as f32 * std::f32::consts::TAU / 16.;
                    let cx = p[0] + a.sin() * 32.;
                    let cz = p[2] + a.cos() * 32.;
                    let s = world.sample(cx, cz);
                    if s.ocean
                        || s.water_height > s.height - 0.4
                        || geometry::blocks_player(&world, cx, cz)
                    {
                        continue;
                    }
                    let y = geometry::walk_height(&world, cx, cz) + 1.72;
                    let grade = (world.height(cx + 3., cz) - world.height(cx - 3., cz))
                        .hypot(world.height(cx, cz + 3.) - world.height(cx, cz - 3.))
                        / 6.;
                    if grade > 0.65 {
                        continue;
                    }
                    let yaw = (p[0] - cx).atan2(-(p[2] - cz));
                    let pitch = ((p[1] + 3. - y) / 32.).atan().clamp(-0.16, 0.16);
                    let back = world.natural_sample(cx + yaw.sin() * 1600., cz - yaw.cos() * 1600.);
                    let background = ((back.height.max(back.water_height) - y) / 1600.).atan();
                    let score = 4. - (p[1] + 3. - y).abs() * 0.22 - grade * 5.
                        + background.clamp(-0.2, 0.24) * 7.
                        - world.vegetation_density_from_sample(cx, cz, &s) * 3.;
                    if best.as_ref().is_none_or(|v| score > v.0) {
                        best = Some((score, glam::Vec3::new(cx, y, cz), yaw, pitch));
                    }
                }
            }
            if let Some((_, eye, yaw, pitch)) = best {
                renderer.clear_chunks();
                renderer.update_chunks(&world, eye, true);
                while renderer.pending_count() > 0 {
                    renderer.update_chunks(&world, eye, false);
                }
                renderer.render(eye, yaw, pitch, 9.5).unwrap();
                save_png(
                    &format!("{dir}/{name}.png"),
                    1280,
                    720,
                    &renderer.capture_rgba().unwrap(),
                );
                views.push(
                    serde_json::json!({"name":name,"eye":eye.to_array(),"yaw":yaw,"pitch":pitch}),
                );
                println!("Formation portrait {name} at {eye:?} yaw{yaw} pitch{pitch}");
            }
        }
        fs::write(
            format!("{dir}/views.json"),
            serde_json::to_string_pretty(&views).unwrap(),
        )
        .unwrap();
        return;
    }
    if check.as_deref() == Some("shot") {
        let args: Vec<String> = std::env::args().collect();
        let read =
            |i: usize, default: f32| args.get(i).and_then(|v| v.parse().ok()).unwrap_or(default);
        let x = read(4, spawn[0]);
        let z = read(5, spawn[1]);
        let eye = glam::Vec3::new(x, geometry::walk_height(&world, x, z) + 1.72, z);
        renderer.set_quality(1);
        renderer.set_render_resolution(720);
        renderer.set_filter(1, 0.85);
        renderer.set_ground_cover_density(4.);
        renderer.set_weather_mode(read(9, 0.0) as u32);
        renderer.update_weather(&world, eye, read(8, 9.5), 0.0);
        for _ in 0..read(10, 0.0).clamp(0.0, 3600.0) as u32 {
            renderer.update_weather(&world, eye, read(8, 9.5), 1.0);
        }
        renderer.update_chunks(&world, eye, true);
        while renderer.pending_count() > 0 {
            renderer.update_chunks(&world, eye, false);
        }
        renderer
            .render(eye, read(6, yaw), read(7, -0.04), read(8, 9.5))
            .unwrap();
        save_png(
            &format!("{dir}/shot.png"),
            1280,
            720,
            &renderer.capture_rgba().unwrap(),
        );
        println!("Shot ground eye {:?}", eye);
        return;
    }
    if regions_only {
        verify_regions(&world, &mut renderer, &dir);
        return;
    }
    if cover_only {
        verify_cover(&world, &mut renderer, &dir);
        return;
    }
    if coasts_only {
        verify_coasts(&world, &mut renderer, &dir);
        return;
    }
    if roads_only {
        verify_roads(&world, &mut renderer, &dir);
        return;
    }
    if grounding_only {
        verify_grounding(&world, &mut renderer, &dir);
        return;
    }
    let floor = geometry::walk_height(&world, spawn[0], spawn[1]);
    let eye = glam::Vec3::new(spawn[0], floor + 1.72, spawn[1]);
    let start = Instant::now();
    renderer.update_chunks(&world, eye, true);
    while renderer.pending_count() > 0 {
        renderer.update_chunks(&world, eye, false);
    }
    println!(
        "{} chunks, {} triangles, generation {:?}",
        renderer.chunk_count(),
        renderer.triangle_count(),
        start.elapsed()
    );
    if filters_only {
        verify_filters(&world, &mut renderer, eye, yaw, &dir);
        return;
    }
    renderer
        .render(eye, yaw, -0.10, 9.0)
        .expect("render native world");
    let pixels = renderer.capture_rgba().expect("read rendered image");
    save_png(&format!("{dir}/spawn.png"), 1280, 720, &pixels);
    let opaque = pixels.chunks(4).filter(|p| p[3] == 255).count();
    assert_eq!(opaque, 1280 * 720);
    let colors: std::collections::HashSet<_> =
        pixels.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
    assert!(
        colors.len() > 100,
        "render must contain terrain and scenery, got {} colors",
        colors.len()
    );
    println!(
        "Native GPU render validated: {} colors, no pipeline/validation errors",
        colors.len()
    );
    renderer
        .render(eye + glam::Vec3::Y * 100., yaw, -0.25, 14.0)
        .expect("render elevated context");
    save_png(
        &format!("{dir}/terrain-context.png"),
        1280,
        720,
        &renderer.capture_rgba().unwrap(),
    );
    let advance = glam::Vec3::new(yaw.sin(), 0., -yaw.cos()) * 330.;
    let mut river_eye = eye + advance;
    river_eye.y = geometry::walk_height(&world, river_eye.x, river_eye.z) + 1.72;
    renderer.update_chunks(&world, river_eye, true);
    while renderer.pending_count() > 0 {
        renderer.update_chunks(&world, river_eye, false);
    }
    renderer.render(river_eye, yaw + 0.22, -0.07, 11.0).unwrap();
    save_png(
        &format!("{dir}/river-approach.png"),
        1280,
        720,
        &renderer.capture_rgba().unwrap(),
    );
    renderer.render(river_eye, yaw + 0.22, 0.20, 22.0).unwrap();
    save_png(
        &format!("{dir}/night.png"),
        1280,
        720,
        &renderer.capture_rgba().unwrap(),
    );
    let metadata = serde_json::json!({"hydrology":world.hydrology_stats(),"seed":world.seed,"sizeMeters":WORLD_SIZE,"spawn":spawn,"sites":world.sites_near(spawn[0],spawn[1],6000.),"landmarks":world.landmarks_near(spawn[0],spawn[1],3000.),"chunkCount":renderer.chunk_count(),"triangleCount":renderer.triangle_count()});
    fs::write(
        format!("{dir}/world.json"),
        serde_json::to_string_pretty(&metadata).unwrap(),
    )
    .unwrap();
    fn verify_regions(world: &World, renderer: &mut Renderer, dir: &str) {
        use fantasy_land::regions::{self, LandscapeKind};
        use std::collections::BTreeMap;
        let kinds = [
            LandscapeKind::AncientWoodland,
            LandscapeKind::GraniteHighlands,
            LandscapeKind::WindsweptCoast,
            LandscapeKind::WetLowlands,
            LandscapeKind::SandstoneCountry,
            LandscapeKind::Meadowlands,
            LandscapeKind::Alpine,
        ];
        let mut candidates: BTreeMap<String, (f32, [f32; 2])> = BTreeMap::new();
        let mut occurrences: BTreeMap<String, Vec<[i32; 2]>> = BTreeMap::new();
        // Unbiased regular samples choose actual generated country, never planted showcases.
        for iz in -62..=62 {
            for ix in -62..=62 {
                let x = ix as f32 * 2900.0 + 731.0;
                let z = iz as f32 * 2900.0 + 419.0;
                let s = world.natural_sample(x, z);
                if s.ocean || s.water_height > s.height - 0.3 {
                    continue;
                }
                let r = regions::sample(world.seed, x, z, &s);
                let name = r.kind.name().to_string();
                let region_cell = [(x / 24000.).floor() as i32, (z / 24000.).floor() as i32];
                let cells = occurrences.entry(name.clone()).or_default();
                if !cells.contains(&region_cell) {
                    cells.push(region_cell);
                }
                let trees = fantasy_land::ecology::tree_density(world.seed, x, z, &s);
                let mut score = -r.slope.min(5.) * 8.0;
                score += match r.kind {
                    LandscapeKind::AncientWoodland => trees * 12. + r.ancient * 10.,
                    LandscapeKind::GraniteHighlands => r.rockiness * 8. + s.height / 250.,
                    LandscapeKind::WindsweptCoast => {
                        r.exposure * 5. - (world.coast_info(x, z).distance - 80.).abs() / 120.
                    }
                    LandscapeKind::WetLowlands => r.wetness * 12. + trees * 2.,
                    LandscapeKind::SandstoneCountry => r.rockiness * 7. + (1. - trees) * 3.,
                    LandscapeKind::Meadowlands => (1. - trees) * 10.,
                    LandscapeKind::Alpine => r.rockiness * 5. + s.height / 500.,
                };
                if candidates.get(&name).is_none_or(|old| score > old.0) {
                    candidates.insert(name, (score, [x, z]));
                }
            }
        }
        for kind in kinds {
            assert!(
                candidates.contains_key(kind.name()),
                "missing landscape {}",
                kind.name()
            );
            assert!(
                occurrences[kind.name()].len() >= 2,
                "landscape must repeat: {}",
                kind.name()
            );
        }
        println!(
            "Regional occurrences: {:?}",
            occurrences
                .iter()
                .map(|(k, v)| (k, v.len()))
                .collect::<Vec<_>>()
        );
        renderer.set_quality(1);
        renderer.set_render_resolution(450);
        renderer.set_ground_cover_density(4.0);
        renderer.set_filter(1, 1.0);
        let mut reports = Vec::new();
        for kind in kinds {
            let p = candidates[kind.name()].1;
            let eye = glam::Vec3::new(p[0], geometry::walk_height(world, p[0], p[1]) + 1.72, p[1]);
            let mut yaw = 0.0;
            if kind == LandscapeKind::WindsweptCoast {
                let dirs = [[-1., 0.], [1., 0.], [0., -1.], [0., 1.]];
                let mut nearest = f32::MAX;
                for d in dirs {
                    let dist = world
                        .coast_info(p[0] + d[0] * 600., p[1] + d[1] * 600.)
                        .distance;
                    if dist < nearest {
                        nearest = dist;
                        yaw = d[0].atan2(-d[1]);
                    }
                }
            }
            renderer.clear_chunks();
            let warm_start = Instant::now();
            renderer.update_chunks(world, eye, true);
            let mut updates = 0;
            while renderer.pending_count() > 0 {
                renderer.update_chunks(world, eye, false);
                updates += 1;
                assert!(updates < 8000);
            }
            let warm_ms = warm_start.elapsed().as_secs_f64() * 1000.;
            renderer.set_shadows(true);
            renderer.render(eye, yaw, -0.08, 10.0).unwrap();
            let on = renderer.capture_rgba().unwrap();
            let label = kind.name().to_lowercase().replace(' ', "-");
            save_png(&format!("{dir}/{label}.png"), 1280, 720, &on);
            let colors: std::collections::HashSet<_> =
                on.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
            assert!(colors.len() > 120, "empty regional view {label}");
            let sample = world.sample(p[0], p[1]);
            let region = regions::sample(world.seed, p[0], p[1], &sample);
            let mut times = Vec::new();
            for _ in 0..15 {
                let start = Instant::now();
                renderer.render(eye, yaw, -0.08, 10.0).unwrap();
                renderer.device.poll(wgpu::PollType::Wait).unwrap();
                times.push(start.elapsed().as_secs_f64() * 1000.);
            }
            let mut shadow_difference = 0;
            if kind == LandscapeKind::AncientWoodland {
                renderer.set_shadows(false);
                renderer.render(eye, yaw, -0.08, 10.0).unwrap();
                let off = renderer.capture_rgba().unwrap();
                shadow_difference = on
                    .chunks_exact(4)
                    .zip(off.chunks_exact(4))
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(
                    shadow_difference > 500,
                    "sunlight toggle must change visible woodland shade"
                );
                save_png(&format!("{dir}/{label}-shadows-off.png"), 1280, 720, &off);
                renderer.set_shadows(true);
                for (mode, name) in [(0, "clean"), (2, "crt")] {
                    renderer.set_filter(mode, 1.0);
                    renderer.render(eye, yaw, -0.08, 10.0).unwrap();
                    save_png(
                        &format!("{dir}/{label}-{name}.png"),
                        1280,
                        720,
                        &renderer.capture_rgba().unwrap(),
                    );
                }
                renderer.set_filter(1, 1.0);
                renderer.render(eye, yaw, -0.08, 18.0).unwrap();
                save_png(
                    &format!("{dir}/{label}-dusk.png"),
                    1280,
                    720,
                    &renderer.capture_rgba().unwrap(),
                );
                renderer.render(eye, yaw, -0.08, 23.0).unwrap();
                save_png(
                    &format!("{dir}/{label}-night.png"),
                    1280,
                    720,
                    &renderer.capture_rgba().unwrap(),
                );
            }
            let report = serde_json::json!({"landscape":region,"eye":eye.to_array(),"yaw":yaw,"warmMs":warm_ms,"meshBytes":renderer.mesh_bytes(),"frameMeanMs":times.iter().sum::<f64>()/times.len() as f64,"shadowDifferencePixels":shadow_difference,"cover":renderer.cover_stats()});
            println!("Regional capture {label}: {report}");
            reports.push(report);
        }
        verify_lakes(world, renderer, dir);
        let map = world.map_background_rgba(0., 0., WORLD_SIZE, 512);
        save_png(&format!("{dir}/regional-atlas.png"), 512, 512, &map);
        let report = serde_json::json!({"seed":world.seed,"landscapes":reports,"occurrences":occurrences,"lakes":world.lakes(),"method":"Native serialized render plus device.poll(Wait),15 completed frames,450p Bloom,400% cover; not browser FPS. Landscape locations chosen from generated terrain."});
        fs::write(
            format!("{dir}/regions-report.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
    }

    fn verify_lakes(world: &World, renderer: &mut Renderer, dir: &str) {
        renderer.set_quality(1);
        renderer.set_render_resolution(450);
        renderer.set_filter(1, 1.);
        renderer.set_ground_cover_density(4.);
        // Use actual water/terrain intersection points to frame a retained basin.
        assert!(!world.lakes().is_empty(), "world must retain natural lakes");
        let mut shore_view: Option<(f32, glam::Vec3, f32, u32)> = None;
        for lake in world.lakes().iter().take(12) {
            let center = world.natural_sample(lake.center[0], lake.center[1]);
            assert!(
                center.height < lake.surface && (center.water_height - lake.surface).abs() < 0.02
            );
            for i in 0..24 {
                let angle = i as f32 * std::f32::consts::TAU / 24.;
                let dir = [angle.sin(), angle.cos()];
                let mut wet = 0.;
                for step in 1..=160 {
                    let d = step as f32 * 64.;
                    let p = [lake.center[0] + dir[0] * d, lake.center[1] + dir[1] * d];
                    if !world.lake_at(p[0], p[1]).is_some_and(|l| l.id == lake.id) {
                        if wet < 128. {
                            break;
                        }
                        let mut low = wet;
                        let mut high = d;
                        for _ in 0..12 {
                            let mid = (low + high) * 0.5;
                            if !world
                                .lake_at(
                                    lake.center[0] + dir[0] * mid,
                                    lake.center[1] + dir[1] * mid,
                                )
                                .is_some_and(|l| l.id == lake.id)
                            {
                                high = mid;
                            } else {
                                low = mid;
                            }
                        }
                        let dry = high + 12.;
                        let p = [lake.center[0] + dir[0] * dry, lake.center[1] + dir[1] * dry];
                        let ground = geometry::walk_height(world, p[0], p[1]);
                        if ground > lake.surface && ground < lake.surface + 18. {
                            let sample = world.natural_sample(p[0], p[1]);
                            let trees = fantasy_land::ecology::tree_density(
                                world.seed, p[0], p[1], &sample,
                            );
                            let score = wet - (ground - lake.surface) * 30. + (1. - trees) * 600.;
                            if shore_view.is_none_or(|old| score > old.0) {
                                shore_view = Some((
                                    score,
                                    glam::Vec3::new(p[0], ground + 1.72, p[1]),
                                    (-dir[0]).atan2(dir[1]),
                                    lake.id,
                                ));
                            }
                        }
                        break;
                    }
                    wet = d;
                }
            }
        }
        let (_, lake_eye, lake_yaw, lake_id) =
            shore_view.expect("a lake must have an accessible shore");
        renderer.clear_chunks();
        renderer.update_chunks(world, lake_eye, true);
        while renderer.pending_count() > 0 {
            renderer.update_chunks(world, lake_eye, false);
        }
        renderer.render(lake_eye, lake_yaw, -0.06, 10.).unwrap();
        save_png(
            &format!("{dir}/retained-lake.png"),
            1280,
            720,
            &renderer.capture_rgba().unwrap(),
        );
        println!(
            "Retained lake{lake_id} shore: {:?}, yaw{lake_yaw}",
            lake_eye
        );
    }

    fn verify_cover(world: &World, renderer: &mut Renderer, dir: &str) {
        const WIDTH: u32 = 1280;
        const HEIGHT: u32 = 720;
        const HOUR: f32 = 11.0;
        const CAPTURE_PITCH: f32 = -0.18;
        const BENCHMARK_PITCH: f32 = -0.10;
        const FRAMES: usize = 30;
        let (spawn, yaw) = world.spawn_view();
        let eye = glam::Vec3::new(
            spawn[0],
            geometry::walk_height(world, spawn[0], spawn[1]) + 1.72,
            spawn[1],
        );
        renderer.set_quality(1);
        renderer.resize(WIDTH, HEIGHT);
        renderer.set_render_resolution(450);
        renderer.set_filter(1, 1.0);
        renderer.set_ground_cover_density(1.0);
        let warm_start = Instant::now();
        renderer.update_chunks(world, eye, true);
        let warm_updates = drain_cover_queues(world, renderer, eye);
        renderer.device.poll(wgpu::PollType::Wait).unwrap();
        let warm_ms = warm_start.elapsed().as_secs_f64() * 1000.0;
        let resident = cover_residency(renderer);
        assert!(
            resident["cover"]["loaded_instances"].as_u64().unwrap() > 0,
            "spawn must contain resident ground cover"
        );
        println!("Cover warmed in {warm_ms:.1} ms: {resident}");

        // Keep simulation time frozen: every density sees exactly the same
        // camera, daylight, wind and postprocess inputs. No streaming here.
        let mut captures = Vec::new();
        let mut comparisons = Vec::new();
        let mut previous_pixels: Option<Vec<u8>> = None;
        let mut default_pixels = Vec::new();
        let mut previous_drawn = 0;
        for density in [0.0, 1.0, 2.0, 4.0] {
            let before = cover_residency(renderer);
            renderer.set_ground_cover_density(density);
            assert_eq!(
                cover_residency(renderer),
                before,
                "density {density}: setter changed resident resources or queues"
            );
            renderer.render(eye, yaw, CAPTURE_PITCH, HOUR).unwrap();
            let pixels = renderer.capture_rgba().unwrap();
            let label = format!("cover-{density:.0}x");
            assert_image(&pixels, WIDTH, HEIGHT, &label);
            save_png(&format!("{dir}/{label}.png"), WIDTH, HEIGHT, &pixels);
            let stats = serde_json::to_value(renderer.cover_stats()).unwrap();
            let drawn = stats["drawn_instances"].as_u64().unwrap();
            if density == 0.0 {
                assert_eq!(drawn, 0, "Off still submitted ground-cover instances");
                assert_eq!(stats["drawn_tiles"].as_u64(), Some(0));
                assert_eq!(stats["drawn_triangles"].as_u64(), Some(0));
            } else {
                assert!(
                    drawn > previous_drawn,
                    "density {density}: expected a larger submitted instance prefix"
                );
            }
            if let Some(previous) = &previous_pixels {
                let difference = compare_images(previous, &pixels, WIDTH, HEIGHT);
                assert!(
                    difference.changed_fraction > 0.0001,
                    "density {density}: no visible cover change in fixed-camera capture"
                );
                comparisons.push(serde_json::json!({
                    "toDensity": density, "difference": difference.json()
                }));
            }
            assert_eq!(
                cover_residency(renderer),
                resident,
                "density {density}: rendering changed resident resources"
            );
            if density == 1.0 {
                default_pixels = pixels.clone();
            }
            captures.push(
                serde_json::json!({"density":density,"file":format!("{label}.png"),"cover":stats}),
            );
            previous_pixels = Some(pixels);
            previous_drawn = drawn;
        }
        renderer.set_ground_cover_density(1.0);
        renderer.render(eye, yaw, CAPTURE_PITCH, HOUR).unwrap();
        assert_pixels_equal(
            &default_pixels,
            &renderer.capture_rgba().unwrap(),
            "returning to 1x must restore the identical fixed-time image",
        );

        let mut runs = Vec::new();
        for (run, densities) in [[0.0, 1.0, 2.0, 4.0], [4.0, 2.0, 1.0, 0.0]]
            .into_iter()
            .enumerate()
        {
            for density in densities {
                renderer.set_ground_cover_density(density);
                assert_eq!(cover_residency(renderer), resident);
                for _ in 0..3 {
                    renderer.render(eye, yaw, BENCHMARK_PITCH, HOUR).unwrap();
                    renderer.device.poll(wgpu::PollType::Wait).unwrap();
                }
                let mut cpu_ms = Vec::with_capacity(FRAMES);
                let mut completed_ms = Vec::with_capacity(FRAMES);
                for _ in 0..FRAMES {
                    let start = Instant::now();
                    renderer.render(eye, yaw, BENCHMARK_PITCH, HOUR).unwrap();
                    cpu_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                    renderer.device.poll(wgpu::PollType::Wait).unwrap();
                    completed_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                let entry = serde_json::json!({
                    "run":run+1,"density":density,"frames":FRAMES,
                    "cpuEncodeSubmit":timing_summary(&cpu_ms),
                    "gpuCompletedFrame":timing_summary(&completed_ms),
                    "cover":renderer.cover_stats(),
                });
                println!("Cover benchmark {entry}");
                runs.push(entry);
            }
        }
        assert_eq!(cover_residency(renderer), resident);

        // Four genuine 48 m cover-tile transitions, retaining the warm world.
        // Terrain chunk transitions are separately labelled rather than being
        // mistaken for the cost of a cover-only boundary crossing.
        renderer.set_ground_cover_density(4.0);
        let mut movement = Vec::new();
        let mut previous_eye = eye;
        for step in 1..=4 {
            let x = eye.x + step as f32 * 48.0;
            let next_eye = glam::Vec3::new(x, geometry::walk_height(world, x, eye.z) + 1.72, eye.z);
            let before = cover_residency(renderer);
            let first = Instant::now();
            renderer.update_chunks(world, next_eye, false);
            let first_ms = first.elapsed().as_secs_f64() * 1000.0;
            let after_first = cover_residency(renderer);
            let mut updates = vec![first_ms];
            for _ in 0..20000 {
                if cover_queues_empty(renderer) {
                    break;
                }
                let start = Instant::now();
                renderer.update_chunks(world, next_eye, false);
                updates.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            assert!(
                cover_queues_empty(renderer),
                "movement queues failed to drain"
            );
            renderer.render(next_eye, yaw, CAPTURE_PITCH, HOUR).unwrap();
            renderer.device.poll(wgpu::PollType::Wait).unwrap();
            let tile = |p: glam::Vec3| [(p.x / 48.0).floor() as i32, (p.z / 48.0).floor() as i32];
            let chunk = |p: glam::Vec3| {
                [
                    (p.x / geometry::CHUNK_SIZE).floor() as i32,
                    (p.z / geometry::CHUNK_SIZE).floor() as i32,
                ]
            };
            assert_ne!(tile(previous_eye), tile(next_eye));
            let entry = serde_json::json!({
                "step":step,"position":[next_eye.x,next_eye.y,next_eye.z],
                "fromCoverTile":tile(previous_eye),"toCoverTile":tile(next_eye),
                "terrainChunkChanged":chunk(previous_eye)!=chunk(next_eye),
                "firstUpdateChunksCpuMs":first_ms,"allUpdateChunksCpu":timing_summary(&updates),
                "totalUpdateChunksCpuMs":updates.iter().sum::<f64>(),
                "before":before,"afterFirstUpdate":after_first,"afterDrain":cover_residency(renderer),
            });
            println!("Cover movement {entry}");
            movement.push(entry);
            previous_eye = next_eye;
        }
        let report = serde_json::json!({
            "seed":world.seed,"scene":"spawn","eye":[eye.x,eye.y,eye.z],"yaw":yaw,
            "hour":HOUR,"capturePitch":CAPTURE_PITCH,"benchmarkPitch":BENCHMARK_PITCH,
            "quality":"Balanced","output":[WIDTH,HEIGHT],"internal":[800,450],"filter":"Bloom",
            "methodology":{
                "frameTiming":"Native serialized render() plus device.poll(Wait), including CPU encoding/submission and GPU completion wait; no capture/readback in timed frames. Not pure GPU time or browser FPS.",
                "sampling":"Two runs of 30 frames per density, reversed density order on the second run, 3 untimed completed frames before each sample. Fixed simulation time in captures and timings.",
                "streaming":"CPU wall time of update_chunks only, force=false across four 48m tile crossings; queues drained at each position without clearing or regenerating the world. Terrain-chunk crossings are identified separately.",
                "residency":"All density setters and fixed-camera renders preserve chunk counts, pending queues, mesh bytes, loaded cover tiles/instances and cover buffer bytes. Off changes only submitted cover draws; trees and structural props remain.",
            },
            "historicalBaseline":{
                "source":"Prior native render-bench spawn, Balanced, 800x450 Bloom, pitch -0.10, hour 11",
                "gpuCompletedMeanMs":[8.25597075,9.24977121],"meshBytes":239262012,
                "limitations":"Historical runs used 100 animated frames after 20 warmups; current bounded runs use 30 frozen-time frames after 3 warmups. Host load and renderer changes can differ, so this is context rather than an isolated causal comparison. meshBytes is mesh/cover buffer payload, not total GPU memory.",
            },
            "warmup":{"elapsedMs":warm_ms,"subsequentUpdateCalls":warm_updates},
            "resident":resident,"captures":captures,"captureDifferences":comparisons,
            "runs":runs,"movement":movement,
        });
        fs::write(
            format!("{dir}/cover-report.json"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("Cover density, fixed-resource invariants, native frame timings and tile streaming validated");
    }

    fn cover_residency(renderer: &Renderer) -> serde_json::Value {
        let stats = serde_json::to_value(renderer.cover_stats()).unwrap();
        let mut cover = serde_json::Map::new();
        for key in [
            "tile_count",
            "loaded_instances",
            "buffer_bytes",
            "pending_tiles",
        ] {
            cover.insert(key.to_string(), stats[key].clone());
        }
        serde_json::json!({
            "chunks":renderer.chunk_count(),"pending":renderer.pending_count(),
            "meshBytes":renderer.mesh_bytes(),"cover":cover,
        })
    }

    fn cover_queues_empty(renderer: &Renderer) -> bool {
        let stats = serde_json::to_value(renderer.cover_stats()).unwrap();
        renderer.pending_count() == 0 && stats["pending_tiles"].as_u64() == Some(0)
    }

    fn drain_cover_queues(world: &World, renderer: &mut Renderer, eye: glam::Vec3) -> usize {
        for calls in 0..20000 {
            if cover_queues_empty(renderer) {
                return calls;
            }
            renderer.update_chunks(world, eye, false);
        }
        panic!("terrain/cover warmup queues failed to drain");
    }

    fn timing_summary(values: &[f64]) -> serde_json::Value {
        assert!(!values.is_empty());
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let percentile = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
        serde_json::json!({
            "samples":values.len(),"meanMs":values.iter().sum::<f64>()/values.len() as f64,
            "minMs":sorted[0],"p50Ms":percentile(0.50),"p95Ms":percentile(0.95),
            "maxMs":sorted[sorted.len()-1],
        })
    }

    fn verification_region_site(world: &World) -> fantasy_land::world::Site {
        const OLD_MISTFIELD: [f32; 2] = [-16545.926, -12303.939];
        const MISTFIELD_ID: u32 = 1845583983;
        let mut sites = world.sites_near(OLD_MISTFIELD[0], OLD_MISTFIELD[1], 3000.0);
        sites.sort_by(|a, b| {
            let priority = |site: &fantasy_land::world::Site| {
                let distance = (site.x - OLD_MISTFIELD[0]).hypot(site.z - OLD_MISTFIELD[1]);
                (site.id != MISTFIELD_ID, distance)
            };
            let a = priority(a);
            let b = priority(b);
            a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1))
        });
        let site = sites
            .into_iter()
            .next()
            .expect("no settlement in the reference region");
        if world.seed == 1337 {
            assert_eq!(
                site.id, MISTFIELD_ID,
                "Mistfield ID must resolve after site relocation"
            );
        }
        site
    }

    struct RoadCapture {
        label: &'static str,
        route_id: u64,
        point: [f32; 2],
        target: [f32; 2],
        half_width: f32,
        shoulder_width: f32,
        query_radius: f32,
    }
    fn road_point_ahead(points: &[[f32; 2]], segment: usize, t: f32, distance: f32) -> [f32; 2] {
        let mut point = [
            points[segment][0] + (points[segment + 1][0] - points[segment][0]) * t,
            points[segment][1] + (points[segment + 1][1] - points[segment][1]) * t,
        ];
        let mut remaining = distance;
        for &next in &points[segment + 1..] {
            let length = (next[0] - point[0]).hypot(next[1] - point[1]);
            if length >= remaining && length > 0.001 {
                return [
                    point[0] + (next[0] - point[0]) * remaining / length,
                    point[1] + (next[1] - point[1]) * remaining / length,
                ];
            }
            remaining -= length;
            point = next;
        }
        point
    }
    fn select_road_capture(
        world: &World,
        routes: &[fantasy_land::world::Road],
        reference: [f32; 2],
        kind: fantasy_land::world::RoadKind,
        label: &'static str,
        radius: f32,
    ) -> Option<RoadCapture> {
        let mut candidates = Vec::new();
        for (route_index, route) in routes
            .iter()
            .enumerate()
            .filter(|(_, route)| route.kind == kind)
        {
            for (segment, edge) in route.points.windows(2).enumerate() {
                let dx = edge[1][0] - edge[0][0];
                let dz = edge[1][1] - edge[0][1];
                let squared = dx * dx + dz * dz;
                if squared < 16.0 {
                    continue;
                }
                let t = (((reference[0] - edge[0][0]) * dx + (reference[1] - edge[0][1]) * dz)
                    / squared)
                    .clamp(0.15, 0.85);
                let point = [edge[0][0] + dx * t, edge[0][1] + dz * t];
                let distance = (point[0] - reference[0]).hypot(point[1] - reference[1]);
                candidates.push((distance, route_index, segment, t, point));
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Terrain queries are bounded even when an outer query returns long
        // polylines. Reject junctions whose visible surface belongs to a wider class.
        for (_, index, segment, t, point) in candidates.into_iter().take(128) {
            let sample = world.sample(point[0], point[1]);
            if sample.road_kind != Some(kind) || sample.water_height > sample.height - 0.05 {
                continue;
            }
            if geometry::blocks_player(world, point[0], point[1]) {
                continue;
            }
            if world
                .sites_near(point[0], point[1], 45.0)
                .iter()
                .any(|site| (site.x - point[0]).hypot(site.z - point[1]) < 40.0)
            {
                continue;
            }
            if world
                .landmarks_near(point[0], point[1], 20.0)
                .iter()
                .any(|site| (site.x - point[0]).hypot(site.z - point[1]) < 15.0)
            {
                continue;
            }
            let route = &routes[index];
            let target = road_point_ahead(&route.points, segment, t, 45.0);
            if (target[0] - point[0]).hypot(target[1] - point[1]) < 12.0 {
                continue;
            }
            return Some(RoadCapture {
                label,
                route_id: route.id,
                point,
                target,
                half_width: kind.half_width(),
                shoulder_width: kind.shoulder_width(),
                query_radius: radius,
            });
        }
        None
    }
    fn verify_roads(world: &World, renderer: &mut Renderer, dir: &str) {
        use fantasy_land::world::RoadKind;
        const WIDTH: u32 = 1280;
        const HEIGHT: u32 = 720;
        let site = verification_region_site(world);
        let reference = [site.x, site.z];
        let mut maps = Vec::new();
        for (label, span) in [("mistfield-region", 6000.0), ("broader-region", 24000.0)] {
            let start = Instant::now();
            let pixels = world.map_rgba(reference[0], reference[1], span, 512);
            assert_road_map(&pixels, 512, label);
            let file = format!("{dir}/roads-{label}.png");
            save_png(&file, 512, 512, &pixels);
            maps.push(serde_json::json!({"name":label,"file":file,"center":reference,"spanMeters":span,"pixels":512}));
            println!("Road map {label}: {:?} -> {file}", start.elapsed());
        }
        let classes = [
            (RoadKind::Main, "main"),
            (RoadKind::Lane, "lane"),
            (RoadKind::Trail, "trail"),
        ];
        let mut captures: [Option<RoadCapture>; 3] = [None, None, None];
        for radius in [4000.0, 9000.0, 18000.0] {
            let routes = world.road_routes_near(reference[0], reference[1], radius);
            for (index, &(kind, label)) in classes.iter().enumerate() {
                if captures[index].is_none() {
                    let near = captures[0].as_ref().map_or(reference, |view| view.point);
                    captures[index] =
                        select_road_capture(world, &routes, near, kind, label, radius);
                }
            }
            if captures.iter().all(Option::is_some) {
                break;
            }
        }
        renderer.set_quality(1);
        renderer.resize(WIDTH, HEIGHT);
        renderer.set_render_resolution(1);
        renderer.set_filter(0, 1.0);
        let mut views = Vec::new();
        let mut warmups = 0;
        for (index, capture) in captures.into_iter().enumerate() {
            let capture = capture.unwrap_or_else(|| {
                panic!(
                    "no dry {} route view within 18 km of {:?}",
                    classes[index].1, reference
                )
            });
            let eye = glam::Vec3::new(
                capture.point[0],
                geometry::walk_height(world, capture.point[0], capture.point[1]) + 1.72,
                capture.point[1],
            );
            let target = glam::Vec3::new(
                capture.target[0],
                geometry::walk_height(world, capture.target[0], capture.target[1]) + 0.7,
                capture.target[1],
            );
            let delta = target - eye;
            let yaw = delta.x.atan2(-delta.z);
            let pitch = (delta.y.atan2(delta.x.hypot(delta.z)) - 0.035).clamp(-0.35, 0.15);
            let start = Instant::now();
            // Reuse the existing renderer and overlapping streamed chunks. Views
            // within one chunk incur one warmup; more distant classes stream once
            // per view, without the unrelated spawn/map verification captures.
            renderer.update_chunks(world, eye, index == 0);
            if index == 0 || renderer.pending_count() > 0 {
                warmups += 1;
                while renderer.pending_count() > 0 {
                    renderer.update_chunks(world, eye, false);
                }
            }
            renderer
                .render(eye, yaw, pitch, 11.0)
                .expect("render actual road class");
            let pixels = renderer.capture_rgba().expect("capture actual road class");
            assert_image(&pixels, WIDTH, HEIGHT, capture.label);
            let file = format!("{dir}/road-{}.png", capture.label);
            save_png(&file, WIDTH, HEIGHT, &pixels);
            let local = world.map_rgba(capture.point[0], capture.point[1], 1000.0, 256);
            assert_road_map(&local, 256, capture.label);
            let map_file = format!("{dir}/road-{}-local-map.png", capture.label);
            save_png(&map_file, 256, 256, &local);
            views.push(serde_json::json!({
                "class":capture.label,"routeId":capture.route_id,"file":file,"localMap":map_file,
                "eye":eye.to_array(),"target":target.to_array(),"yaw":yaw,"pitch":pitch,"hour":11.0,
                "widthMeters":capture.half_width*2.0,"shoulderWidthMeters":capture.shoulder_width,
                "selectionRadiusMeters":capture.query_radius,"verifiedDryClassAtCamera":true,
                "chunkCount":renderer.chunk_count(),"triangleCount":renderer.triangle_count(),
            }));
            println!(
                "Road {}: route {}, camera {:?}, warm/render {:?} -> {file}",
                capture.label,
                capture.route_id,
                capture.point,
                start.elapsed()
            );
        }
        fs::write(
            format!("{dir}/roads-verification.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "seed":world.seed,"roadNetwork":world.road_stats(),"referenceSite":site.name,"referenceSiteId":site.id,
                "referencePosition":reference,"maps":maps,"views":views,"warmups":warmups,
                "captureDimensions":[WIDTH,HEIGHT],"sceneResolution":"native","filter":"clean",
                "appearanceNote":"Maps are actual world.map_rgba output. Each native capture stands on a dry route of its stated class. Inspect route hierarchy, bends, shoulders and off-road gaps visually; these image checks do not assert road-network quality."
            })).unwrap(),
        ).unwrap();
        println!(
            "Road verification passed: actual regional maps and all three native road classes"
        );
    }
    fn assert_road_map(pixels: &[u8], resolution: u32, label: &str) {
        assert_eq!(
            pixels.len(),
            resolution as usize * resolution as usize * 4,
            "{label}: wrong map dimensions"
        );
        assert!(
            pixels.chunks_exact(4).all(|p| p[3] == 255),
            "{label}: map is not opaque"
        );
        let colors: std::collections::HashSet<_> =
            pixels.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
        assert!(colors.len() > 16, "{label}: map is empty or near-solid");
    }

    fn verify_coasts(world: &World, renderer: &mut Renderer, dir: &str) {
        use fantasy_land::world::ShoreKind;
        let map = world.map_rgba(0., 0., WORLD_SIZE, 1024);
        save_png(
            &format!("{dir}/continent-and-islands.png"),
            1024,
            1024,
            &map,
        );
        assert_road_map(&map, 1024, "continent and islands");
        let mut scenes: Vec<(&str, [f32; 2], [f32; 2], f32)> = Vec::new();
        // Find actual zero-contour crossings independently of preselected coordinates.
        // The same shoreline is then inspected at walking and cliff-top height.
        let mut candidates = Vec::new();
        let half = WORLD_SIZE * 0.5;
        let step = 1500.;
        for row in 1..(WORLD_SIZE / step) as usize {
            let z = -half + row as f32 * step;
            let mut previous = world.is_land(-half, z);
            for column in 1..=(WORLD_SIZE / step) as usize {
                let x = -half + column as f32 * step;
                let land = world.is_land(x, z);
                if land != previous {
                    let (mut lo, mut hi) = (x - step, x);
                    for _ in 0..17 {
                        let mid = (lo + hi) * 0.5;
                        if world.is_land(mid, z) == previous {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    let coast = [(lo + hi) * 0.5, z];
                    let dx = world.coast_info(coast[0] + 24., z).distance
                        - world.coast_info(coast[0] - 24., z).distance;
                    let dz = world.coast_info(coast[0], z + 24.).distance
                        - world.coast_info(coast[0], z - 24.).distance;
                    let normal = glam::Vec2::new(dx, dz).normalize_or_zero();
                    let p = [coast[0] + normal.x * 75., z + normal.y * 75.];
                    let sample = world.natural_sample(p[0], p[1]);
                    if sample.ocean
                        || sample.water_height > sample.height
                        || !sample.height.is_finite()
                    {
                        previous = land;
                        continue;
                    }
                    candidates.push((
                        p,
                        normal.to_array(),
                        sample.shore,
                        world.landmass_id(p[0], p[1]),
                        sample.height,
                    ));
                }
                previous = land;
            }
        }
        for (name, shore, island) in [
            ("sandy-bay", ShoreKind::Beach, false),
            ("sea-cliffs", ShoreKind::Cliff, false),
            ("island-shore", ShoreKind::Beach, true),
        ] {
            let selected = candidates
                .iter()
                .filter(|c| c.2 == shore && c.3.is_some_and(|id| (id > 0) == island))
                .min_by(|a, b| {
                    let score = |c: &([f32; 2], [f32; 2], ShoreKind, Option<u32>, f32)| {
                        if shore == ShoreKind::Cliff {
                            -c.4
                        } else {
                            (c.0[0] + 90000.).hypot(c.0[1] + 25000.)
                        }
                    };
                    score(a).total_cmp(&score(b))
                })
                .expect("world needs dry sandy and cliff coastlines on both landmass types");
            scenes.push((
                name,
                selected.0,
                selected.1,
                if shore == ShoreKind::Cliff { 12. } else { 0. },
            ));
        }
        renderer.set_quality(1);
        renderer.resize(1280, 720);
        renderer.set_render_resolution(1);
        renderer.set_filter(1, 1.0);
        let mut views = Vec::new();
        for (name, p, normal, lift) in scenes {
            let eye = glam::Vec3::new(
                p[0],
                geometry::walk_height(world, p[0], p[1]) + 1.72 + lift,
                p[1],
            );
            let start = Instant::now();
            renderer.update_chunks(world, eye, true);
            while renderer.pending_count() > 0 {
                renderer.update_chunks(world, eye, false);
            }
            let along = glam::Vec2::new(normal[1], -normal[0]);
            let direction = along * 0.84 - glam::Vec2::from_array(normal) * 0.54;
            let yaw = direction.x.atan2(-direction.y);
            let pitch = if lift > 0. { -0.26 } else { -0.06 };
            renderer
                .render(eye, yaw, pitch, 11.0)
                .expect("render native coast");
            let pixels = renderer.capture_rgba().unwrap();
            assert_image(&pixels, 1280, 720, name);
            save_png(&format!("{dir}/{name}.png"), 1280, 720, &pixels);
            save_png(
                &format!("{dir}/{name}-map.png"),
                512,
                512,
                &world.map_rgba(p[0], p[1], 12000., 512),
            );
            views.push(serde_json::json!({"name":name,"eye":eye.to_array(),"yaw":yaw,"pitch":pitch,"landmass":world.landmass_id(p[0],p[1]),"shore":world.sample(p[0],p[1]).shore}));
            println!(
                "Coast {name}: eye{eye:?}, warmup{:?}, {} triangles",
                start.elapsed(),
                renderer.triangle_count()
            );
        }
        fs::write(format!("{dir}/coasts.json"),serde_json::to_string_pretty(&serde_json::json!({"seed":world.seed,"sizeMeters":WORLD_SIZE,"views":views,"hydrology":world.hydrology_stats(),"roads":world.road_stats()})).unwrap()).unwrap();
        println!("Coastal verification passed: actual atlas, beach, cliff and island GPU captures");
    }

    fn verify_grounding(world: &World, renderer: &mut Renderer, dir: &str) {
        const WIDTH: u32 = 1280;
        const HEIGHT: u32 = 720;
        // Resolve the stable site ID: road revisions may move its coordinates.
        // The compound layout retains its independently grounded tent offset.
        let reference = verification_region_site(world);
        let site = [reference.x, reference.z];
        let tent = [site[0] - 10.0, site[1] + 8.0];
        let ground_eye = |x, z| glam::Vec3::new(x, geometry::walk_height(world, x, z) + 1.72, z);
        let primary = ground_eye(site[0] - 24.0, site[1] + 26.0);
        let tent_target = glam::Vec3::new(
            tent[0],
            geometry::walk_height(world, tent[0], tent[1]) + 1.0,
            tent[1],
        );
        let site_target = glam::Vec3::new(
            site[0],
            geometry::walk_height(world, site[0], site[1]) + 3.0,
            site[1],
        );
        renderer.set_quality(1);
        renderer.resize(WIDTH, HEIGHT);
        renderer.set_render_resolution(1);
        renderer.set_filter(0, 1.0);
        // All viewpoints fit the warmed finest-terrain ring, even if the moved
        // site straddles a chunk edge. Reuse the same meshes for every view.
        let start = Instant::now();
        renderer.update_chunks(world, primary, true);
        while renderer.pending_count() > 0 {
            renderer.update_chunks(world, primary, false);
        }
        println!(
            "Grounding warmup: {} chunks, {} triangles, {:?}",
            renderer.chunk_count(),
            renderer.triangle_count(),
            start.elapsed()
        );
        let scenes = [
            ("mistfield-repro", primary, tent_target, Some(0.665_f32)),
            (
                "mistfield-downslope",
                ground_eye(site[0] - 25.0, site[1] + 30.0),
                tent_target,
                None,
            ),
            (
                "mistfield-side",
                ground_eye(site[0] + 16.0, site[1] + 24.0),
                tent_target,
                None,
            ),
            (
                "mistfield-foundations",
                primary + glam::Vec3::Y * 32.0,
                site_target,
                None,
            ),
        ];
        let mut views = Vec::new();
        for (name, eye, target, fixed_yaw) in scenes {
            assert!(
                (eye.x - primary.x).abs() < 96.0 && (eye.z - primary.z).abs() < 96.0,
                "grounding view left the warmed finest-terrain neighborhood"
            );
            let direction = target - eye;
            let yaw = fixed_yaw.unwrap_or_else(|| direction.x.atan2(-direction.z));
            let pitch = direction.y.atan2(direction.x.hypot(direction.z));
            renderer
                .render(eye, yaw, pitch, 11.0)
                .expect("render grounding scene");
            let pixels = renderer.capture_rgba().expect("read grounding capture");
            assert_image(&pixels, WIDTH, HEIGHT, name);
            let file = format!("{dir}/grounding-{name}.png");
            save_png(&file, WIDTH, HEIGHT, &pixels);
            println!("Grounding {name}: eye{eye:?} yaw{yaw:.4} pitch{pitch:.4} -> {file}");
            views.push(serde_json::json!({
                "name":name,"file":file,"eye":eye.to_array(),"yaw":yaw,
                "pitch":pitch,"hour":11.0,"opaqueAndNonempty":true,
            }));
        }
        fs::write(
            format!("{dir}/grounding-verification.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "seed":world.seed,"regressionSeed":1337,"site":reference.name,"siteId":reference.id,
                "sitePosition":site,"tentPosition":tent,"views":views,
                "captureDimensions":[WIDTH,HEIGHT],"sceneResolution":"native",
                "filter":"clean","warmupCount":1,
                "chunkCount":renderer.chunk_count(),"triangleCount":renderer.triangle_count(),
                "appearanceNote":"Inspect tent hems, campfire stones, bench feet, tower foundations, signpost and banner bases. Opaque/nonempty GPU checks do not prove ground contact; geometry tests validate support vertices and LOD equality."
            })).unwrap(),
        ).unwrap();
        println!(
            "Grounding capture passed: 4 {} views, native 1280x720, 11:00, one warmup",
            reference.name
        );
    }

    fn verify_filters(
        world: &World,
        renderer: &mut Renderer,
        eye: glam::Vec3,
        yaw: f32,
        dir: &str,
    ) {
        const WIDTH: u32 = 1280;
        const HEIGHT: u32 = 720;
        let mut report = Vec::new();
        renderer.render(eye, yaw, -0.10, 14.0).unwrap();
        let default_frame = renderer.capture_rgba().unwrap();
        // No advance_time calls: each mode sees identical water, clouds, foliage,
        // camera and lighting. The warmed world is reused for every comparison.
        for (name, camera, pitch, hour) in [
            ("day", eye, -0.10, 14.0),
            ("elevated", eye + glam::Vec3::Y * 100.0, -0.25, 14.0),
            ("night", eye, 0.24, 22.0),
        ] {
            let mut frames = Vec::with_capacity(3);
            for (mode, label) in [(0, "clean"), (1, "bloom"), (2, "crt")] {
                renderer.set_filter(mode, 1.0);
                renderer.render(camera, yaw, pitch, hour).unwrap();
                let pixels = renderer.capture_rgba().unwrap();
                assert_image(&pixels, WIDTH, HEIGHT, &format!("{name}/{label}"));
                save_png(
                    &format!("{dir}/filter-{name}-{label}.png"),
                    WIDTH,
                    HEIGHT,
                    &pixels,
                );
                frames.push(pixels);
            }
            let bloom = compare_images(&frames[0], &frames[1], WIDTH, HEIGHT);
            let crt = compare_images(&frames[0], &frames[2], WIDTH, HEIGHT);
            if name != "night" {
                assert!(
                    bloom.mean_absolute_rgb > 0.03 && bloom.changed_fraction > 0.001,
                    "{name}: bloom must visibly affect highlights, metrics {bloom:?}"
                );
            }
            // A dark sky with no threshold highlights can legitimately produce
            // no bloom; retain its metrics and image without requiring a glow.
            if name == "day" {
                assert_pixels_equal(
                    &default_frame,
                    &frames[1],
                    "default filter must be Bloom at strength one",
                );
            }
            assert!(
                bloom.shadow_lift < 12.0,
                "{name}: bloom washes out the darkest fifth of the image: {bloom:?}"
            );
            assert!(
                crt.mean_absolute_rgb > 1.0 && crt.changed_fraction > 0.20,
                "{name}: CRT must visibly change the image, metrics {crt:?}"
            );
            assert!(
                crt.corner_dark_gain > 0.01,
                "{name}: CRT curvature should expose opaque dark corner borders: {crt:?}"
            );
            // A stationary CRT image must remain stable across repeated submits:
            // scanlines, phosphor mask and curvature are not fresh random noise.
            for repeat in 0..3 {
                renderer.render(camera, yaw, pitch, hour).unwrap();
                assert_pixels_equal(
                    &frames[2],
                    &renderer.capture_rgba().unwrap(),
                    &format!("{name}: CRT repeat {repeat} flickered with unchanged inputs"),
                );
            }
            for mode in [1, 2] {
                renderer.set_filter(mode, 0.0);
                renderer.render(camera, yaw, pitch, hour).unwrap();
                assert_pixels_equal(
                    &frames[0],
                    &renderer.capture_rgba().unwrap(),
                    &format!("{name}: mode {mode} at strength zero must equal Clean exactly"),
                );
            }
            println!("Filters {name}: bloom {bloom:?}; CRT {crt:?}; zero-strength and repeated-frame checks passed");
            report.push(serde_json::json!({
                "scene":name,
                "bloom":bloom.json(),
                "crt":crt.json(),
                "bloomVisibleEffectRequired":name != "night",
                "zeroStrengthPixelExact":true,
                "crtRepeatedFramesPixelExact":true,
            }));
        }
        // Exercise public setting semantics against actual GPU output.
        renderer.set_filter(1, 1.0);
        renderer.render(eye, yaw, -0.10, 14.0).unwrap();
        let bloom = renderer.capture_rgba().unwrap();
        for mode in [3, u32::MAX] {
            renderer.set_filter(mode, 1.0);
            renderer.render(eye, yaw, -0.10, 14.0).unwrap();
            assert_pixels_equal(
                &bloom,
                &renderer.capture_rgba().unwrap(),
                &format!("invalid mode {mode} must fall back to Bloom"),
            );
        }
        for mode in [1, 2] {
            renderer.set_filter(0, 1.0);
            renderer.render(eye, yaw, -0.10, 14.0).unwrap();
            let clean = renderer.capture_rgba().unwrap();
            renderer.set_filter(mode, -10.0);
            renderer.render(eye, yaw, -0.10, 14.0).unwrap();
            assert_pixels_equal(
                &clean,
                &renderer.capture_rgba().unwrap(),
                "negative strength must clamp to zero",
            );
            renderer.set_filter(mode, 1.5);
            renderer.render(eye, yaw, -0.10, 14.0).unwrap();
            let maximum = renderer.capture_rgba().unwrap();
            renderer.set_filter(mode, 100.0);
            renderer.render(eye, yaw, -0.10, 14.0).unwrap();
            assert_pixels_equal(
                &maximum,
                &renderer.capture_rgba().unwrap(),
                "strength must clamp to 1.5",
            );
        }
        let mut qualities = Vec::new();
        // Reuse the warmed meshes: these are render-target/quality smoke checks,
        // not another world-streaming benchmark. Odd dimensions exercise padded
        // GPU readback after resize without generating a huge high-quality ring.
        for (quality, width, height) in [(2, 1279, 719), (1, 1024, 640), (0, 959, 539)] {
            renderer.set_quality(quality);
            renderer.resize(width, height);
            let mut clean = Vec::new();
            for (mode, label) in [(0, "clean"), (1, "bloom"), (2, "crt")] {
                renderer.set_filter(mode, 1.0);
                renderer.render(eye, yaw, -0.10, 14.0).unwrap();
                let pixels = renderer.capture_rgba().unwrap();
                assert_image(
                    &pixels,
                    width,
                    height,
                    &format!("quality {quality}/{label}"),
                );
                if mode == 0 {
                    clean = pixels;
                } else {
                    assert!(
                        compare_images(&clean, &pixels, width, height).changed_fraction > 0.001,
                        "mode {label} had no effect after resizing quality {quality}"
                    );
                    if mode == 2 {
                        save_png(
                            &format!("{dir}/filter-quality-{quality}-crt.png"),
                            width,
                            height,
                            &pixels,
                        );
                    }
                }
            }
            qualities.push(serde_json::json!({"quality":quality,"width":width,"height":height,"allModesRendered":true}));
        }
        let resolutions = verify_resolutions(renderer, eye, yaw, dir);
        fs::write(format!("{dir}/filter-verification.json"), serde_json::to_string_pretty(&serde_json::json!({
            "seed":world.seed,"scenes":report,"qualities":qualities,"resolutionChecks":resolutions,
            "defaultIsBloomStrengthOne":true,"qualityChecksReuseWarmedMeshes":true,
            "invalidModeFallsBackToBloom":[3,u32::MAX],"strengthClampVerified":[0.0,1.5],
            "appearanceNote":"Numeric metrics check visible effect and shadow preservation; PNGs provide visual proof of glow, scanlines, mask and curvature."
        })).unwrap()).unwrap();
        println!("GPU filter verification passed: 3 scenes, 3 modes, exact bypass, stable CRT, setting clamps, 6 resolutions, independent quality and resize persistence");
    }

    fn verify_resolutions(
        renderer: &mut Renderer,
        eye: glam::Vec3,
        yaw: f32,
        dir: &str,
    ) -> serde_json::Value {
        const WIDTH: u32 = 1280;
        const HEIGHT: u32 = 720;
        renderer.set_quality(1);
        renderer.resize(WIDTH, HEIGHT);
        renderer.set_filter(1, 1.0);
        // Keep the warmed world, camera and simulation time fixed while changing
        // only the internal render target. Output resolution remains unchanged.
        let mut resolutions = Vec::new();
        for (setting, label) in [
            (120, "120"),
            (240, "240"),
            (450, "450"),
            (720, "720"),
            (1, "native"),
            (0, "auto"),
        ] {
            renderer.set_render_resolution(setting);
            let dimensions = assert_render_resolution(renderer, WIDTH, HEIGHT, 1, setting);
            let pixels = filter_capture(renderer, eye, yaw, WIDTH, HEIGHT, label);
            save_png(
                &format!("{dir}/filter-resolution-{label}.png"),
                WIDTH,
                HEIGHT,
                &pixels,
            );
            resolutions.push(serde_json::json!({
                "setting":label,"sceneDimensions":dimensions,
                "captureDimensions":[WIDTH,HEIGHT],
            }));
        }
        let mut qualities = Vec::new();
        // Changing world quality must not override an explicit scene resolution.
        // Rendering is enough here; warmed meshes remain available throughout.
        renderer.set_render_resolution(240);
        for quality in 0..=2 {
            renderer.set_quality(quality);
            let fixed = assert_render_resolution(renderer, WIDTH, HEIGHT, quality, 240);
            filter_capture(
                renderer,
                eye,
                yaw,
                WIDTH,
                HEIGHT,
                "fixed resolution quality change",
            );
            renderer.set_render_resolution(0);
            let automatic = assert_render_resolution(renderer, WIDTH, HEIGHT, quality, 0);
            filter_capture(
                renderer,
                eye,
                yaw,
                WIDTH,
                HEIGHT,
                "automatic resolution quality change",
            );
            qualities.push(serde_json::json!({
                "quality":quality,"explicit240":fixed,"automatic":automatic,
            }));
            renderer.set_render_resolution(240);
        }
        // Preserve the selected filter, strength and scene resolution through
        // target recreation, including odd dimensions and padded GPU readback.
        renderer.set_quality(1);
        renderer.set_filter(2, 0.75);
        let before_resize = filter_capture(renderer, eye, yaw, WIDTH, HEIGHT, "before resize");
        renderer.resize(959, 539);
        let resized = assert_render_resolution(renderer, 959, 539, 1, 240);
        let pixels = filter_capture(renderer, eye, yaw, 959, 539, "odd resized CRT");
        save_png(
            &format!("{dir}/filter-resized-959x539.png"),
            959,
            539,
            &pixels,
        );
        renderer.resize(WIDTH, HEIGHT);
        assert_render_resolution(renderer, WIDTH, HEIGHT, 1, 240);
        assert_pixels_equal(
            &before_resize,
            &filter_capture(renderer, eye, yaw, WIDTH, HEIGHT, "restored resize"),
            "resizing away and back must preserve filter and resolution settings",
        );
        renderer.set_filter(1, 1.0);
        renderer.set_render_resolution(1);
        renderer.resize(1001, 563);
        let native_resized = assert_render_resolution(renderer, 1001, 563, 1, 1);
        filter_capture(renderer, eye, yaw, 1001, 563, "native resized Bloom");
        renderer.set_quality(0);
        assert_render_resolution(renderer, 1001, 563, 0, 1);
        renderer.set_render_resolution(0);
        renderer.set_quality(2);
        let capped_auto = assert_render_resolution(renderer, 1001, 563, 2, 0);
        filter_capture(renderer, eye, yaw, 1001, 563, "automatic viewport cap");
        renderer.set_render_resolution(2);
        let minimum = assert_render_resolution(renderer, 1001, 563, 2, 2);
        filter_capture(renderer, eye, yaw, 1001, 563, "minimum scene resolution");
        serde_json::json!({
            "resolutions":resolutions,"qualities":qualities,
            "resizedExplicit240":resized,"resizedNative":native_resized,
            "autoCappedToViewport":capped_auto,"minimumResolutionClamp":minimum,
            "allCapturesOpaqueAndNonempty":true,
            "resizePreservesSettingsPixelExact":true,"checksReuseWarmedMeshes":true,
        })
    }

    fn assert_render_resolution(
        renderer: &Renderer,
        width: u32,
        height: u32,
        quality: u32,
        setting: u32,
    ) -> [u32; 2] {
        let actual = renderer.render_resolution();
        let expected_height = match setting {
            0 => [270, 450, 720][quality as usize].min(height),
            1 => height,
            value => value.clamp(90, 2160),
        };
        // All verification viewports keep the requested dimensions below the
        // 4096 texture cap; tolerate the final integer width rounding only.
        assert_eq!(
            actual[1], expected_height,
            "wrong scene height for setting {setting}, quality {quality}"
        );
        let expected_width = width as f64 * expected_height as f64 / height as f64;
        assert!(
            (actual[0] as f64 - expected_width).abs() <= 1.0,
            "scene aspect ratio changed: output {width}x{height}, scene {actual:?}"
        );
        assert!(actual.iter().all(|v| (1..=4096).contains(v)));
        if setting == 1 {
            assert_eq!(
                actual,
                [width, height],
                "Native must match output dimensions exactly"
            );
        }
        actual
    }

    fn filter_capture(
        renderer: &mut Renderer,
        eye: glam::Vec3,
        yaw: f32,
        width: u32,
        height: u32,
        label: &str,
    ) -> Vec<u8> {
        renderer.render(eye, yaw, -0.10, 14.0).unwrap();
        let pixels = renderer.capture_rgba().unwrap();
        assert_image(&pixels, width, height, label);
        pixels
    }

    fn assert_image(pixels: &[u8], width: u32, height: u32, label: &str) {
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * 4,
            "{label}: wrong capture dimensions"
        );
        assert!(
            pixels.chunks_exact(4).all(|p| p[3] == 255),
            "{label}: capture is not fully opaque"
        );
        let colors: std::collections::HashSet<_> =
            pixels.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
        assert!(
            colors.len() > 64,
            "{label}: empty or near-solid capture ({} colors)",
            colors.len()
        );
        let visible = pixels
            .chunks_exact(4)
            .filter(|p| p[0].max(p[1]).max(p[2]) > 12)
            .count();
        assert!(
            visible > (width as usize * height as usize) / 10,
            "{label}: capture is predominantly empty"
        );
    }
    fn assert_pixels_equal(a: &[u8], b: &[u8], label: &str) {
        assert_eq!(a.len(), b.len(), "{label}: different dimensions");
        let changed = a
            .chunks_exact(4)
            .zip(b.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(changed, 0, "{label}: {changed} pixels changed");
    }
    fn luminance(p: &[u8]) -> f64 {
        p[0] as f64 * 0.2126 + p[1] as f64 * 0.7152 + p[2] as f64 * 0.0722
    }
    #[derive(Debug)]
    struct FilterDifference {
        mean_absolute_rgb: f64,
        changed_fraction: f64,
        shadow_lift: f64,
        corner_dark_gain: f64,
        row_modulation: f64,
    }
    impl FilterDifference {
        fn json(&self) -> serde_json::Value {
            serde_json::json!({
                "meanAbsoluteRgbByteDifference":self.mean_absolute_rgb,
                "changedPixelFraction":self.changed_fraction,
                "darkestFifthMeanLuminanceLift":self.shadow_lift,
                "cornerDarkFractionGain":self.corner_dark_gain,
                "rowModulationDiagnostic":self.row_modulation,
            })
        }
    }
    fn compare_images(clean: &[u8], filtered: &[u8], width: u32, height: u32) -> FilterDifference {
        assert_eq!(clean.len(), filtered.len());
        let count = clean.len() / 4;
        let mut histogram = [0usize; 256];
        for p in clean.chunks_exact(4) {
            histogram[luminance(p).round().clamp(0., 255.) as usize] += 1;
        }
        let mut seen = 0;
        let shadow_cutoff = histogram
            .iter()
            .position(|n| {
                seen += n;
                seen >= count / 5
            })
            .unwrap_or(64) as f64;
        let mut absolute = 0u64;
        let mut changed = 0usize;
        let mut shadow_lift = 0.0;
        let mut shadow_count = 0usize;
        let mut corner_dark_delta = 0i64;
        let mut corner_count = 0usize;
        let mut rows = vec![(0.0, 0usize); height as usize];
        for (index, (a, b)) in clean
            .chunks_exact(4)
            .zip(filtered.chunks_exact(4))
            .enumerate()
        {
            let difference = (0..3).map(|c| a[c].abs_diff(b[c]) as u64).sum::<u64>();
            absolute += difference;
            if difference > 0 {
                changed += 1;
            }
            let la = luminance(a);
            let lb = luminance(b);
            if la <= shadow_cutoff {
                shadow_lift += (lb - la).max(0.0);
                shadow_count += 1;
            }
            let x = index % width as usize;
            let y = index / width as usize;
            if (x < width as usize / 10 || x >= width as usize * 9 / 10)
                && (y < height as usize / 10 || y >= height as usize * 9 / 10)
            {
                corner_dark_delta += i64::from(lb < 8.0) - i64::from(la < 8.0);
                corner_count += 1;
            }
            if x > width as usize / 4 && x < width as usize * 3 / 4 && la > 20.0 {
                rows[y].0 += lb / la;
                rows[y].1 += 1;
            }
        }
        let rows: Vec<f64> = rows
            .iter()
            .map(|(sum, n)| sum / (*n).max(1) as f64)
            .collect();
        let row_modulation = rows
            .windows(3)
            .map(|r| (r[1] - (r[0] + r[2]) * 0.5).abs())
            .sum::<f64>()
            / (height as usize).saturating_sub(2).max(1) as f64;
        FilterDifference {
            mean_absolute_rgb: absolute as f64 / (count * 3) as f64,
            changed_fraction: changed as f64 / count as f64,
            shadow_lift: shadow_lift / shadow_count.max(1) as f64,
            corner_dark_gain: corner_dark_delta as f64 / corner_count.max(1) as f64,
            row_modulation,
        }
    }

    fn save_png(path: &str, width: u32, height: u32, rgba: &[u8]) {
        let file = BufWriter::new(File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(rgba)
            .unwrap();
    }
}
#[cfg(target_arch = "wasm32")]
fn main() {}
