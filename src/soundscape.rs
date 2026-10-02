//! Bounded acoustic probes; audio playback belongs to the browser audio thread.
use crate::{
    ecology, geometry,
    player::Player,
    settlements::Use,
    weather::WeatherState,
    world::{Biome, Sample, ShoreKind, World},
};
use serde::Serialize;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub floor: &'static str,
    pub forest: f32,
    pub indoor: f32,
    pub water: [f32; 4], // absolute x/y/z, proximity
    pub water_kind: &'static str,
    pub fire: [f32; 4],
    pub swimming: bool,
    pub lightning_event: [f32; 4],
    pub thunder_source: [f32; 3],
}
#[derive(Default)]
pub struct Probe {
    position: Option<[f32; 3]>,
    clock: f64,
    cached: Environment,
}
impl Probe {
    pub(crate) fn sample(
        &mut self,
        world: &World,
        p: &Player,
        s: &Sample,
        w: &WeatherState,
        clock: f64,
    ) -> Environment {
        let pos = p.position.to_array();
        if self.position.is_none_or(|old| {
            (old[0] - pos[0]).hypot(old[2] - pos[2]) > 8.0 || (old[1] - pos[1]).abs() > 2.0
        }) || (clock - self.clock).abs() > 0.8
        {
            self.position = Some(pos);
            self.clock = clock;
            self.cached = Environment::default();
            let mut best = 0.0;
            for radius in [0.0, 24.0, 65.0, 125.0] {
                for i in 0..if radius == 0.0 { 1 } else { 8 } {
                    let a = i as f32 * std::f32::consts::TAU / 8.0;
                    let x = pos[0] + a.cos() * radius;
                    let z = pos[2] + a.sin() * radius;
                    let wet = world.natural_sample(x, z);
                    if wet.water_height <= wet.height + 0.05
                        || geometry::ice_surface_height(world, x, z).is_some()
                    {
                        continue;
                    }
                    let strength = (1.0 - radius / 155.0).max(0.0);
                    if strength <= best {
                        continue;
                    }
                    best = strength;
                    self.cached.water = [x, wet.water_height, z, strength];
                    self.cached.water_kind = if wet.ocean {
                        "surf"
                    } else if wet.river > 0.1 {
                        "river"
                    } else {
                        "lake"
                    };
                }
            }
            let mut fires = geometry::campfire_emitters(world, pos[0], pos[2], 35.0);
            for layout in world.settlements.layouts_near(world, pos[0], pos[2], 40.0) {
                for b in &layout.buildings {
                    let roofed = !matches!(b.usage, Use::Market | Use::Stable);
                    if roofed
                        && b.inside(pos[0], pos[2], 0.0)
                        && pos[1] >= b.floor - 0.3
                        && pos[1] < b.floor + b.height
                    {
                        self.cached.indoor = 1.0;
                        self.cached.floor = if matches!(
                            b.usage,
                            Use::Temple | Use::Smithy | Use::Hall | Use::Watchtower
                        ) {
                            "stone"
                        } else {
                            "wood"
                        };
                        fires.push(b.hearth());
                    }
                }
            }
            if let Some(f) = fires.iter().min_by(|a, b| {
                (a[0] - pos[0])
                    .hypot(a[2] - pos[2])
                    .total_cmp(&(b[0] - pos[0]).hypot(b[2] - pos[2]))
            }) {
                self.cached.fire = [
                    f[0],
                    f[1],
                    f[2],
                    (1.0 - (f[0] - pos[0]).hypot(f[2] - pos[2]) / 28.0).clamp(0.0, 1.0),
                ];
            }
        }
        let mut out = self.cached.clone();
        out.forest = ecology::tree_density(world.seed, pos[0], pos[2], s);
        out.swimming = s.water_height > pos[1] + 0.25 && out.indoor < 0.5;
        if out.indoor < 0.5 {
            out.floor = floor(s, w, pos[1]);
        }
        out.lightning_event = crate::weather::lightning_event(world.seed, w.seconds as f64);
        out.thunder_source = thunder_source(pos, out.lightning_event);
        out
    }
}
fn floor(s: &Sample, w: &WeatherState, y: f32) -> &'static str {
    if s.water_height > y + 0.05 {
        return "water";
    }
    if w.snow_cover > 0.22 || (matches!(s.biome, Biome::Alpine) && s.temperature < 0.22) {
        return "snow";
    }
    if s.shore == ShoreKind::Beach || matches!(s.biome, Biome::Desert | Biome::TropicalCoast) {
        return "sand";
    }
    if matches!(s.biome, Biome::Swamp | Biome::Wetland)
        || w.wetness > 0.48 && (s.road > 0.25 || s.moisture > 0.55)
    {
        return "mud";
    }
    if s.road > 0.3 {
        return "gravel";
    }
    if matches!(s.biome, Biome::Alpine) || s.shore == ShoreKind::Cliff {
        return "stone";
    }
    if matches!(s.biome, Biome::Forest | Biome::PineForest | Biome::Jungle) {
        return "leaves";
    }
    "grass"
}
// Same small-domain hash and candidate anchors as the sky shader. Thunder is
// tied to the nearest distant discharge cell, not to a camera-attached pan.
fn hash21(p: [f32; 2]) -> f32 {
    let fract = |v: f32| v - v.floor();
    let mut q = [
        fract(p[0] * 0.1031),
        fract(p[1] * 0.1031),
        fract(p[0] * 0.1031),
    ];
    let d = q[0] * (q[1] + 33.33) + q[1] * (q[2] + 33.33) + q[2] * (q[0] + 33.33);
    for v in &mut q {
        *v += d;
    }
    fract((q[0] + q[1]) * q[2])
}
fn thunder_source(pos: [f32; 3], event: [f32; 4]) -> [f32; 3] {
    let c = [(pos[0] / 14000.0).floor(), (pos[2] / 14000.0).floor()];
    let mut best = f32::MAX;
    let mut source = [pos[0] + 6000.0, 2000.0, pos[2]];
    for dz in -1..=1 {
        for dx in -1..=1 {
            let cell = [c[0] + dx as f32, c[1] + dz as f32];
            let k = [
                cell[0] + event[0] * 3.17 + event[3],
                cell[1] - event[0] * 5.71,
            ];
            if hash21([k[0] + 8.7, k[1] + 31.9]) < 0.42 {
                continue;
            }
            let x = (cell[0] + 0.5) * 14000.0 + (hash21([k[0] + 13.1, k[1] + 5.8]) - 0.5) * 10000.0;
            let z = (cell[1] + 0.5) * 14000.0 + (hash21([k[0] - 3.1, k[1] + 19.4]) - 0.5) * 10000.0;
            let d = (x - pos[0]).hypot(z - pos[2]);
            if d > 2200.0 && d < 21000.0 && d < best {
                best = d;
                source = [x, 2000.0, z];
            }
        }
    }
    source
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surface_priority_preserves_water_snow_and_wet_roads() {
        let mut s = Sample {
            height: 20.,
            biome: Biome::Grassland,
            road: 0.,
            road_kind: None,
            ocean: false,
            shore: ShoreKind::None,
            river: 0.,
            water_height: -1000.,
            temperature: 0.5,
            moisture: 0.7,
        };
        let mut w = WeatherState::default();
        assert_eq!(floor(&s, &w, 20.), "grass");
        s.road = 1.;
        assert_eq!(floor(&s, &w, 20.), "gravel");
        w.wetness = 0.8;
        assert_eq!(floor(&s, &w, 20.), "mud");
        w.snow_cover = 0.8;
        assert_eq!(floor(&s, &w, 20.), "snow");
        s.water_height = 21.;
        assert_eq!(floor(&s, &w, 20.), "water");
    }
    #[test]
    fn thunder_anchors_are_seeded_finite_and_distant() {
        for seed in [1, 67, 1337] {
            for time in [3., 19., 48.] {
                let e = crate::weather::lightning_event(seed, time);
                let p = [87851., 100., 55519.];
                let a = thunder_source(p, e);
                assert_eq!(a, thunder_source(p, e));
                assert!(a.iter().all(|x| x.is_finite()));
                assert!((a[0] - p[0]).hypot(a[2] - p[2]) >= 2200.);
            }
        }
    }
}
