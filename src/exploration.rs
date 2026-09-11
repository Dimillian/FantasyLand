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
                chosen[i] = Some((
                    score,
                    Destination {
                        name: r.kind.name(),
                        x,
                        z,
                    },
                ));
            }
        }
    }
    chosen.into_iter().flatten().map(|(_, p)| p).collect()
}
