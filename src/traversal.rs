//! Bounded contour routing shared by mountain approaches and walking journeys.
//! A failed search returns no route; it never substitutes a line over a cliff.
use crate::world::World;
use std::{cmp::Reverse, collections::BinaryHeap};

pub fn route(world: &World, a: [f32; 2], b: [f32; 2], grade: f32) -> Option<Vec<[f32; 2]>> {
    route_surface(a, b, grade, |p| {
        let s = world.natural_sample(p[0], p[1]);
        (!s.ocean && s.height > s.water_height + 0.35).then_some(s.height)
    })
}

/// Walking journeys use the rendered surface and the actual open monument solids.
/// Trees remain ordinary small obstacles to step around; cliffs and water do not.
pub fn walking_route(world: &World, a: [f32; 2], b: [f32; 2], grade: f32) -> Option<Vec<[f32; 2]>> {
    route_surface(a, b, grade, |p| {
        let s = world.natural_sample(p[0], p[1]);
        if s.ocean {
            return None;
        }
        let h = crate::geometry::walk_height(world, p[0], p[1]);
        (h > s.water_height + 0.35
            && !crate::natural::blocks_player(world, p[0], h, p[1], 0.4, 1.8))
        .then_some(h)
    })
}

pub fn length(points: &[[f32; 2]]) -> f32 {
    points.windows(2).map(|p| distance(p[0], p[1])).sum()
}
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn segment<F: Fn([f32; 2]) -> Option<f32>>(
    a: [f32; 2],
    b: [f32; 2],
    grade: f32,
    surface: &F,
) -> bool {
    let len = distance(a, b);
    let n = (len / 8.).ceil().max(1.) as usize;
    let Some(mut last) = surface(a) else {
        return false;
    };
    for i in 1..=n {
        let t = i as f32 / n as f32;
        let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let Some(h) = surface(p) else { return false };
        if (h - last).abs() > grade * (len / n as f32) + 0.04 {
            return false;
        }
        last = h;
    }
    true
}

fn route_surface<F: Fn([f32; 2]) -> Option<f32>>(
    a: [f32; 2],
    b: [f32; 2],
    grade: f32,
    surface: F,
) -> Option<Vec<[f32; 2]>> {
    if !a.iter().chain(b.iter()).all(|x| x.is_finite()) || !(0.05..=1.).contains(&grade) {
        return None;
    }
    surface(a)?;
    surface(b)?;
    let direct = distance(a, b);
    // Bound the direct fine scan as well as A*. Finite endpoints can still
    // overflow their separation; neither case should launch unbounded work.
    if !direct.is_finite() || direct > 4200. {
        return None;
    }
    if direct < 0.1 {
        return Some(vec![a, b]);
    }
    if segment(a, b, grade, &surface) {
        return Some(vec![a, b]);
    }
    // Full two-dimensional search permits actual reversals/switchbacks. Regional
    // routes split long corridors before reaching this bounded local planner.
    let pad = (direct * 0.55).clamp(220., 800.);
    let min = [a[0].min(b[0]) - pad, a[1].min(b[1]) - pad];
    let max = [a[0].max(b[0]) + pad, a[1].max(b[1]) + pad];
    let step = ((max[0] - min[0]).max(max[1] - min[1]) / 64.).max(24.);
    let nx = ((max[0] - min[0]) / step).ceil() as usize + 1;
    let nz = ((max[1] - min[1]) / step).ceil() as usize + 1;
    let index = |p: [f32; 2]| {
        ((p[1] - min[1]) / step).round().clamp(0., (nz - 1) as f32) as usize * nx
            + ((p[0] - min[0]) / step).round().clamp(0., (nx - 1) as f32) as usize
    };
    let start = index(a);
    let end = index(b);
    if start == end {
        return None;
    }
    let point = |i: usize| {
        if i == start {
            a
        } else if i == end {
            b
        } else {
            [
                min[0] + (i % nx) as f32 * step,
                min[1] + (i / nx) as f32 * step,
            ]
        }
    };
    let heights: Vec<_> = (0..nx * nz).map(|i| surface(point(i))).collect();
    let mut costs = vec![f32::INFINITY; nx * nz];
    let mut previous = vec![usize::MAX; nx * nz];
    let mut closed = vec![false; nx * nz];
    let mut queue = BinaryHeap::new();
    costs[start] = 0.;
    queue.push(Reverse((0u64, start)));
    let mut expanded = 0;
    while let Some(Reverse((_, current))) = queue.pop() {
        if closed[current] {
            continue;
        }
        closed[current] = true;
        if current == end {
            break;
        }
        if expanded >= 5000 {
            return None;
        }
        expanded += 1;
        let p = point(current);
        let h = heights[current]?;
        for (dx, dz) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
            (-2, -1),
            (-2, 1),
            (2, -1),
            (2, 1),
            (-1, -2),
            (1, -2),
            (-1, 2),
            (1, 2),
        ] {
            let x = (current % nx) as i32 + dx;
            let z = (current / nx) as i32 + dz;
            if x < 0 || z < 0 || x >= nx as i32 || z >= nz as i32 {
                continue;
            }
            let next = z as usize * nx + x as usize;
            if closed[next] {
                continue;
            }
            let Some(nh) = heights[next] else { continue };
            let q = point(next);
            let len = distance(p, q);
            let slope = (nh - h).abs() / len.max(0.1);
            if slope > grade {
                continue;
            }
            let cost = costs[current] + len * (1. + slope * slope * 5.);
            if cost >= costs[next] || !segment(p, q, grade, &surface) {
                continue;
            }
            previous[next] = current;
            costs[next] = cost;
            queue.push(Reverse((((cost + distance(q, b)) * 64.) as u64, next)));
        }
    }
    if previous[end] == usize::MAX {
        return None;
    }
    let mut path = vec![b];
    let mut cursor = end;
    while cursor != start {
        cursor = previous[cursor];
        path.push(point(cursor));
    }
    path.reverse();
    // Simplify only where the same fine grade/water checks still pass. Retain
    // turns around contour shoulders instead of smoothing through the hillside.
    let mut result = vec![a];
    let mut i = 0;
    while i + 1 < path.len() {
        let mut next = i + 1;
        for j in (i + 2..(i + 14).min(path.len())).rev() {
            if segment(path[i], path[j], grade, &surface) {
                next = j;
                break;
            }
        }
        result.push(path[next]);
        i = next;
    }
    Some(result)
}

/// Repair steep dry road approaches in bounded neighborhoods. Existing river
/// crossing spans remain owned by the bridge builder. At most four disjoint
/// steep neighborhoods receive a bounded search in one road repair pass.
pub fn repair_approaches(world: &World, points: Vec<[f32; 2]>, grade: f32) -> Vec<[f32; 2]> {
    if points.len() < 3 {
        return points;
    }
    let surface = |p: [f32; 2]| {
        let s = world.natural_sample(p[0], p[1]);
        (!s.ocean && s.height > s.water_height + 0.4).then_some(s.height)
    };
    let mut result = vec![points[0]];
    let mut i = 0;
    let mut searches = 0;
    while i + 1 < points.len() {
        if searches >= 4 {
            result.extend_from_slice(&points[i + 1..]);
            break;
        }
        let end = (i + 8).min(points.len() - 1);
        let dry = points[i..=end].iter().all(|&p| surface(p).is_some());
        let steep = dry
            && points[i..=end]
                .windows(2)
                .any(|p| !segment(p[0], p[1], grade, &surface));
        if steep && searches < 4 {
            searches += 1;
            if let Some(detour) = route(world, points[i], points[end], grade) {
                result.extend_from_slice(&detour[1..]);
                i = end;
                continue;
            }
            // Move beyond the failed neighborhood. Repeating four almost
            // identical searches here starves every later steep approach.
            result.extend_from_slice(&points[i + 1..=end]);
            i = end;
            continue;
        }
        result.push(points[i + 1]);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contour_route_climbs_by_switchbacks_and_checks_every_short_segment() {
        let terrain = |p: [f32; 2]| Some(p[0] * 0.45);
        let a = [0., 0.];
        let b = [240., 0.];
        assert!(!segment(a, b, 0.26, &terrain));
        let path = route_surface(a, b, 0.26, terrain).expect("traversable switchback");
        assert!(length(&path) > distance(a, b) * 1.6);
        assert!(path.windows(2).all(|p| segment(p[0], p[1], 0.26, &terrain)));
    }
    #[test]
    fn impassable_water_has_no_invented_connection() {
        assert!(
            route_surface([-200., 0.], [200., 0.], 0.3, |p| (p[0].abs() > 70.)
                .then_some(0.))
            .is_none()
        );
    }
    #[test]
    fn remote_or_overflowing_input_is_rejected_before_a_fine_scan() {
        use std::cell::Cell;
        let reads = Cell::new(0usize);
        for b in [[100_000., 0.], [f32::MAX, 0.]] {
            reads.set(0);
            let route = route_surface([0., 0.], b, 0.3, |_| {
                reads.set(reads.get() + 1);
                Some(0.)
            });
            assert!(route.is_none());
            assert!(reads.get() <= 2);
        }
    }
}
