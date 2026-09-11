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
    ];
    let mut chosen: [Option<(f32, Destination)>; 7] = std::array::from_fn(|_| None);
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
    chosen
        .into_iter()
        .flatten()
        .map(|(_, p)| scenic_view(world, p))
        .collect()
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
            if region.kind.name() != destination.name {
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
        assert_eq!(points.len(), 7);
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
