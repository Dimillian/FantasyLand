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
    let filters_only = std::env::args().nth(3).as_deref() == Some("filters");
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
    if !filters_only {
        let map_time = Instant::now();
        let map = world.map_rgba(0., 0., WORLD_SIZE, 512);
        save_png(&format!("{dir}/world-map.png"), 512, 512, &map);
        println!("512px world map: {:?}", map_time.elapsed());
        let local = world.map_rgba(spawn[0], spawn[1], 6000., 512);
        save_png(&format!("{dir}/local-map.png"), 512, 512, &local);
    }
    let mut renderer =
        pollster::block_on(Renderer::headless(1280, 720)).expect("create native wgpu renderer");
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
