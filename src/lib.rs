pub mod ecology;
pub mod geometry;
mod horizon;
mod player;
mod postprocess;
pub mod renderer;
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
                if sample.road > 0.2 {
                    "The King's Road".into()
                } else {
                    "The Wilds".into()
                }
            });
        serde_wasm_bindgen::to_value(&GameState {
            x: p.position.x,
            y: p.position.y,
            z: p.position.z,
            yaw: p.yaw,
            pitch: p.pitch,
            biome: sample.biome.name().to_string(),
            landscape: {
                let trees =
                    ecology::tree_density(self.world.seed, p.position.x, p.position.z, &sample);
                match sample.biome {
                    world::Biome::Alpine
                    | world::Biome::Desert
                    | world::Biome::Moor
                    | world::Biome::Wetland => sample.biome.name(),
                    _ if trees < 0.025 => "Open wildland",
                    _ if trees < 0.20 => "Sparse woodland",
                    world::Biome::PineForest if trees > 0.70 => "Dense pine forest",
                    _ if trees > 0.70 => "Dense forest",
                    _ => "Woodland",
                }
                .to_string()
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
        })
        .unwrap_or(JsValue::NULL)
    }
    pub fn map_data(&self, cx: f32, cz: f32, span: f32, res: u32) -> Vec<u8> {
        self.world.map_rgba(
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
        }
        // Showing all regional sites at continent scale adds noise and expensive geometry.
        let radius = span * 0.72;
        let mut sites = self.world.sites_near(cx, cz, radius);
        if span > 16000.0 {
            let stride = (span / 10000.0).ceil() as u32;
            sites.retain(|s| s.id % stride == 0);
        }
        let landmarks = if span < 14000.0 {
            self.world.landmarks_near(cx, cz, radius)
        } else {
            Vec::new()
        };
        let roads = if span < 22000.0 {
            self.world.roads_near(cx, cz, radius)
        } else {
            Vec::new()
        };
        serde_wasm_bindgen::to_value(&Features {
            sites,
            landmarks,
            roads,
        })
        .unwrap_or(JsValue::NULL)
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
    pub fn set_filter(&mut self, mode: u32, strength: f32) {
        self.renderer.set_filter(mode, strength);
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
        vegetation_density: f32,
        hydrology: world::HydrologyStats,
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
        vegetation_density: ecology::tree_density(seed, x, z, &s),
        hydrology: world.hydrology_stats(),
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
