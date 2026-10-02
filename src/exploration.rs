//! Small, lazily requested tour of real generated landscapes for this prototype.
use crate::{
    ecology,
    regions::{self, LandscapeKind},
    world::World,
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Destination {
    pub name: &'static str,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
}

pub fn destinations(world: &World) -> Vec<Destination> {
    let kinds = [
        LandscapeKind::AncientWoodland,
        LandscapeKind::GraniteHighlands,
        LandscapeKind::WindsweptCoast,
        LandscapeKind::WetLowlands,
        LandscapeKind::SandstoneCountry,
        LandscapeKind::Meadowlands,
        LandscapeKind::Alpine,
        LandscapeKind::ChalkDowns,
        LandscapeKind::BasaltUplands,
    ];
    let mut chosen: [Option<(f32, Destination)>; 9] = std::array::from_fn(|_| None);
    let biome_kinds = [
        crate::world::Biome::Desert,
        crate::world::Biome::Swamp,
        crate::world::Biome::Savanna,
        crate::world::Biome::Jungle,
        crate::world::Biome::TropicalCoast,
        crate::world::Biome::Alpine,
        crate::world::Biome::Grassland,
    ];
    let mut biomes: [Option<(f32, Destination)>; 7] = std::array::from_fn(|_| None);
    let mut forests: [Option<(f32, Destination)>; 8] = std::array::from_fn(|_| None);
    for iz in -62..=62 {
        for ix in -62..=62 {
            let x = ix as f32 * 2900.0 + 731.0;
            let z = iz as f32 * 2900.0 + 419.0;
            let s = world.natural_sample(x, z);
            if s.ocean || s.water_height > s.height - 0.3 {
                continue;
            }
            let r = regions::sample(world.seed, x, z, &s);
            let trees = ecology::tree_density(world.seed, x, z, &s);
            if let Some(i) = biome_kinds.iter().position(|b| *b == s.biome) {
                let score = match s.biome {
                    crate::world::Biome::Jungle => trees * 8. - r.slope * 4.,
                    crate::world::Biome::TropicalCoast => {
                        trees * 9.
                            - (world.coast_info(x, z).distance - 300.).abs() / 700.
                            - r.slope * 5.
                    }
                    crate::world::Biome::Savanna => trees * 15. + s.height / 450. - r.slope * 5.,
                    crate::world::Biome::Alpine => s.height / 1400. - r.slope * 7.,
                    _ => r.rockiness * 2. + s.height / 400. - r.slope * 5.,
                };
                if biomes[i].as_ref().is_none_or(|old| score > old.0)
                    && !crate::geometry::blocks_player(world, x, z)
                    && crate::geometry::walk_height(world, x, z) > s.water_height + 0.4
                {
                    biomes[i] = Some((
                        score,
                        Destination {
                            name: s.biome.name(),
                            x,
                            z,
                            yaw: 1.4,
                            pitch: 0.06,
                        },
                    ));
                }
            }
            let density_threshold = match s.biome {
                crate::world::Biome::TropicalCoast => 0.35,
                crate::world::Biome::Savanna => 0.07,
                crate::world::Biome::Swamp => 0.50,
                _ => 0.72,
            };
            if trees > density_threshold && r.slope < 0.4 {
                let forest = ecology::forest_in(world.seed, x, z, &s, &r);
                let i = forest.kind as usize;
                let score = trees * 4. - r.slope * 3. + forest.regeneration * 0.4;
                if forests[i].as_ref().is_none_or(|old| score > old.0)
                    && !crate::geometry::blocks_player(world, x, z)
                    && crate::geometry::walk_height(world, x, z) > s.water_height + 0.40
                {
                    forests[i] = Some((
                        score,
                        Destination {
                            name: forest.kind.name(),
                            x,
                            z,
                            yaw: 1.4,
                            pitch: 0.12,
                        },
                    ));
                }
            }
            let score = -r.slope.min(5.0) * 8.0
                + match r.kind {
                    LandscapeKind::AncientWoodland => trees * 12.0 + r.ancient * 10.0,
                    LandscapeKind::GraniteHighlands => r.rockiness * 8.0 + s.height / 250.0,
                    LandscapeKind::WindsweptCoast => {
                        r.exposure * 5.0 - (world.coast_info(x, z).distance - 80.0).abs() / 120.0
                    }
                    LandscapeKind::WetLowlands => r.wetness * 12.0 + trees * 2.0,
                    LandscapeKind::SandstoneCountry => r.rockiness * 7.0 + (1.0 - trees) * 3.0,
                    LandscapeKind::Meadowlands => (1.0 - trees) * 10.0,
                    LandscapeKind::Alpine => r.rockiness * 5.0 + s.height / 500.0,
                    LandscapeKind::ChalkDowns => (1. - trees) * 8. + r.soil * 3.,
                    LandscapeKind::BasaltUplands => r.rockiness * 9. + s.height / 500.,
                };
            let i = kinds.iter().position(|k| *k == r.kind).unwrap();
            if chosen[i].as_ref().is_none_or(|old| score > old.0) {
                if crate::geometry::walk_height(world, x, z) < s.water_height + 0.40
                    || crate::geometry::blocks_player(world, x, z)
                {
                    continue;
                }
                chosen[i] = Some((
                    score,
                    Destination {
                        name: r.kind.name(),
                        x,
                        z,
                        yaw: 0.,
                        pitch: -0.04,
                    },
                ));
            }
        }
    }
    let mut result: Vec<_> = chosen
        .into_iter()
        .flatten()
        .map(|(_, p)| scenic_view(world, p))
        .collect();
    result.extend(
        forests
            .into_iter()
            .flatten()
            .map(|(_, p)| forest_view(world, p)),
    );
    result.extend(
        biomes
            .into_iter()
            .flatten()
            .map(|(_, p)| scenic_view(world, p)),
    );
    // A shore arrival beside actual retained ice, never a teleport into water.
    for lake in world
        .lakes()
        .iter()
        .filter(|l| world.lake_ice(l.center[0], l.center[1]) > 0.5)
        .take(1)
    {
        'shore: for radius in [150., 300., 550., 850., 1300.] {
            for i in 0..24 {
                let a = i as f32 * std::f32::consts::TAU / 24.;
                let x = lake.center[0] + a.sin() * radius;
                let z = lake.center[1] + a.cos() * radius;
                let s = world.sample(x, z);
                if !s.ocean
                    && s.height > s.water_height + 0.5
                    && (s.height - lake.surface).abs() < 60.
                    && !crate::geometry::blocks_player(world, x, z)
                {
                    result.push(Destination {
                        name: "Frozen mountain tarn",
                        x,
                        z,
                        yaw: (lake.center[0] - x).atan2(z - lake.center[1]),
                        pitch: -0.02,
                    });
                    break 'shore;
                }
            }
        }
    }
    // Expose real climbing routes, rather than dropping explorers at an
    // unrelated scenic summit. Arrivals face uphill along the generated path.
    let mut ascents = world.mountain_trails();
    ascents.sort_by(|a, b| {
        let height = |r: &crate::world::Road| {
            let p = r.points.last().unwrap();
            world.natural_sample(p[0], p[1]).height
        };
        height(b).total_cmp(&height(a))
    });
    let names = [
        "Mountain ascent · I",
        "Mountain ascent · II",
        "Mountain ascent · III",
    ];
    for (road, name) in ascents.iter().take(3).zip(names) {
        for pair in road.points.windows(2).take(8) {
            let [x, z] = pair[0];
            let water = world.natural_sample(x, z).water_height;
            if crate::geometry::walk_height(world, x, z) > water + 0.4
                && !crate::geometry::blocks_player(world, x, z)
            {
                result.push(Destination {
                    name,
                    x,
                    z,
                    yaw: (pair[1][0] - x).atan2(z - pair[1][1]),
                    pitch: 0.04,
                });
                break;
            }
        }
    }
    result.extend(crate::regional_tour::destinations(world));
    result
}

/// Choose a view between nearby trunks, keeping the actual generated location.
pub fn forest_view(world: &World, mut p: Destination) -> Destination {
    let mut best = f32::NEG_INFINITY;
    for i in 0..16 {
        let yaw = i as f32 * std::f32::consts::TAU / 16.;
        let mut score = yaw.sin() * 0.8;
        for distance in (3..28).step_by(3) {
            let d = distance as f32;
            if crate::geometry::blocks_player(world, p.x + yaw.sin() * d, p.z - yaw.cos() * d) {
                score -= (28. - d) * 0.20;
                break;
            }
        }
        if score > best {
            best = score;
            p.yaw = yaw;
        }
    }
    p
}

/// Find a reachable nearby viewpoint using terrain sightlines. This does not
/// alter or place scenery: the same view is available to a walking player.
pub fn scenic_view(world: &World, destination: Destination) -> Destination {
    let mut result = destination.clone();
    let mut best = f32::NEG_INFINITY;
    for dz in [-120., 0., 120.] {
        for dx in [-120., 0., 120.] {
            let x = destination.x + dx;
            let z = destination.z + dz;
            let s = world.natural_sample(x, z);
            if s.ocean
                || s.water_height > s.height - 0.4
                || crate::geometry::blocks_player(world, x, z)
            {
                continue;
            }
            let region = regions::sample(world.seed, x, z, &s);
            if region.kind.name() != destination.name && s.biome.name() != destination.name {
                continue;
            }
            let local_grade = (world.height(x + 4., z) - world.height(x - 4., z))
                .hypot(world.height(x, z + 4.) - world.height(x, z - 4.))
                / 8.;
            if local_grade > 0.60 {
                continue;
            }
            let ground = crate::geometry::walk_height(world, x, z);
            if ground < s.water_height + 0.35 {
                continue;
            }
            for direction in 0..16 {
                let yaw = direction as f32 * std::f32::consts::TAU / 16.;
                let mut score = 0.;
                let mut near_angle = f32::NEG_INFINITY;
                let mut far_angle = f32::NEG_INFINITY;
                let mut water_rays = 0;
                for (ray, offset) in [-0.50, 0., 0.50].into_iter().enumerate() {
                    let a = yaw + offset;
                    let mut skyline = -10.0_f32;
                    for distance in [24., 65., 180., 500., 1600., 4500.] {
                        let px = x + a.sin() * distance;
                        let pz = z - a.cos() * distance;
                        let t = world.natural_sample(px, pz);
                        let visible_height = t.height.max(t.water_height);
                        let angle = ((visible_height - ground - 1.72) / distance).atan();
                        if distance <= 65. {
                            near_angle = near_angle.max(angle);
                        }
                        if distance >= 500. {
                            far_angle = far_angle.max(angle);
                        }
                        if angle > skyline {
                            // Each successive layer contributes a real visible depth cue.
                            score += if distance >= 500. { 1.2 } else { 0.2 };
                            skyline = angle;
                        }
                        if distance == 180. && t.ocean {
                            water_rays += 1;
                        }
                        if distance == 65. && ray == 1 {
                            let trees = ecology::tree_density(world.seed, px, pz, &t);
                            if region.kind == LandscapeKind::AncientWoodland {
                                score += 2.5 * (1. - (trees - 0.40).abs());
                            }
                        }
                    }
                }
                score -= near_angle.max(0.04) * 18.0;
                score += far_angle.clamp(-0.25, 0.22) * 9.;
                score -= local_grade * 5.;
                if region.kind == LandscapeKind::WindsweptCoast {
                    score += if water_rays == 1 || water_rays == 2 {
                        7.
                    } else if water_rays == 3 {
                        -4.
                    } else {
                        -2.
                    };
                }
                if score > best {
                    best = score;
                    result = Destination {
                        name: destination.name,
                        x,
                        z,
                        yaw,
                        pitch: -0.035,
                    };
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tour_arrivals_are_on_rendered_dry_ground_and_clear_of_trunks() {
        let world = World::new(1337);
        let points = destinations(&world);
        assert!(points.len() >= 20, "only {} destinations", points.len());
        for label in [
            "Lowland swamp",
            "Golden savanna",
            "Tropical rainforest",
            "Tropical coast",
            "Desert & badlands",
            "Frozen mountain tarn",
            "Mountain ascent · I",
            "Cultivated countryside",
            "River gorge waterfall",
        ] {
            assert!(points.iter().any(|p| p.name == label), "missing {label}");
        }
        for p in points {
            let water = world.natural_sample(p.x, p.z).water_height;
            assert!(
                crate::geometry::walk_height(&world, p.x, p.z) >= water + 0.34,
                "submerged tour arrival {}",
                p.name
            );
            assert!(!crate::geometry::blocks_player(&world, p.x, p.z));
            assert!(p.yaw.is_finite() && p.pitch.is_finite());
        }
    }
}
