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
    let world = World::new(1337);
    let (spawn, yaw) = world.spawn_view();
    println!(
        "World: {} x {} km, seed {}, spawn {:?}",
        WORLD_SIZE / 1000.,
        WORLD_SIZE / 1000.,
        world.seed,
        spawn
    );
    let map_time = Instant::now();
    let map = world.map_rgba(0., 0., WORLD_SIZE, 512);
    save_png(&format!("{dir}/world-map.png"), 512, 512, &map);
    println!("512px world map: {:?}", map_time.elapsed());
    let local = world.map_rgba(spawn[0], spawn[1], 6000., 512);
    save_png(&format!("{dir}/local-map.png"), 512, 512, &local);
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
    let metadata = serde_json::json!({"seed":world.seed,"sizeMeters":WORLD_SIZE,"spawn":spawn,"sites":world.sites_near(spawn[0],spawn[1],6000.),"landmarks":world.landmarks_near(spawn[0],spawn[1],3000.),"chunkCount":renderer.chunk_count(),"triangleCount":renderer.triangle_count()});
    fs::write(
        format!("{dir}/world.json"),
        serde_json::to_string_pretty(&metadata).unwrap(),
    )
    .unwrap();
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
