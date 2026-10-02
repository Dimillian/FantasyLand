//! Geographic names belong to stable generated features, never the current
//! viewport. Only their visibility changes with map scale.
use crate::{
    geography::{self, GeoLandmarkKind},
    regions,
    world::{hash, rand01, World},
};
use crate::{natural, settlements, world};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct Label {
    pub id: u64,
    pub name: String,
    pub kind: &'static str,
    pub x: f32,
    pub z: f32,
    pub priority: u32,
}
pub fn stem(id: u32) -> String {
    let a = [
        "Aster", "Raven", "Aurel", "Vey", "Elder", "Thorn", "Cinder", "Grey", "Briar", "Hollow",
        "Silver", "Morrow", "Dawn", "Wren", "Amber", "Fallow",
    ];
    let b = [
        "mere", "ward", "vale", "en", "or", "eth", "wyn", "fall", "wold", "reach", "helm", "a",
        "ain", "moor", "ith", "haven",
    ];
    format!("{}{}", a[(id & 15) as usize], b[((id >> 8) & 15) as usize])
}
pub fn labels(world: &World, cx: f32, cz: f32, span: f32) -> Vec<Label> {
    let mut out = Vec::new();
    let radius = span * 0.74;
    let inside = |p: [f32; 2]| (p[0] - cx).hypot(p[1] - cz) < radius;
    let mut add = |id: u32, name, kind, p: [f32; 2], priority| {
        if inside(p) {
            let category = match kind {
                "range" => 1u64,
                "valley" => 2,
                "region" => 3,
                "river" => 4,
                _ => 5,
            };
            out.push(Label {
                id: (category << 32) | id as u64,
                name,
                kind,
                x: p[0],
                z: p[1],
                priority,
            });
        }
    };
    for p in geography::landmarks(world.seed) {
        if !inside(p.position) {
            continue;
        }
        let s = world.natural_sample(p.position[0], p.position[1]);
        if s.ocean {
            continue;
        }
        let family = stem(hash(world.seed ^ 0xA712, p.range_id as i32, 0));
        match p.kind {
            GeoLandmarkKind::RidgeSummit if span > 35000. => {
                add(p.id, format!("{family} Mountains"), "range", p.position, 0)
            }
            GeoLandmarkKind::ShelteredBasin if span < 60000. && span > 2500. => add(
                p.id,
                format!("{} Valley", stem(p.id)),
                "valley",
                p.position,
                3,
            ),
            _ => {}
        }
    }
    if span > 8000. && span < 170000. {
        for j in -8..8 {
            for i in -8..8 {
                let h = hash(world.seed ^ 0x7201, i, j);
                let p = [
                    (i as f32 + 0.5 + (rand01(h) - 0.5) * 0.46) * 24000.,
                    (j as f32 + 0.5 + (rand01(hash(h, j, i)) - 0.5) * 0.46) * 24000.,
                ];
                if !inside(p) {
                    continue;
                }
                let s = world.natural_sample(p[0], p[1]);
                if s.ocean || s.water_height > s.height {
                    continue;
                }
                let r = regions::sample(world.seed, p[0], p[1], &s);
                let suffix = match r.kind {
                    regions::LandscapeKind::AncientWoodland => "Wildwood",
                    regions::LandscapeKind::GraniteHighlands => "Fells",
                    regions::LandscapeKind::WindsweptCoast => "Coast",
                    regions::LandscapeKind::WetLowlands => "Fens",
                    regions::LandscapeKind::SandstoneCountry => "Redlands",
                    regions::LandscapeKind::Meadowlands => "Meadows",
                    regions::LandscapeKind::Alpine => "Highlands",
                    regions::LandscapeKind::ChalkDowns => "Downs",
                    regions::LandscapeKind::BasaltUplands => "Blacklands",
                };
                let identity = ((j + 8) * 16 + i + 8) as u32;
                add(
                    identity,
                    format!(
                        "{} {suffix}",
                        stem(hash(world.seed ^ 0xA719, identity as i32, 0))
                    ),
                    "region",
                    p,
                    2,
                );
            }
        }
    }
    if span > 5000. {
        for &(id, p, flow) in world.named_rivers() {
            if span < 110000. || flow > 2500. {
                add(id, format!("River {}", stem(id)), "river", p, 1);
            }
        }
    }
    if span < 140000. {
        for lake in world.lakes() {
            if lake.area_km2 < 0.1 && span > 30000. {
                continue;
            }
            let id = hash(world.seed ^ 0xA713, lake.id as i32, 0);
            let suffix = if lake.surface > 1700. { "Tarn" } else { "Lake" };
            add(id, format!("{} {suffix}", stem(id)), "lake", lake.center, 2);
        }
    }
    out.sort_by_key(|l| (l.priority, l.id));
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_stable_across_scale_and_layers_have_real_differences() {
        let w = World::new(1337);
        let a = labels(&w, 0., 0., 100000.);
        let b = labels(&w, 1000., 1000., 90000.);
        assert!(a.iter().any(|l| l.kind == "range"));
        assert!(a.iter().any(|l| l.kind == "region"));
        for x in &a {
            if let Some(y) = b.iter().find(|y| y.id == x.id) {
                assert_eq!(x.name, y.name);
                assert_eq!(x.x, y.x);
            }
        }
        let normal = w.map_layer_rgba(0., 0., 50000., 48, 0);
        let relief = w.map_layer_rgba(0., 0., 50000., 48, 1);
        let rock = w.map_layer_rgba(0., 0., 50000., 48, 2);
        assert_eq!(normal.len(), 48 * 48 * 4);
        assert_ne!(normal, relief);
        assert_ne!(relief, rock);
    }
}

#[derive(Serialize)]
pub struct Features {
    geography: Vec<Label>,
    sites: Vec<world::Site>,
    landmarks: Vec<world::Landmark>,
    roads: Vec<Vec<[f32; 2]>>,
    routes: Vec<world::Road>,
    buildings: Vec<settlements::Building>,
    streets: Vec<settlements::Street>,
}

pub fn features(world: &World, cx: f32, cz: f32, span: f32) -> Features {
    // Showing all regional sites at continent scale adds noise and expensive geometry.
    let radius = span * 0.72;
    let mut sites = world.sites_near(cx, cz, radius);
    if span > 16000.0 {
        let stride = (span / 10000.0).ceil() as u32;
        sites.retain(|s| s.kind == "city" || s.id % stride == 0);
    }
    let mut landmarks = if span < 14000.0 {
        world.landmarks_near(cx, cz, radius)
    } else {
        Vec::new()
    };
    if span < 14000. {
        landmarks.extend(
            natural::landmarks_near(world, cx, cz, radius)
                .into_iter()
                .map(|n| world::Landmark {
                    stable_id: n.stable_id(),
                    id: n.id,
                    name: n.name,
                    x: n.x,
                    z: n.z,
                    kind: n.kind.name().into(),
                }),
        );
    }
    if span < 100000. {
        for p in geography::landmarks(world.seed) {
            if p.kind != geography::GeoLandmarkKind::MountainPass
                || (p.position[0] - cx).hypot(p.position[1] - cz) > radius
            {
                continue;
            }
            let s = world.natural_sample(p.position[0], p.position[1]);
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
                stable_id: crate::worldgen::LocationId::cell(
                    "mountain_pass",
                    p.range_id as i32,
                    0,
                    p.id,
                ),
                id: p.id,
                name: format!("{} Pass", names[(p.id as usize) % names.len()]),
                x: p.position[0],
                z: p.position[1],
                kind: "mountain_pass".into(),
            });
        }
    }
    let routes = world.road_map_routes(cx, cz, span);
    let roads = routes.iter().map(|route| route.points.clone()).collect();
    Features {
        geography: labels(world, cx, cz, span),
        sites,
        landmarks,
        roads,
        routes,
        buildings: if span < 5000. {
            world
                .settlements
                .layouts_near(world, cx, cz, radius)
                .iter()
                .flat_map(|l| l.buildings.clone())
                .collect()
        } else {
            vec![]
        },
        streets: if span < 5000. {
            world
                .settlements
                .layouts_near(world, cx, cz, radius)
                .iter()
                .flat_map(|l| l.streets.clone())
                .collect()
        } else {
            vec![]
        },
    }
}
