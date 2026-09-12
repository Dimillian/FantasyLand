//! Three short journeys selected from real geographic relationships.
//! Candidate generation is cheap and bounded; only successful routes on the
//! rendered ground become journeys. Arrival positions exclude all solid props.
use crate::{
    ecology, exploration,
    geography::{self, GeoLandmarkKind},
    geometry, traversal,
    world::World,
};
use serde::Serialize;
use std::f32::consts::TAU;

#[derive(Clone, Debug, Serialize)]
pub struct Journey {
    pub name: String,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub minutes: f32,
    pub points: Vec<[f32; 2]>,
}
#[derive(Clone, Copy)]
struct Candidate {
    a: [f32; 2],
    b: [f32; 2],
    score: f32,
}
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn dry(world: &World, p: [f32; 2]) -> bool {
    let s = world.natural_sample(p[0], p[1]);
    !s.ocean && s.height > s.water_height + 0.6
}
fn safe_point(world: &World, p: [f32; 2]) -> Option<[f32; 2]> {
    for radius in [0., 16., 40., 85.] {
        for i in 0..if radius == 0. { 1 } else { 8 } {
            let angle = i as f32 * TAU / 8.;
            let q = [p[0] + angle.sin() * radius, p[1] + angle.cos() * radius];
            if !dry(world, q) {
                continue;
            }
            let water = world.natural_sample(q[0], q[1]).water_height;
            if geometry::walk_height(world, q[0], q[1]) > water + 0.4
                && !geometry::blocks_player(world, q[0], q[1])
            {
                return Some(q);
            }
        }
    }
    None
}
fn candidate(world: &World, a: [f32; 2], b: [f32; 2], theme_score: f32) -> Option<Candidate> {
    if !dry(world, a) || !dry(world, b) {
        return None;
    }
    let ah = world.natural_sample(a[0], a[1]).height;
    let bh = world.natural_sample(b[0], b[1]).height;
    let mut steep = 0.;
    let mut wet = 0;
    let mut last = ah;
    for i in 1..=12 {
        let t = i as f32 / 12.;
        let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let s = world.natural_sample(p[0], p[1]);
        if s.ocean || s.height < s.water_height + 0.5 {
            wet += 1;
        }
        steep += (s.height - last).abs();
        last = s.height;
    }
    // Allow a little water in the straight corridor: contour routing can skirt
    // a river or lake edge. Prefer candidates requiring only short detours.
    Some(Candidate {
        a,
        b,
        score: theme_score - wet as f32 * 18. - steep * 0.018 - (bh - ah).abs() * 0.012,
    })
}
fn choose(world: &World, name: &str, mut choices: Vec<Candidate>) -> Option<Journey> {
    choices.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.a[0].total_cmp(&b.a[0]))
            .then(a.a[1].total_cmp(&b.a[1]))
    });
    for c in choices.into_iter().take(10) {
        let Some(a) = safe_point(world, c.a) else {
            continue;
        };
        let Some(b) = safe_point(world, c.b) else {
            continue;
        };
        let Some(points) = traversal::walking_route(world, a, b, 0.48) else {
            continue;
        };
        let len = traversal::length(&points);
        if !(1650.0..=3300.0).contains(&len) {
            continue;
        }
        let next = points[1];
        return Some(Journey {
            name: name.into(),
            x: a[0],
            z: a[1],
            yaw: (next[0] - a[0]).atan2(-(next[1] - a[1])),
            pitch: -0.04,
            minutes: len / 330.,
            points,
        });
    }
    None
}
fn woodland_pass(world: &World) -> Option<Journey> {
    let mut choices = Vec::new();
    for pass in geography::landmarks(world.seed)
        .into_iter()
        .filter(|p| p.kind == GeoLandmarkKind::MountainPass && p.range_id < 3)
    {
        if !dry(world, pass.position) {
            continue;
        }
        // Start on a real forested shoulder across the divide, ending at the
        // generated saddle. Small changes of approach can avoid a rocky gully.
        let normal = [-pass.axis[1], pass.axis[0]];
        for side in [-1., 1.] {
            for along in [-850., 0., 850.] {
                for span in [1950., 2450.] {
                    let a = [
                        pass.position[0] + normal[0] * span * side + pass.axis[0] * along,
                        pass.position[1] + normal[1] * span * side + pass.axis[1] * along,
                    ];
                    let s = world.natural_sample(a[0], a[1]);
                    let forest = ecology::tree_density(world.seed, a[0], a[1], &s);
                    if let Some(c) = candidate(
                        world,
                        a,
                        pass.position,
                        forest * 35. + (pass.range_id == 0) as u8 as f32 * 2.,
                    ) {
                        choices.push(c);
                    }
                }
            }
        }
    }
    choose(world, "Woodland to the mountain pass", choices)
}
fn mountain_lake(world: &World) -> Option<Journey> {
    let mut choices = Vec::new();
    let mut lakes: Vec<_> = world.lakes().iter().filter(|l| l.surface > 450.).collect();
    lakes.sort_by(|a, b| b.surface.total_cmp(&a.surface));
    for lake in lakes.into_iter().take(5) {
        let d = distance(lake.center, lake.outlet).max(1.);
        let out = [
            (lake.outlet[0] - lake.center[0]) / d,
            (lake.outlet[1] - lake.center[1]) / d,
        ];
        let side = [-out[1], out[0]];
        for sign in [-1., 1.] {
            for offset in [100., 220., 400.] {
                let a = [
                    lake.outlet[0] + side[0] * offset * sign,
                    lake.outlet[1] + side[1] * offset * sign,
                ];
                let s = world.natural_sample(a[0], a[1]);
                if s.height > lake.surface + 65. {
                    continue;
                }
                for angle in [-0.65f32, -0.30, 0., 0.30, 0.65] {
                    let (sn, cs) = angle.sin_cos();
                    let forward = [out[0] * cs - out[1] * sn, out[0] * sn + out[1] * cs];
                    let b = [a[0] + forward[0] * 2150., a[1] + forward[1] * 2150.];
                    let lower = world.natural_sample(b[0], b[1]);
                    let descent = (s.height - lower.height).clamp(-100., 180.);
                    if let Some(c) =
                        candidate(world, a, b, descent * 0.055 + (lake.surface - 450.) * 0.008)
                    {
                        choices.push(c);
                    }
                }
            }
        }
    }
    choose(world, "From the high lake to its outlet valley", choices)
}
fn coast_walk(world: &World) -> Option<Journey> {
    let places = exploration::destinations(world);
    let start = places.iter().find(|p| p.name == "Windswept coast")?;
    let a = [start.x, start.z];
    let mut choices = Vec::new();
    for i in 0..24 {
        let angle = i as f32 * TAU / 24.;
        let mut b = [a[0] + angle.sin() * 2200., a[1] + angle.cos() * 2200.];
        // Follow the actual coast's signed-distance contours. This bends endpoint
        // selection around headlands/coves instead of merely picking map axes.
        for _ in 0..4 {
            let c = world.coast_info(b[0], b[1]).distance;
            let gx = world.coast_info(b[0] + 80., b[1]).distance
                - world.coast_info(b[0] - 80., b[1]).distance;
            let gz = world.coast_info(b[0], b[1] + 80.).distance
                - world.coast_info(b[0], b[1] - 80.).distance;
            let len = gx.hypot(gz).max(0.001);
            let move_by = (260. - c).clamp(-700., 700.);
            b[0] += gx / len * move_by;
            b[1] += gz / len * move_by;
        }
        if !(1650.0..=2900.0).contains(&distance(a, b)) {
            continue;
        }
        let c = world.coast_info(b[0], b[1]).distance;
        let sh = world.natural_sample(b[0], b[1]);
        let score = 15. - c.abs() * 0.005 + sh.height.min(180.) * 0.025;
        if let Some(c) = candidate(world, a, b, score) {
            choices.push(c);
        }
    }
    choose(world, "Coves and the headland shore", choices)
}
pub fn discover(world: &World) -> Vec<Journey> {
    [
        woodland_pass(world),
        mountain_lake(world),
        coast_walk(world),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_geographic_walks_repeat_and_follow_dry_rendered_ground() {
        let world = World::new(1337);
        let journeys = discover(&world);
        assert_eq!(journeys.len(), 3);
        assert_eq!(
            serde_json::to_string(&journeys).unwrap(),
            serde_json::to_string(&discover(&world)).unwrap()
        );
        for j in journeys {
            assert!((5.0..=10.0).contains(&j.minutes));
            assert!(!geometry::blocks_player(&world, j.x, j.z));
            for pair in j.points.windows(2) {
                let len = distance(pair[0], pair[1]);
                let n = (len / 8.).ceil().max(1.) as usize;
                let mut last = geometry::walk_height(&world, pair[0][0], pair[0][1]);
                for i in 0..=n {
                    let t = i as f32 / n as f32;
                    let p = [
                        pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                        pair[0][1] + (pair[1][1] - pair[0][1]) * t,
                    ];
                    let s = world.natural_sample(p[0], p[1]);
                    let h = geometry::walk_height(&world, p[0], p[1]);
                    assert!(!s.ocean && h > s.water_height + 0.34);
                    assert!(!crate::natural::blocks_player(
                        &world, p[0], h, p[1], 0.4, 1.8
                    ));
                    assert!((h - last).abs() < 0.48 * (len / n as f32) + 0.05);
                    last = h;
                }
            }
        }
    }
}
