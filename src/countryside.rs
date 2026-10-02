//! Settlement hinterlands: irregular parcels share their boundaries across
//! ground pigment, cover and tree recipes. Never changes collision elevations.
use crate::{
    ecology,
    settlements::{Entry, Kind},
    world::{hash, noise, rand01, smooth, Sample, ShoreKind, World},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Pasture,
    Crop,
    Orchard,
    Coppice,
}
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub usage: Use,
    pub strength: f32,
    pub edge: f32,
    pub track: f32,
    pub row: f32,
    pub id: u32,
}
pub fn reach(e: &Entry) -> f32 {
    match e.kind {
        Kind::Camp => 0.,
        Kind::Hamlet => 300.,
        Kind::Fort => 360.,
        Kind::Village => 630.,
        Kind::Town => 1000.,
        Kind::City => 1550.,
    }
}
pub fn sample(world: &World, x: f32, z: f32, s: &Sample) -> Option<Field> {
    if s.ocean
        || s.shore != ShoreKind::None
        || s.water_height > s.height - 1.5
        || s.height > 1050.
        || s.temperature < 0.32
        || s.moisture < 0.32
    {
        return None;
    }
    let e = world.settlements.rural_owner(x, z)?;
    let distance = (x - e.site.x).hypot(z - e.site.z);
    let influence = (1. - smooth(reach(e) * 0.65, reach(e), distance))
        * smooth(e.radius + 22., e.radius + 80., distance);
    if influence < 0.05 {
        return None;
    }
    let land = crate::regions::base(world.seed, x, z);
    let suitable = (1. - smooth(0.10, 0.28, land.slope)) * smooth(0.24, 0.65, land.fertility);
    if suitable < 0.04 {
        return None;
    }
    let a = rand01(e.site.id) * std::f32::consts::TAU;
    let (sn, cs) = a.sin_cos();
    let dx = x - e.site.x;
    let dz = z - e.site.z;
    let u = (dx * cs + dz * sn) + (noise(e.site.id ^ 33, x / 160., z / 160.) - 0.5) * 27.;
    let v = (-dx * sn + dz * cs) + (noise(e.site.id ^ 34, x / 160., z / 160.) - 0.5) * 27.;
    let cell = 95.;
    let ix = (u / cell).floor() as i32;
    let iz = (v / cell).floor() as i32;
    let mut first = (f32::MAX, 0u32, [0.; 2]);
    let mut second = (f32::MAX, [0.; 2]);
    for j in iz - 1..=iz + 1 {
        for i in ix - 1..=ix + 1 {
            let h = hash(e.site.id ^ 0xF13D, i, j);
            let p = [
                (i as f32 + 0.5 + (rand01(h) - 0.5) * 0.70) * cell,
                (j as f32 + 0.5 + (rand01(hash(h, 2, 3)) - 0.5) * 0.70) * cell,
            ];
            let d = (u - p[0]).powi(2) + (v - p[1]).powi(2);
            if d < first.0 {
                second = (first.0, first.2);
                first = (d, h, p);
            } else if d < second.0 {
                second = (d, p);
            }
        }
    }
    let border = (second.0 - first.0)
        / (2.
            * (first.2[0] - second.1[0])
                .hypot(first.2[1] - second.1[1])
                .max(1.));
    // Leave uncultivated fragments and natural margins within the farm belt.
    if first.1 % 9 == 0 {
        return None;
    }
    let usage = match first.1 % 10 {
        0..=3 => Use::Pasture,
        4..=6 => Use::Crop,
        7..=8 => Use::Orchard,
        _ => Use::Coppice,
    };
    let edge = (1. - smooth(3., 7., border)) * smooth(1.5, 3., border);
    let track = (1. - smooth(1., 2.8, border)) * (1. - s.road);
    let row = ((u * 0.72 + v * 0.19) * std::f32::consts::TAU / 2.2).sin() * 0.5 + 0.5;
    Some(Field {
        usage,
        strength: influence * suitable,
        edge,
        track,
        row,
        id: first.1,
    })
}
pub fn tint(color: [f32; 3], f: Field) -> [f32; 3] {
    let base = match f.usage {
        Use::Pasture => [0.40, 0.52, 0.23],
        Use::Crop => [0.59 + f.row * 0.06, 0.49 + f.row * 0.045, 0.23],
        Use::Orchard => [0.36, 0.44, 0.20],
        Use::Coppice => [0.29, 0.38, 0.19],
    };
    std::array::from_fn(|k| {
        let c = base[k] + ([0.46, 0.37, 0.23][k] - base[k]) * f.track;
        color[k] + (c - color[k]) * f.strength * 0.85
    })
}
pub fn cover(world: &World, x: f32, z: f32, s: &Sample) -> ecology::Ecology {
    let mut e = ecology::sample(world.seed, x, z, s);
    if let Some(f) = sample(world, x, z, s) {
        let strength = f.strength;
        e.tree_density *= 1. - strength * 0.95;
        e.ferns *= 1. - strength;
        e.shrubs *= 1. - strength;
        e.litter *= 1. - strength;
        e.grass_height *= 1. - strength * if f.usage == Use::Pasture { 0.38 } else { 0.10 };
        e.flowers *= 1. - strength * if f.usage == Use::Crop { 1.0 } else { 0.45 };
        e.cover_color = tint(e.cover_color, f);
        if f.usage == Use::Crop {
            e.seedheads = 0.65 * strength;
            e.grass_density =
                e.grass_density.max(0.82 * strength) * (1. - strength * 0.45 * (1. - f.row));
            e.grass_height *= 1. + strength * 0.28;
        }
        e.shrubs = (e.shrubs + f.edge * strength * 0.75).min(0.80);
        e.grass_density = e.grass_density.max(f.edge * strength * 0.72);
        let sum = e.flowers + e.ferns + e.heather + e.seedheads + e.shrubs + e.litter + e.reeds;
        let scale = (0.97 / sum.max(0.97)).min(1.);
        e.flowers *= scale;
        e.ferns *= scale;
        e.heather *= scale;
        e.seedheads *= scale;
        e.shrubs *= scale;
        e.litter *= scale;
        e.reeds *= scale;
        e.grass_density *= 1. - f.track * strength;
        e.undergrowth *= 1. - strength;
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fields_are_local_dry_suitable_and_leave_settlement_centers_clear() {
        let w = World::new(1337);
        let mut counts = [0; 4];
        let mut fields = 0;
        for e in w
            .settlements
            .entries
            .iter()
            .filter(|e| reach(e) > 500.)
            .take(100)
        {
            let s = w.natural_sample(e.site.x, e.site.z);
            assert!(sample(&w, e.site.x, e.site.z, &s).is_none());
            for i in 0..32 {
                let a = i as f32 * std::f32::consts::TAU / 32.;
                let x = e.site.x + a.sin() * reach(e) * 0.6;
                let z = e.site.z + a.cos() * reach(e) * 0.6;
                let s = w.natural_sample(x, z);
                if let Some(f) = sample(&w, x, z, &s) {
                    assert!(!s.ocean && s.height > s.water_height + 1.5 && s.height <= 1050.);
                    assert!((0.0..=1.0).contains(&f.strength));
                    let again = sample(&w, x, z, &s).unwrap();
                    assert_eq!(f.id, again.id);
                    assert_eq!(f.strength, again.strength);
                    counts[match f.usage {
                        Use::Pasture => 0,
                        Use::Crop => 1,
                        Use::Orchard => 2,
                        Use::Coppice => 3,
                    }] += 1;
                    fields += 1;
                    let mut wet = s;
                    wet.water_height = wet.height + 1.;
                    assert!(sample(&w, x, z, &wet).is_none());
                }
            }
        }
        assert!(fields > 100, "no useful hinterland: {fields}");
        assert!(counts.iter().all(|n| *n > 4), "{counts:?}");
    }
}
