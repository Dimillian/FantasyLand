//! Generated, safe arrivals for inspecting the regional worldgen pass.
use crate::{
    countryside,
    exploration::Destination,
    geometry,
    settlements::Kind,
    world::{ShoreKind, World},
};
fn safe(world: &World, x: f32, z: f32) -> bool {
    let s = world.natural_sample(x, z);
    !s.ocean
        && geometry::walk_height(world, x, z) > s.water_height + 0.5
        && !geometry::blocks_player(world, x, z)
}
pub fn destinations(world: &World) -> Vec<Destination> {
    let mut out = vec![];
    let mut farm: Option<(f32, Destination)> = None;
    for e in world
        .settlements
        .entries
        .iter()
        .filter(|e| matches!(e.kind, Kind::Village | Kind::Town | Kind::City))
        .take(800)
    {
        if world.landmass_id(e.site.x, e.site.z) != Some(0) {
            continue;
        }
        let reach = countryside::reach(e);
        for i in 0..12 {
            let a = i as f32 * std::f32::consts::TAU / 12.;
            let x = e.site.x + a.sin() * reach * 0.55;
            let z = e.site.z + a.cos() * reach * 0.55;
            let s = world.natural_sample(x, z);
            let Some(f) = countryside::sample(world, x, z, &s) else {
                continue;
            };
            let score = f.strength
                + (f.usage == countryside::Use::Crop) as u32 as f32 * 0.3
                + if s.temperature < 0.67 && s.temperature > 0.42 {
                    0.4
                } else {
                    0.
                }
                - e.site.x.hypot(e.site.z) / 600000.;
            if f.strength > 0.45
                && farm.as_ref().is_none_or(|(v, _)| score > *v)
                && safe(world, x, z)
            {
                farm = Some((
                    score,
                    Destination {
                        name: "Cultivated countryside",
                        x,
                        z,
                        yaw: (e.site.x - x).atan2(z - e.site.z),
                        pitch: -0.02,
                    },
                ));
            }
        }
    }
    if let Some((_, d)) = farm {
        out.push(d);
    }
    let kinds = [
        ShoreKind::Dunes,
        ShoreKind::SaltMarsh,
        ShoreKind::Shingle,
        ShoreKind::Cliff,
    ];
    let names = [
        "Dune coast",
        "Tidal saltmarsh",
        "Shingle shore",
        "Sea cliffs",
    ];
    let mut shores: [Option<(f32, Destination)>; 4] = std::array::from_fn(|_| None);
    for ray in 0..80 {
        let a = ray as f32 * std::f32::consts::TAU / 80.;
        let (dx, dz) = (a.sin(), a.cos());
        let (mut lo, mut hi) = (0., 155000.);
        for _ in 0..20 {
            let r = (lo + hi) * 0.5;
            if world.landmass_id(dx * r, dz * r) == Some(0) {
                lo = r;
            } else {
                hi = r;
            }
        }
        for inland in [18., 40., 80., 180., 300., 480.] {
            let x = dx * (lo - inland);
            let z = dz * (lo - inland);
            let s = world.natural_sample(x, z);
            if let Some(k) = kinds.iter().position(|t| *t == s.shore) {
                let score = if k == 3 {
                    world.natural_sample(x - dx * 130., z - dz * 130.).height - s.height
                } else {
                    world.natural_sample(x + 45., z + 45.).height - s.height
                };
                if shores[k].as_ref().is_none_or(|(old, _)| score > *old) && safe(world, x, z) {
                    let gx = world.coast_info(x + 50., z).distance
                        - world.coast_info(x - 50., z).distance;
                    let gz = world.coast_info(x, z + 50.).distance
                        - world.coast_info(x, z - 50.).distance;
                    let tx = if k == 3 {
                        gx - gz * 0.3
                    } else {
                        -gz * 1.7 - gx
                    };
                    let tz = if k == 3 { gz + gx * 0.3 } else { gx * 1.7 - gz };
                    shores[k] = Some((
                        score,
                        Destination {
                            name: names[k],
                            x,
                            z,
                            yaw: tx.atan2(-tz),
                            pitch: if k == 3 { 0.24 } else { -0.04 },
                        },
                    ));
                }
            }
        }
    }
    out.extend(shores.into_iter().flatten().map(|(_, d)| d));
    let mut falls = world.waterfalls_near(0., 0., 192000.);
    falls.retain(|f| f.lip[1] - f.toe[1] < 45. && f.width > 1.5);
    falls.sort_by(|a, b| (b.lip[1] - b.toe[1]).total_cmp(&(a.lip[1] - a.toe[1])));
    'falls: for f in falls.iter().take(80) {
        let dx = f.toe[0] - f.lip[0];
        let dz = f.toe[2] - f.lip[2];
        let len = dx.hypot(dz);
        for side in [-1., 1.] {
            for offset in [8., 5., 3.] {
                let x = f.toe[0] - dz / len * f.width * offset * side + dx / len * 25.;
                let z = f.toe[2] + dx / len * f.width * offset * side + dz / len * 25.;
                let y = geometry::walk_height(world, x, z) + 1.72;
                if safe(world, x, z) && (y - f.toe[1]).abs() < f.lip[1] - f.toe[1] + 8. {
                    let tx = (f.lip[0] + f.toe[0]) * 0.5;
                    let tz = (f.lip[2] + f.toe[2]) * 0.5;
                    let ty = (f.lip[1] + f.toe[1]) * 0.5;
                    out.push(Destination {
                        name: "River gorge waterfall",
                        x,
                        z,
                        yaw: (tx - x).atan2(z - tz),
                        pitch: ((ty - y) / (tx - x).hypot(tz - z)).atan().clamp(-0.35, 0.5),
                    });
                    break 'falls;
                }
            }
        }
    }
    out
}
