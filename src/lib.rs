mod antialias;
pub mod celestial;
pub mod climate;
mod cloud_shadow;
pub mod cover;
pub mod ecology;
pub mod exploration;
pub mod geography;
pub mod geometry;
mod gpu_profile;
pub mod habitat;
mod horizon;
pub mod journeys;
pub mod materials;
pub mod meadow;
pub mod natural;
mod plants;
mod player;
mod postprocess;
pub mod precipitation;
mod rays;
pub mod regions;
pub mod renderer;
mod shadow;
pub mod streaming;
pub mod traversal;
mod vertex;
pub mod water_sim;
pub mod weather;
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
    hour: f32,
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
    site_name: String,
    day_time: f32,
    chunk_count: usize,
    triangle_count: usize,
    ground_cover_density: f32,
    sun_shadows: bool,
    cover_instances: u32,
    mesh_megabytes: f32,
    weather: weather::WeatherState,
    reflection_draws: u32,
    gpu_timings: Vec<gpu_profile::PassTime>,
    gpu_render_ms: Option<f32>,
    streaming_pending: usize,
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
            hour: 9.0,
        })
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
        let dt = dt.clamp(0.0, 0.05);
        self.player
            .update(&self.world, dt, forward, strafe, sprint, jump);
        self.hour = (self.hour + dt / 120.0) % 24.0;
        self.renderer.advance_time(dt);
        self.renderer
            .update_weather(&self.world, self.player.eye(), self.hour, dt);
        self.renderer
            .update_chunks(&self.world, self.player.position, false);
        self.renderer
            .render(
                self.player.eye(),
                self.player.yaw,
                self.player.pitch,
                self.hour,
            )
            .map_err(|e| JsValue::from_str(&e))
    }
    pub fn teleport(&mut self, x: f32, z: f32) {
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
            .sites_near(p.position.x, p.position.z, 130.0)
            .into_iter()
            .min_by(|a, b| {
                let da = (a.x - p.position.x).powi(2) + (a.z - p.position.z).powi(2);
                let db = (b.x - p.position.x).powi(2) + (b.z - p.position.z).powi(2);
                da.total_cmp(&db)
            });
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
            site_name: place_name,
            day_time: self.hour,
            chunk_count: self.renderer.chunk_count(),
            triangle_count: self.renderer.triangle_count(),
            ground_cover_density: self.renderer.ground_cover_density(),
            sun_shadows: self.renderer.shadows_enabled(),
            cover_instances: self.renderer.cover_drawn_instances(),
            mesh_megabytes: self.renderer.mesh_bytes() as f32 / 1_000_000.0,
            weather: *self.renderer.weather_state(),
            reflection_draws: self.renderer.reflection_draws(),
            gpu_timings: self.renderer.gpu_timings(),
            gpu_render_ms: self.renderer.gpu_render_ms(),
            streaming_pending: self.renderer.pending_count(),
        })
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
    pub fn features(&self, cx: f32, cz: f32, span: f32) -> JsValue {
        #[derive(Serialize)]
        struct Features {
            sites: Vec<world::Site>,
            landmarks: Vec<world::Landmark>,
            roads: Vec<Vec<[f32; 2]>>,
            routes: Vec<world::Road>,
        }
        // Showing all regional sites at continent scale adds noise and expensive geometry.
        let radius = span * 0.72;
        let mut sites = self.world.sites_near(cx, cz, radius);
        if span > 16000.0 {
            let stride = (span / 10000.0).ceil() as u32;
            sites.retain(|s| s.id % stride == 0);
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
            sites,
            landmarks,
            roads,
            routes,
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
