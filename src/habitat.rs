//! Geographic opportunities for future habitation; no town/NPC placement here.
use crate::{
    regions,
    world::{ShoreKind, World},
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suitability {
    pub dry_ground: bool,
    pub buildable: f32,
    pub fresh_water: f32,
    pub shelter: f32,
    pub fertility: f32,
    pub crossing: f32,
    pub harbor: f32,
    pub habitation: f32,
}
pub fn sample(world: &World, x: f32, z: f32) -> Suitability {
    let s = world.natural_sample(x, z);
    let r = regions::base(world.seed, x, z);
    let dry = !s.ocean && s.height > s.water_height + 1.5;
    let mut low = s.height;
    let mut high = s.height;
    let mut fresh: f32 = 0.;
    let mut crossing: f32 = 0.;
    let mut ocean: f32 = 0.;
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.;
        let near = world.natural_sample(x + a.sin() * 24., z + a.cos() * 24.);
        low = low.min(near.height);
        high = high.max(near.height);
        for d in [90., 240., 600.] {
            let p = world.natural_sample(x + a.sin() * d, z + a.cos() * d);
            if p.water_height > p.height {
                if p.ocean {
                    ocean += 1. / 24.;
                } else {
                    fresh = fresh.max(1. - d / 900.);
                    if p.river > 0.2 && d < 250. {
                        crossing = crossing.max(0.85 - d / 1000.);
                    }
                }
            }
        }
    }
    let buildable = if dry {
        (1. - (high - low) / 12.).clamp(0., 1.)
    } else {
        0.
    };
    let harbor = if dry && s.shore != ShoreKind::Cliff {
        (1. - (ocean - 0.30).abs() * 3.3).max(0.) * (ocean * 8.).min(1.) * r.shelter
    } else {
        0.
    };
    let habitation = buildable
        * (0.15
            + fresh * 0.34
            + r.shelter * 0.22
            + r.fertility * 0.20
            + crossing * 0.05
            + harbor * 0.04);
    Suitability {
        dry_ground: dry,
        buildable,
        fresh_water: fresh,
        shelter: r.shelter,
        fertility: r.fertility,
        crossing,
        harbor,
        habitation,
    }
}
