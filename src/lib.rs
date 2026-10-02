mod antialias;
pub mod ao;
pub mod atlas;
pub mod celestial;
pub mod citizens;
pub mod climate;
mod cloud_shadow;
pub mod countryside;
pub mod cover;
pub mod ecology;
pub mod exploration;
pub mod geography;
pub mod geometry;
pub mod gi;
pub mod gi_screen;
mod gpu_profile;
pub mod habitat;
mod horizon;
pub mod interior_design;
pub mod journeys;
pub mod materials;
pub mod meadow;
pub mod natural;
pub mod people_sprites;
mod plants;
mod player;
mod postprocess;
pub mod precipitation;
mod rays;
pub mod regional_tour;
pub mod regions;
pub mod renderer;
pub mod settlement_mesh;
pub mod settlements;
mod shadow;
pub mod soundscape;
pub mod streaming;
pub mod traversal;
mod vertex;
pub mod water_sim;
pub mod waterfalls;
pub mod weather;
pub mod wind;
pub mod world;

use player::Player;
use renderer::Renderer;
use serde::Serialize;
use wasm_bindgen::prelude::*;
use world::World;

#[wasm_bindgen]
pub struct Game {
    world: World,
    player: Player,
    renderer: Renderer,
    gpu_drain: std::sync::Arc<std::sync::atomic::AtomicBool>,
    hour: f32,
    life: citizens::Life,
    audio_probe: std::cell::RefCell<soundscape::Probe>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GameState {
    x: f32,
    y: f32,
    z: f32,
    yaw: f32,
    pitch: f32,
    biome: String,
    landscape: String,
    forest: Option<String>,
    grounded: bool,
    stamina: f32,
    health: f32,
    mana: f32,
    walked: f32,
    seed: u32,
    speed: f32,
    altitude: f32,
    camera_position: [f32; 3],
    site_name: String,
    day_time: f32,
    world_clock: f64,
    nearby_people: usize,
    interaction: String,
    chunk_count: usize,
    triangle_count: usize,
    ground_cover_density: f32,
    sun_shadows: bool,
    cover_instances: u32,
    mesh_megabytes: f32,
    weather: weather::WeatherState,
    audio: soundscape::Environment,
    reflection_draws: u32,
    gpu_timings: Vec<gpu_profile::PassTime>,
    gpu_render_ms: Option<f32>,
    gpu_sample_id: u32,
    streaming_pending: usize,
    lighting_mode: u32,
    lighting_bytes: u64,
    gi_ready_fraction: f32,
}

#[wasm_bindgen]
impl Game {
    #[cfg(target_arch = "wasm32")]
    pub async fn create(canvas: web_sys::HtmlCanvasElement, seed: u32) -> Result<Game, JsValue> {
        console_error_panic_hook::set_once();
        let world = World::new(seed);
        let (spawn, yaw) = world.spawn_view();
        let mut player = Player::new(&world, spawn[0], spawn[1]);
        player.yaw = yaw;
        player.pitch = -0.10;
        let mut renderer = Renderer::browser(canvas)
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        renderer.update_chunks(&world, player.position, true);
        Ok(Self {
            world,
            player,
            renderer,
            gpu_drain: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            hour: 9.0,
            life: citizens::Life::new(),
            audio_probe: Default::default(),
        })
    }
    pub fn set_lighting_mode(&mut self, mask: u32) {
        self.renderer.set_lighting_mode(mask);
    }
    pub fn next_gi_job(&mut self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.renderer.next_gi_job()).unwrap_or(JsValue::NULL)
    }
    pub fn accept_gi_result(&mut self, ticket: u32, bytes: &[u8]) -> bool {
        self.renderer.accept_gi_result(ticket, bytes)
    }
    /// An asynchronous queue barrier for repeatable benchmark setup. Rendering
    /// remains nonblocking; the browser waits on RAF while the callback fires.
    pub fn begin_gpu_drain(&mut self) {
        self.gpu_drain
            .store(false, std::sync::atomic::Ordering::Release);
        let ready = self.gpu_drain.clone();
        self.renderer.queue.on_submitted_work_done(move || {
            ready.store(true, std::sync::atomic::Ordering::Release)
        });
    }
    pub fn gpu_drained(&self) -> bool {
        self.gpu_drain.load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn set_async_streaming(&mut self, enabled: bool) {
        self.renderer.set_async_streaming(enabled);
    }
    pub fn next_stream_job(&mut self) -> Vec<i32> {
        self.renderer.next_stream_job()
    }
    pub fn accept_stream_result(&mut self, ticket: u32, bytes: &[u8]) -> bool {
        self.renderer.accept_stream_result(ticket, bytes)
    }
    pub fn pending_chunks(&self) -> usize {
        self.renderer.pending_count()
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.player.look(dx, dy);
    }
    pub fn tick(
        &mut self,
        dt: f32,
        forward: f32,
        strafe: f32,
        sprint: bool,
        jump: bool,
    ) -> Result<(), JsValue> {
        let dt = if self.life.talking.is_some() {
            0.
        } else {
            dt.clamp(0.0, 0.05)
        };
        self.player
            .update(&self.world, dt, forward, strafe, sprint, jump);
        self.life
            .update(&self.world, self.player.eye().to_array(), dt);
        self.hour = (self.life.clock / 120. % 24.) as f32;
        self.renderer.advance_time(dt);
        self.renderer
            .update_weather(&self.world, self.player.eye(), self.hour, dt);
        self.renderer
            .update_chunks(&self.world, self.player.position, false);
        self.life.weather = self.renderer.weather_state().label.to_string();
        self.renderer
            .update_people(&self.world, &self.life, self.player.eye(), self.player.yaw);
        self.renderer
            .render(
                self.player.eye(),
                self.player.yaw,
                self.player.pitch,
                self.hour,
            )
            .map_err(|e| JsValue::from_str(&e))
    }
    /// Small audio packet matching the weather/event uniforms of the rendered
    /// frame. Avoids repeating full world/acoustic probes at frame frequency.
    pub fn lightning_audio_frame(&self) -> Vec<f32> {
        let w = self.renderer.weather_state();
        let event = self.renderer.lightning_audio_event();
        let p = self.player.eye().to_array();
        let source = if w.lightning > 0.035 {
            soundscape::thunder_source(p, event)
        } else {
            [0.0; 3]
        };
        vec![
            event[0],
            event[1],
            event[2],
            event[3],
            w.lightning,
            w.storm_strength,
            w.mode as f32,
            p[0],
            p[1],
            p[2],
            self.player.yaw,
            source[0],
            source[1],
            source[2],
        ]
    }
    pub fn teleport(&mut self, x: f32, z: f32) {
        self.life.talking = None;
        self.life.invalidate();
        if !x.is_finite() || !z.is_finite() {
            return;
        }
        let half = world::WORLD_SIZE * 0.5 - 40.0;
        self.player
            .teleport(&self.world, x.clamp(-half, half), z.clamp(-half, half));
        self.renderer.clear_chunks();
        self.renderer
            .update_chunks(&self.world, self.player.position, true);
    }
    pub fn face(&mut self, yaw: f32, pitch: f32) {
        if yaw.is_finite() && pitch.is_finite() {
            self.player.yaw = yaw.rem_euclid(std::f32::consts::TAU);
            self.player.pitch = pitch.clamp(-1.45, 1.45);
        }
    }
    pub fn state(&self) -> JsValue {
        let p = &self.player;
        let sample = self.world.sample(p.position.x, p.position.z);
        let nearest = self
            .world
            .settlements
            .layouts_near(&self.world, p.position.x, p.position.z, 0.)
            .into_iter()
            .filter(|l| {
                (l.entry.site.x - p.position.x).hypot(l.entry.site.z - p.position.z)
                    < l.entry.radius + 12.
            })
            .min_by(|a, b| {
                (a.entry.site.x - p.position.x)
                    .hypot(a.entry.site.z - p.position.z)
                    .total_cmp(
                        &(b.entry.site.x - p.position.x).hypot(b.entry.site.z - p.position.z),
                    )
            })
            .map(|l| l.entry.site.clone());
        let landmark = self
            .world
            .landmarks_near(p.position.x, p.position.z, 55.0)
            .into_iter()
            .next();
        let place_name = landmark
            .map(|s| s.name)
            .or_else(|| nearest.map(|s| s.name))
            .unwrap_or_else(|| {
                match sample.road_kind.filter(|_| sample.road > 0.2) {
                    Some(world::RoadKind::Main) => "The King's Road",
                    Some(world::RoadKind::Lane) => "Country Lane",
                    Some(world::RoadKind::Trail) => "Wilderness Trail",
                    None if sample.ocean => "The Open Sea",
                    None if sample.shore == world::ShoreKind::Beach => "Sandy Shore",
                    None if sample.shore == world::ShoreKind::Cliff => "Sea Cliffs",
                    None => "The Wilds",
                }
                .into()
            });
        serde_wasm_bindgen::to_value(&GameState {
            x: p.position.x,
            y: p.position.y,
            z: p.position.z,
            yaw: p.yaw,
            pitch: p.pitch,
            biome: sample.biome.name().to_string(),
            forest: (ecology::tree_density(self.world.seed, p.position.x, p.position.z, &sample)
                > 0.34)
                .then(|| {
                    ecology::forest_stand(self.world.seed, p.position.x, p.position.z, &sample)
                        .kind
                        .name()
                        .to_string()
                }),
            landscape: if matches!(
                sample.biome,
                world::Biome::Desert
                    | world::Biome::Swamp
                    | world::Biome::Savanna
                    | world::Biome::Jungle
                    | world::Biome::TropicalCoast
            ) && !sample.ocean
            {
                sample.biome.name().into()
            } else if sample.ocean {
                "Ocean".into()
            } else {
                regions::sample(self.world.seed, p.position.x, p.position.z, &sample)
                    .kind
                    .name()
                    .into()
            },
            grounded: p.grounded,
            stamina: p.stamina,
            health: p.health,
            mana: p.mana,
            walked: p.walked,
            seed: self.world.seed,
            speed: p.speed,
            altitude: p.position.y,
            camera_position: p.eye().to_array(),
            site_name: place_name,
            day_time: self.hour,
            world_clock: self.life.clock,
            nearby_people: self.life.actors.len(),
            interaction: self.interaction_label(),
            chunk_count: self.renderer.chunk_count(),
            triangle_count: self.renderer.triangle_count(),
            ground_cover_density: self.renderer.ground_cover_density(),
            sun_shadows: self.renderer.shadows_enabled(),
            cover_instances: self.renderer.cover_drawn_instances(),
            mesh_megabytes: self.renderer.mesh_bytes() as f32 / 1_000_000.0,
            weather: *self.renderer.weather_state(),
            audio: self.audio_probe.borrow_mut().sample(
                &self.world,
                p,
                &sample,
                self.renderer.weather_state(),
                self.life.clock,
            ),
            reflection_draws: self.renderer.reflection_draws(),
            gpu_timings: self.renderer.gpu_timings(),
            gpu_render_ms: self.renderer.gpu_render_ms(),
            gpu_sample_id: self.renderer.gpu_sample_id(),
            streaming_pending: self.renderer.pending_count(),
            lighting_mode: self.renderer.lighting_mode(),
            lighting_bytes: self.renderer.lighting_bytes(),
            gi_ready_fraction: self.renderer.gi_ready_fraction(),
        })
        .unwrap_or(JsValue::NULL)
    }
    pub fn interaction_label(&self) -> String {
        match self.life.target(
            &self.world,
            self.player.eye().to_array(),
            self.player.yaw,
            self.player.pitch,
        ) {
            Some((kind, id)) if kind == "person" => self
                .life
                .actors
                .iter()
                .find(|a| a.person.id == id)
                .map_or(String::new(), |a| {
                    format!(
                        "[E] Talk · {} · {}",
                        a.person.name,
                        citizens::ROLES[a.person.role as usize]
                    )
                }),
            Some((_, id)) => format!(
                "[E] {} door",
                if self
                    .world
                    .doors
                    .borrow()
                    .get(&(id as u32))
                    .is_some_and(|d| d.target > 0.5)
                {
                    "Close"
                } else {
                    "Open"
                }
            ),
            _ => String::new(),
        }
    }
    pub fn interact(&mut self) -> JsValue {
        let Some((kind, id)) = self.life.target(
            &self.world,
            self.player.eye().to_array(),
            self.player.yaw,
            self.player.pitch,
        ) else {
            return JsValue::NULL;
        };
        if kind == "person" {
            self.life.talking = Some(id);
            return self.dialogue("greeting");
        }
        let layouts = self.world.settlements.layouts_near(
            &self.world,
            self.player.position.x,
            self.player.position.z,
            6.,
        );
        let b = layouts
            .iter()
            .flat_map(|l| l.buildings.iter())
            .find(|b| b.id == id as u32);
        let Some(b) = b else {
            return JsValue::NULL;
        };
        let inside = b.inside(self.player.position.x, self.player.position.z, 0.);
        if !b.usage.public() && !inside && !self.life.home_occupied(b.id) {
            return JsValue::from_str("The door is closed. Its residents are away.");
        }
        let mut doors = self.world.doors.borrow_mut();
        let d = doors.entry(b.id).or_default();
        d.target = if d.target > 0.5 { 0. } else { 1. };
        d.hold = if d.target > 0.5 { 120. } else { 0. };
        JsValue::from_str(if d.target > 0.5 {
            "Opening the door."
        } else {
            "Closing the door."
        })
    }
    pub fn dialogue(&self, topic: &str) -> JsValue {
        serde_wasm_bindgen::to_value(&self.life.conversation(&self.world, topic))
            .unwrap_or(JsValue::NULL)
    }
    pub fn end_dialogue(&mut self) {
        self.life.talking = None;
    }
    pub fn restore_clock(&mut self, clock: f64) {
        if clock.is_finite() && clock >= 0. {
            self.life.clock = clock.min(1e10);
            self.hour = (clock / 120. % 24.) as f32;
            self.life.invalidate();
        }
    }
    pub fn settlement_destinations(&self) -> JsValue {
        #[derive(Serialize)]
        struct Destination {
            id: u32,
            name: String,
            kind: String,
            region: String,
            x: f32,
            z: f32,
        }
        let mut out = vec![];
        for kind in [
            settlements::Kind::Camp,
            settlements::Kind::Hamlet,
            settlements::Kind::Fort,
            settlements::Kind::Village,
            settlements::Kind::Town,
            settlements::Kind::City,
        ] {
            let mut entries: Vec<_> = self
                .world
                .settlements
                .entries
                .iter()
                .filter(|e| e.kind == kind)
                .collect();
            entries.sort_by(|a, b| {
                settlements::dist(
                    [a.site.x, a.site.z],
                    [self.player.position.x, self.player.position.z],
                )
                .total_cmp(&settlements::dist(
                    [b.site.x, b.site.z],
                    [self.player.position.x, self.player.position.z],
                ))
            });
            for e in entries.into_iter().take(2) {
                out.push(Destination {
                    id: e.site.id,
                    name: e.site.name.clone(),
                    kind: kind.name().into(),
                    region: e.region.clone(),
                    x: e.site.x,
                    z: e.site.z,
                });
            }
        }
        serde_wasm_bindgen::to_value(&out).unwrap_or(JsValue::NULL)
    }
    pub fn visit_settlement(&mut self, id: u32) {
        if let Some(l) = self.world.settlements.layout(&self.world, id) {
            self.teleport(l.entry.site.x, l.entry.site.z);
            self.face(0., 0.);
        }
    }
    /// Art review visits actual procedural rooms in the current seeded world.
    pub fn visit_interior(&mut self, kind: &str) -> String {
        use settlements::{Kind, Use};
        let usage = match kind {
            "inn" | "bedroom" => Use::Inn,
            "home" => Use::Home,
            "arcane" => Use::Arcane,
            "smithy" => Use::Smithy,
            "temple" => Use::Temple,
            _ => return String::new(),
        };
        let origin = [self.player.position.x, self.player.position.z];
        let mut entries: Vec<_> = self
            .world
            .settlements
            .entries
            .iter()
            .filter(|e| e.kind == Kind::City)
            .collect();
        entries.sort_by(|a, b| {
            settlements::dist(origin, [a.site.x, a.site.z])
                .total_cmp(&settlements::dist(origin, [b.site.x, b.site.z]))
        });
        let ids: Vec<_> = entries.into_iter().take(6).map(|e| e.site.id).collect();
        for id in ids {
            if let Some(l) = self.world.settlements.layout(&self.world, id) {
                if let Some(b) = l.buildings.iter().find(|b| b.usage == usage) {
                    let p = if kind == "bedroom" {
                        b.point(0., 0., b.half[1] * 0.48)
                    } else {
                        b.point(0., 0., -b.half[1] + 2.3)
                    };
                    let target = if kind == "bedroom" {
                        b.point(b.half[0] - 1.1, 0.8, b.half[1] - 1.2)
                    } else if kind == "inn" {
                        b.point(-b.half[0] + 1.5, 1.25, -b.half[1] + 3.2)
                    } else {
                        b.point(-1.4, 1.3, -b.half[1] + 3.7)
                    };
                    let name = b.name.clone();
                    self.teleport(p[0], p[2]);
                    let direction = glam::Vec3::from_array(target) - self.player.eye();
                    self.face(
                        direction.x.atan2(-direction.z),
                        (direction.y / direction.length()).asin(),
                    );
                    self.set_time(if kind == "home" { 17.5 } else { 20.0 });
                    return name;
                }
            }
        }
        String::new()
    }
    pub fn settlement_inspect(&self, id: u32) -> JsValue {
        serde_wasm_bindgen::to_value(&self.world.settlements.layout(&self.world, id).as_deref())
            .unwrap_or(JsValue::NULL)
    }
    pub fn map_data(&self, cx: f32, cz: f32, span: f32, res: u32) -> Vec<u8> {
        self.world.map_background_rgba(
            cx,
            cz,
            span.clamp(128.0, world::WORLD_SIZE * 4.0),
            res.clamp(32, 512),
        )
    }
    pub fn map_layer_data(&self, cx: f32, cz: f32, span: f32, res: u32, layer: u32) -> Vec<u8> {
        self.world.map_layer_rgba(
            cx,
            cz,
            span.clamp(128., world::WORLD_SIZE * 4.),
            res.clamp(32, 512),
            layer,
        )
    }
    pub fn features(&self, cx: f32, cz: f32, span: f32) -> JsValue {
        #[derive(Serialize)]
        struct Features {
            geography: Vec<atlas::Label>,
            sites: Vec<world::Site>,
            landmarks: Vec<world::Landmark>,
            roads: Vec<Vec<[f32; 2]>>,
            routes: Vec<world::Road>,
            buildings: Vec<settlements::Building>,
            streets: Vec<settlements::Street>,
        }
        // Showing all regional sites at continent scale adds noise and expensive geometry.
        let radius = span * 0.72;
        let mut sites = self.world.sites_near(cx, cz, radius);
        if span > 16000.0 {
            let stride = (span / 10000.0).ceil() as u32;
            sites.retain(|s| s.kind == "city" || s.id % stride == 0);
        }
        let mut landmarks = if span < 14000.0 {
            self.world.landmarks_near(cx, cz, radius)
        } else {
            Vec::new()
        };
        if span < 14000. {
            landmarks.extend(
                natural::landmarks_near(&self.world, cx, cz, radius)
                    .into_iter()
                    .map(|n| world::Landmark {
                        id: n.id,
                        name: n.name,
                        x: n.x,
                        z: n.z,
                        kind: n.kind.name().into(),
                    }),
            );
        }
        if span < 100000. {
            for p in geography::landmarks(self.world.seed) {
                if p.kind != geography::GeoLandmarkKind::MountainPass
                    || (p.position[0] - cx).hypot(p.position[1] - cz) > radius
                {
                    continue;
                }
                let s = self.world.natural_sample(p.position[0], p.position[1]);
                if s.ocean || s.height < s.water_height + 0.5 {
                    continue;
                }
                let names = [
                    "Greywind",
                    "Aster",
                    "Cloudrest",
                    "Raven",
                    "Ashen",
                    "Vey",
                    "Highwater",
                    "Cinder",
                ];
                landmarks.push(world::Landmark {
                    id: p.id,
                    name: format!("{} Pass", names[(p.id as usize) % names.len()]),
                    x: p.position[0],
                    z: p.position[1],
                    kind: "mountain_pass".into(),
                });
            }
        }
        let routes = self.world.road_map_routes(cx, cz, span);
        let roads = routes.iter().map(|route| route.points.clone()).collect();
        serde_wasm_bindgen::to_value(&Features {
            geography: atlas::labels(&self.world, cx, cz, span),
            sites,
            landmarks,
            roads,
            routes,
            buildings: if span < 5000. {
                self.world
                    .settlements
                    .layouts_near(&self.world, cx, cz, radius)
                    .iter()
                    .flat_map(|l| l.buildings.clone())
                    .collect()
            } else {
                vec![]
            },
            streets: if span < 5000. {
                self.world
                    .settlements
                    .layouts_near(&self.world, cx, cz, radius)
                    .iter()
                    .flat_map(|l| l.streets.clone())
                    .collect()
            } else {
                vec![]
            },
        })
        .unwrap_or(JsValue::NULL)
    }
    pub fn landscape_destinations(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&exploration::destinations(&self.world))
            .unwrap_or(JsValue::NULL)
    }
    pub fn walking_journeys(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&journeys::discover(&self.world)).unwrap_or(JsValue::NULL)
    }
    pub fn natural_destinations(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&natural::destinations(&self.world)).unwrap_or(JsValue::NULL)
    }
    pub fn world_size(&self) -> f32 {
        world::WORLD_SIZE
    }
    pub fn return_to_spawn(&mut self) {
        let (spawn, yaw) = self.world.spawn_view();
        self.teleport(spawn[0], spawn[1]);
        self.player.yaw = yaw;
        self.player.pitch = -0.10;
    }
    pub fn spawn(&self) -> Vec<f32> {
        self.world.spawn().to_vec()
    }
    pub fn set_weather_mode(&mut self, mode: u32) {
        self.renderer.set_weather_mode(mode);
    }
    pub fn set_weather_speed(&mut self, speed: f32) {
        self.renderer.set_weather_speed(speed);
    }
    pub fn set_weather_paused(&mut self, paused: bool) {
        self.renderer.set_weather_paused(paused);
    }
    pub fn set_reflections(&mut self, enabled: bool) {
        self.renderer.set_reflections(enabled);
    }
    pub fn set_enclosure(&mut self, enabled: bool) {
        self.renderer.set_enclosure(enabled);
    }
    pub fn set_render_resolution(&mut self, height: u32) {
        self.renderer.set_render_resolution(height);
    }
    pub fn render_resolution(&self) -> Vec<u32> {
        self.renderer.render_resolution().to_vec()
    }
    pub fn set_filter(&mut self, mode: u32, strength: f32) {
        self.renderer.set_filter(mode, strength);
    }
    pub fn set_antialiasing(&mut self, mode: u32) {
        self.renderer.set_antialiasing(mode);
    }
    pub fn set_shadows(&mut self, enabled: bool) {
        self.renderer.set_shadows(enabled);
    }
    pub fn set_meadow(&mut self, enabled: bool) {
        self.renderer.set_meadow(enabled);
    }
    pub fn set_ground_cover_density(&mut self, density: f32) {
        self.renderer.set_ground_cover_density(density);
    }
    pub fn set_quality(&mut self, q: u32) {
        self.renderer.set_quality(q.min(2));
    }
    pub fn set_time(&mut self, hour: f32) {
        if hour.is_finite() {
            self.hour = hour.rem_euclid(24.0);
            self.life.clock = (self.life.clock / 2880.).floor() * 2880. + self.hour as f64 * 120.;
            self.life.invalidate();
        }
    }
    pub fn is_ready(&self) -> bool {
        self.renderer.chunk_count() >= 180
            || (self.renderer.pending_count() == 0 && self.renderer.chunk_count() > 0)
    }
}

// Small pure exports also let Node verify the compiled WASM world without a GPU.
#[wasm_bindgen]
pub fn inspect_world(seed: u32, x: f32, z: f32) -> JsValue {
    #[derive(Serialize)]
    struct Inspection {
        height: f32,
        biome: String,
        road: f32,
        water_height: f32,
        ocean: bool,
        shore: world::ShoreKind,
        landmass: Option<u32>,
        world_size: f32,
        vegetation_density: f32,
        hydrology: world::HydrologyStats,
        landscape: regions::Landscape,
        suitability: habitat::Suitability,
        lakes: Vec<world::LakeInfo>,
        spawn: [f32; 2],
        sites: Vec<world::Site>,
    }
    let world = World::new(seed);
    let s = world.sample(x, z);
    serde_wasm_bindgen::to_value(&Inspection {
        height: s.height,
        biome: s.biome.name().into(),
        road: s.road,
        water_height: s.water_height,
        ocean: s.ocean,
        shore: s.shore,
        landmass: world.landmass_id(x, z),
        world_size: world::WORLD_SIZE,
        vegetation_density: ecology::tree_density(seed, x, z, &s),
        hydrology: world.hydrology_stats(),
        landscape: regions::sample(seed, x, z, &s),
        suitability: habitat::sample(&world, x, z),
        lakes: world.lakes().to_vec(),
        spawn: world.spawn(),
        sites: world.sites_near(x, z, 4000.0),
    })
    .unwrap_or(JsValue::NULL)
}
#[wasm_bindgen]
pub fn inspect_map(seed: u32, cx: f32, cz: f32, span: f32, res: u32) -> Vec<u8> {
    World::new(seed).map_rgba(
        cx,
        cz,
        span.clamp(128., world::WORLD_SIZE * 4.),
        res.clamp(32, 512),
    )
}

#[wasm_bindgen]
pub fn inspect_routes(seed: u32, cx: f32, cz: f32, span: f32) -> JsValue {
    let routes =
        World::new(seed).road_map_routes(cx, cz, span.clamp(128.0, world::WORLD_SIZE * 4.0));
    serde_wasm_bindgen::to_value(&routes).unwrap_or(JsValue::NULL)
}

#[wasm_bindgen]
pub fn inspect_landscapes(seed: u32) -> JsValue {
    serde_wasm_bindgen::to_value(&exploration::destinations(&World::new(seed)))
        .unwrap_or(JsValue::NULL)
}

#[cfg(test)]
mod geography_checks;
