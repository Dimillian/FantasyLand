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
    let filters_only = check.as_deref() == Some("filters");
    let ascii_only = check.as_deref() == Some("ascii");
    let grounding_only = check.as_deref() == Some("grounding");
    let roads_only = check.as_deref() == Some("roads");
    let generation_time = Instant::now();
    let world = World::new(seed);
    println!(
        "Hydrology generation {:?}: {:?}",
        generation_time.elapsed(),
        world.hydrology_stats()
    );
    let (spawn, yaw) = world.spawn_view();
    println!(
        "World: {} x {} km, seed {}, spawn {:?}",
        WORLD_SIZE / 1000.,
        WORLD_SIZE / 1000.,
        world.seed,
        spawn
    );
    if !filters_only && !ascii_only && !grounding_only && !roads_only {
        let map_time = Instant::now();
        let map = world.map_rgba(0., 0., WORLD_SIZE, 512);
        save_png(&format!("{dir}/world-map.png"), 512, 512, &map);
        println!("512px world map: {:?}", map_time.elapsed());
        let local = world.map_rgba(spawn[0], spawn[1], 6000., 512);
        save_png(&format!("{dir}/local-map.png"), 512, 512, &local);
    }
    let mut renderer =
        pollster::block_on(Renderer::headless(1280, 720)).expect("create native wgpu renderer");
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
    if ascii_only {
        verify_ascii(&world, &mut renderer, eye, yaw, &dir);
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
        renderer.set_filter(u32::MAX, 1.0);
        renderer.render(eye, yaw, -0.10, 14.0).unwrap();
        assert_pixels_equal(
            &bloom,
            &renderer.capture_rgba().unwrap(),
            "invalid mode must fall back to Bloom",
        );
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
        fs::write(format!("{dir}/filter-verification.json"), serde_json::to_string_pretty(&serde_json::json!({
            "seed":world.seed,"scenes":report,"qualities":qualities,
            "defaultIsBloomStrengthOne":true,"qualityChecksReuseWarmedMeshes":true,
            "invalidModeFallsBackToBloom":true,"strengthClampVerified":[0.0,1.5],
            "appearanceNote":"Numeric metrics check visible effect and shadow preservation; PNGs provide visual proof of glow, scanlines, mask and curvature."
        })).unwrap()).unwrap();
        println!("GPU filter verification passed: 3 scenes, 3 modes, exact bypass, stable CRT, setting clamps and all qualities after resize");
    }

    fn verify_ascii(world: &World, renderer: &mut Renderer, eye: glam::Vec3, yaw: f32, dir: &str) {
        const WIDTH: u32 = 1280;
        const HEIGHT: u32 = 720;
        renderer.set_quality(1);
        renderer.resize(WIDTH, HEIGHT);
        renderer.set_render_resolution(0);
        renderer.set_filter(0, 1.0);
        let clean = ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, "Clean reference");
        save_png(
            &format!("{dir}/ascii-clean-reference.png"),
            WIDTH,
            HEIGHT,
            &clean,
        );
        let mut palettes = Vec::new();
        let mut scene = Vec::new();
        // Keep the camera, lighting and simulation time fixed. In particular,
        // never regenerate chunks while changing a postprocess or target size.
        renderer.set_filter(3, 1.0);
        for (palette, label) in [(0, "scene"), (1, "amber"), (2, "green")] {
            renderer.set_ascii(2, palette);
            let pixels = ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, label);
            let difference = compare_images(&clean, &pixels, WIDTH, HEIGHT);
            assert!(
                difference.changed_fraction > 0.30 && difference.mean_absolute_rgb > 1.0,
                "{label}: ASCII must replace the scene with a visible glyph rendering: {difference:?}"
            );
            // Check both halves so a filter limited to sky or a small HUD region
            // cannot pass the whole-frame comparison.
            let half = (WIDTH * HEIGHT * 2) as usize;
            for (part, a, b) in [
                ("top", &clean[..half], &pixels[..half]),
                ("bottom", &clean[half..], &pixels[half..]),
            ] {
                assert!(
                    compare_images(a, b, WIDTH, HEIGHT / 2).changed_fraction > 0.20,
                    "{label}: the {part} half was not converted to ASCII"
                );
            }
            let channels = ascii_palette_metrics(&pixels, palette, label);
            save_png(
                &format!("{dir}/ascii-palette-{label}.png"),
                WIDTH,
                HEIGHT,
                &pixels,
            );
            palettes.push(serde_json::json!({
                "palette":label,"differenceFromClean":difference.json(),"channels":channels,
            }));
            if palette == 0 {
                scene = pixels;
            }
        }
        renderer.set_ascii(2, 0);
        for strength in [0.0, 0.4, 1.5] {
            renderer.set_filter(3, strength);
            assert_pixels_equal(
                &scene,
                &ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, "ASCII ignores strength"),
                &format!("ASCII must remain full glyph rendering at strength {strength}"),
            );
        }
        for repeat in 0..3 {
            assert_pixels_equal(
                &scene,
                &ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, "stable ASCII frame"),
                &format!("ASCII repeat {repeat} flickered with unchanged inputs"),
            );
        }
        let mut sizes = Vec::new();
        let mut small = Vec::new();
        let mut large = Vec::new();
        for scale in 1..=3 {
            renderer.set_ascii(scale, 0);
            let pixels = ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, "glyph size");
            if scale != 2 {
                let difference = compare_images(&scene, &pixels, WIDTH, HEIGHT);
                assert!(
                    difference.changed_fraction > 0.05 && difference.mean_absolute_rgb > 0.25,
                    "glyph scale {scale} must visibly differ from scale 2: {difference:?}"
                );
            }
            save_png(
                &format!("{dir}/ascii-glyph-size-{scale}.png"),
                WIDTH,
                HEIGHT,
                &pixels,
            );
            sizes.push(serde_json::json!({"scale":scale,"opaqueAndNonempty":true}));
            if scale == 1 {
                small = pixels;
            } else if scale == 3 {
                large = pixels;
            } else {
                assert_pixels_equal(&scene, &pixels, "restoring the scene palette and scale");
            }
        }
        for (scale, expected) in [(0, &small), (u32::MAX, &large)] {
            renderer.set_ascii(scale, 0);
            assert_pixels_equal(
                expected,
                &ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, "clamped glyph size"),
                "glyph scale must clamp to 1..3",
            );
        }
        renderer.set_ascii(2, u32::MAX);
        assert_pixels_equal(
            &scene,
            &ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, "invalid palette"),
            "invalid ASCII palette must fall back to scene colors",
        );

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
            let pixels = ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, label);
            save_png(
                &format!("{dir}/ascii-resolution-{label}.png"),
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
            ascii_capture(
                renderer,
                eye,
                yaw,
                WIDTH,
                HEIGHT,
                "fixed resolution quality change",
            );
            renderer.set_render_resolution(0);
            let automatic = assert_render_resolution(renderer, WIDTH, HEIGHT, quality, 0);
            ascii_capture(
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
        // Preserve the selected palette, glyph scale, filter and scene resolution
        // through target recreation, including odd dimensions and row padding.
        renderer.set_quality(1);
        renderer.set_ascii(3, 1);
        let before_resize = ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, "before resize");
        renderer.resize(959, 539);
        let resized = assert_render_resolution(renderer, 959, 539, 1, 240);
        let pixels = ascii_capture(renderer, eye, yaw, 959, 539, "odd resized ASCII");
        ascii_palette_metrics(&pixels, 1, "amber after resize");
        save_png(
            &format!("{dir}/ascii-resized-959x539.png"),
            959,
            539,
            &pixels,
        );
        renderer.resize(WIDTH, HEIGHT);
        assert_render_resolution(renderer, WIDTH, HEIGHT, 1, 240);
        assert_pixels_equal(
            &before_resize,
            &ascii_capture(renderer, eye, yaw, WIDTH, HEIGHT, "restored resize"),
            "resizing away and back must preserve ASCII and resolution settings",
        );
        renderer.set_render_resolution(1);
        renderer.resize(1001, 563);
        let native_resized = assert_render_resolution(renderer, 1001, 563, 1, 1);
        ascii_capture(renderer, eye, yaw, 1001, 563, "native resized ASCII");
        renderer.set_quality(0);
        assert_render_resolution(renderer, 1001, 563, 0, 1);
        renderer.set_render_resolution(0);
        renderer.set_quality(2);
        let capped_auto = assert_render_resolution(renderer, 1001, 563, 2, 0);
        ascii_capture(renderer, eye, yaw, 1001, 563, "automatic viewport cap");
        renderer.set_render_resolution(2);
        let minimum = assert_render_resolution(renderer, 1001, 563, 2, 2);
        ascii_capture(renderer, eye, yaw, 1001, 563, "minimum scene resolution");
        fs::write(
            format!("{dir}/ascii-verification.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "seed":world.seed,"palettes":palettes,"glyphSizes":sizes,
                "resolutions":resolutions,"qualities":qualities,
                "resizedExplicit240":resized,"resizedNative":native_resized,
                "autoCappedToViewport":capped_auto,"minimumResolutionClamp":minimum,
                "allCapturesOpaqueAndNonempty":true,"asciiIgnoresStrength":true,
                "repeatedFramesPixelExact":true,"resizePreservesSettingsPixelExact":true,
                "glyphSizeClampVerified":[1,3],"invalidPaletteFallsBackToScene":true,
                "checksReuseWarmedMeshes":true,
                "appearanceNote":"Metrics verify replacement, palettes and determinism; PNGs require visual inspection for actual glyph readability."
            })).unwrap(),
        ).unwrap();
        println!("GPU ASCII verification passed: 3 palettes, 3 glyph sizes, 6 resolutions, independent quality, resize persistence and stable full-strength glyphs");
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

    fn ascii_capture(
        renderer: &mut Renderer,
        eye: glam::Vec3,
        yaw: f32,
        width: u32,
        height: u32,
        label: &str,
    ) -> Vec<u8> {
        renderer.render(eye, yaw, -0.10, 14.0).unwrap();
        let pixels = renderer.capture_rgba().unwrap();
        assert_eq!(
            pixels.len(),
            width as usize * height as usize * 4,
            "{label}: wrong capture dimensions"
        );
        assert!(
            pixels.chunks_exact(4).all(|p| p[3] == 255),
            "{label}: nonopaque pixels"
        );
        let colors: std::collections::HashSet<_> =
            pixels.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
        // A monochrome glyph palette can intentionally contain far fewer colors
        // than a shaded Clean/CRT image, but it must still contain useful detail.
        assert!(
            colors.len() >= 8,
            "{label}: empty or near-solid ASCII capture ({} colors)",
            colors.len()
        );
        let visible = pixels
            .chunks_exact(4)
            .filter(|p| p[0].max(p[1]).max(p[2]) > 16)
            .count();
        assert!(
            visible > width as usize * height as usize / 100,
            "{label}: missing visible glyphs"
        );
        pixels
    }

    fn ascii_palette_metrics(pixels: &[u8], palette: u32, label: &str) -> serde_json::Value {
        let mut sums = [0u64; 3];
        let mut visible = 0usize;
        let mut matching = 0usize;
        for p in pixels
            .chunks_exact(4)
            .filter(|p| p[0].max(p[1]).max(p[2]) > 16)
        {
            visible += 1;
            for c in 0..3 {
                sums[c] += p[c] as u64;
            }
            matching += usize::from(match palette {
                1 => p[0] >= p[1] && p[1] >= p[2],
                2 => p[1] >= p[0] && p[1] >= p[2],
                _ => true,
            });
        }
        assert!(visible > 0, "{label}: no visible palette samples");
        let mean = sums.map(|sum| sum as f64 / visible as f64);
        let matching_fraction = matching as f64 / visible as f64;
        if palette == 1 {
            assert!(
                matching_fraction > 0.95 && mean[0] > mean[1] * 1.08 && mean[1] > mean[2] * 1.10,
                "{label}: expected warm amber RGB ordering, got {mean:?}, matching {matching_fraction}"
            );
        } else if palette == 2 {
            assert!(
                matching_fraction > 0.95 && mean[1] > mean[0] * 1.20 && mean[1] > mean[2] * 1.20,
                "{label}: expected a green monochrome palette, got {mean:?}, matching {matching_fraction}"
            );
        }
        serde_json::json!({"visiblePixels":visible,"meanVisibleRgb":mean,"paletteMatchingFraction":matching_fraction})
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
