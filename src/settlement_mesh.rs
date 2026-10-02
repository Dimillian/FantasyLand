//! The same building recipes supply visible openings, support and collision.
use crate::{
    geometry::{MeshData, CHUNK_SIZE},
    settlements::{Building, Use},
    world::World,
};
use std::f32::consts::PI;
fn tinted(mesh: &mut MeshData, start: usize, texture: f32) {
    for v in &mut mesh.vertices[start..] {
        v.texture = texture;
    }
}
pub fn block(
    mesh: &mut MeshData,
    b: &Building,
    lo: [f32; 3],
    hi: [f32; 3],
    color: [f32; 3],
    texture: f32,
) {
    let p = |x: usize, y: usize, z: usize| {
        b.point([lo[0], hi[0]][x], [lo[1], hi[1]][y], [lo[2], hi[2]][z])
    };
    let start = mesh.vertices.len();
    mesh.quad(p(0, 0, 0), p(0, 1, 0), p(1, 1, 0), p(1, 0, 0), color, 2.);
    mesh.quad(p(1, 0, 1), p(1, 1, 1), p(0, 1, 1), p(0, 0, 1), color, 2.);
    mesh.quad(p(0, 0, 1), p(0, 1, 1), p(0, 1, 0), p(0, 0, 0), color, 2.);
    mesh.quad(p(1, 0, 0), p(1, 1, 0), p(1, 1, 1), p(1, 0, 1), color, 2.);
    mesh.quad(p(0, 1, 0), p(0, 1, 1), p(1, 1, 1), p(1, 1, 0), color, 2.);
    mesh.quad(p(0, 0, 1), p(0, 0, 0), p(1, 0, 0), p(1, 0, 1), color, 2.);
    for (face, vertices) in mesh.vertices[start..].chunks_mut(6).enumerate() {
        for v in vertices {
            let q = b.local(v.position[0], v.position[2]);
            let y = v.position[1] - b.floor;
            v.uv = match face {
                0 | 1 => [q[0], y],
                2 | 3 => [q[1], y],
                _ => [q[0], q[1]],
            };
            if texture == 27.0 {
                // A hanging is one complete heraldic design, independent of
                // its position within the house or which side faces the room.
                v.uv = [
                    2.0 * (q[0] - lo[0]) / (hi[0] - lo[0]).max(0.001),
                    2.0 * (hi[1] - y) / (hi[1] - lo[1]).max(0.001),
                ];
            }
            v.texture = 100.0 + texture;
        }
    }
}
fn window_wall(mesh: &mut MeshData, b: &Building, side: bool, color: [f32; 3]) {
    let [w, d] = b.half;
    let h = b.height;
    // Sashes, recessed frame and transom have actual thickness.
    for sign in [-1.0, 1.0] {
        if side {
            let x = sign * w;
            for z in [-0.88, 0.80] {
                block(
                    mesh,
                    b,
                    [x - 0.2, 1.04, z],
                    [x + 0.2, 2.58, z + 0.08],
                    [0.26, 0.17, 0.09],
                    4.0,
                );
            }
            for y in [1.04, 1.78, 2.50] {
                block(
                    mesh,
                    b,
                    [x - 0.2, y, -0.88],
                    [x + 0.2, y + 0.07, 0.88],
                    [0.26, 0.17, 0.09],
                    4.0,
                );
            }
        } else if sign > 0.0 {
            for x in [-0.88, 0.80] {
                block(
                    mesh,
                    b,
                    [x, 1.04, d - 0.2],
                    [x + 0.08, 2.58, d + 0.2],
                    [0.26, 0.17, 0.09],
                    4.0,
                );
            }
            for y in [1.04, 1.78, 2.50] {
                block(
                    mesh,
                    b,
                    [-0.88, y, d - 0.2],
                    [0.88, y + 0.07, d + 0.2],
                    [0.26, 0.17, 0.09],
                    4.0,
                );
            }
        }
    }
    // Walls genuinely stop at the window aperture; light sees the exterior.
    for sign in [-1., 1.] {
        if side {
            let x = sign * w;
            for (z0, z1, y0, y1) in [
                (-d, -0.85, 0., h),
                (0.85, d, 0., h),
                (-0.85, 0.85, 0., 1.1),
                (-0.85, 0.85, 2.5, h),
            ] {
                block(
                    mesh,
                    b,
                    [x - 0.12, y0, z0],
                    [x + 0.12, y1, z1],
                    color,
                    b.wall_material().0,
                );
            }
            block(
                mesh,
                b,
                [x - 0.17, 1.08, -1.],
                [x + 0.17, 1.2, 1.],
                [0.24, 0.16, 0.095],
                4.,
            );
            block(
                mesh,
                b,
                [x - 0.12, 1.2, -0.045],
                [x + 0.12, 2.5, 0.045],
                [0.23, 0.15, 0.09],
                4.,
            );
        } else {
            let z = sign * d;
            let opening = if sign < 0. { 0.88 } else { 0.85 };
            let low = if sign < 0. { 0. } else { 1.1 };
            let top = if sign < 0. { 2.55 } else { 2.5 };
            for (x0, x1, y0, y1) in [
                (-w, -opening, 0., h),
                (opening, w, 0., h),
                (-opening, opening, top, h),
            ] {
                block(
                    mesh,
                    b,
                    [x0, y0, z - 0.12],
                    [x1, y1, z + 0.12],
                    color,
                    b.wall_material().0,
                );
            }
            if low > 0. {
                block(
                    mesh,
                    b,
                    [-opening, 0., z - 0.12],
                    [opening, low, z + 0.12],
                    color,
                    b.wall_material().0,
                );
                block(
                    mesh,
                    b,
                    [-0.045, low, z - 0.14],
                    [0.045, top, z + 0.14],
                    [0.23, 0.15, 0.09],
                    4.,
                );
            }
        }
    }
}
/// Shared solid furniture/partition recipe. Rendering and movement consume it.
pub fn interior_parts(b: &Building, mut part: impl FnMut([f32; 3], [f32; 3], [f32; 3], f32, bool)) {
    crate::interior_design::generate(b, |p| part(p.lo, p.hi, p.color, p.texture, p.solid));
}

fn flame(mesh: &mut MeshData, b: &Building, p: [f32; 3], size: f32) {
    let start = mesh.vertices.len();
    for axis in 0..2 {
        let mut a = p;
        let mut c = p;
        let mut tip = p;
        a[axis * 2] -= size;
        c[axis * 2] += size;
        a[1] -= size;
        c[1] -= size;
        tip[1] += size * 1.6;
        mesh.triangle(a, tip, c, [1.0, 0.46, 0.07], 10.0);
    }
    for (i, v) in mesh.vertices[start..].iter_mut().enumerate() {
        v.uv = [[0.0, 1.0], [0.5, 0.0], [1.0, 1.0]][i % 3];
        v.texture = 16.0;
    }
    let _ = b;
}
fn furniture(mesh: &mut MeshData, b: &Building, lod: u32) {
    crate::interior_design::render(mesh, b, lod);
    if !matches!(b.usage, Use::Tent | Use::Market | Use::Stable) {
        let p = b.lamp_local();
        for i in 0..4 {
            let a = i as f32 * std::f32::consts::FRAC_PI_2;
            let at = b.point(a.cos() * 0.4, p[1] + 0.12, p[2] + a.sin() * 0.4);
            flame(mesh, b, at, 0.043);
        }
        if b.partition().is_some() {
            for side in [-1., 1.] {
                let p = b.point(side * (b.half[0] - 2.24), 0.90, b.half[1] - 0.7);
                flame(mesh, b, p, 0.04);
            }
        }
    }
    if !matches!(b.usage, Use::Stable | Use::Market) {
        let h = b.hearth();
        flame(mesh, b, [h[0], h[1], h[2]], 0.27);
    }
    if let Some(t) = b.torch() {
        let z = -b.half[1] + 1.25;
        let w = b.half[0];
        block(
            mesh,
            b,
            [-w + 0.1, 1.68, z - 0.07],
            [-w + 0.49, 1.79, z + 0.07],
            [0.20, 0.20, 0.18],
            12.0,
        );
        block(
            mesh,
            b,
            [-w + 0.36, 1.69, z - 0.06],
            [-w + 0.48, 2.05, z + 0.06],
            [0.28, 0.16, 0.07],
            4.0,
        );
        flame(mesh, b, [t[0], t[1], t[2]], 0.14);
    }
}
fn building(mesh: &mut MeshData, b: &Building, lod: u32) {
    let [w, d] = b.half;
    let h = b.height;
    let wall = b.wall_material().1;
    if b.usage == Use::Tent {
        let start = mesh.vertices.len();
        for side in [-1., 1.] {
            let mut p = [
                b.point(side * w, 0., -d),
                b.point(0., 2.7, -d),
                b.point(0., 2.7, d),
                b.point(side * w, 0., d),
            ];
            if side < 0.0 {
                p.reverse();
            }
            mesh.quad(p[0], p[1], p[2], p[3], [0.59, 0.46, 0.28], 5.0);
        }
        tinted(mesh, start, 11.);
        block(
            mesh,
            b,
            [-0.07, -0.1, -d],
            [0.07, 2.8, -d + 0.13],
            [0.28, 0.17, 0.075],
            4.,
        );
        block(mesh, b, [-w, -0.15, -d], [w, 0., d], [0.34, 0.25, 0.13], 4.);
        if lod < 2 {
            furniture(mesh, b, lod);
        }
        return;
    }
    block(
        mesh,
        b,
        [-w - 0.12, -5., -d - 0.12],
        [w + 0.12, -0.04, d + 0.12],
        [0.32, 0.34, 0.32],
        17.,
    );
    let stone_floor = matches!(
        b.usage,
        Use::Inn | Use::Smithy | Use::Temple | Use::Hall | Use::Barracks
    );
    block(
        mesh,
        b,
        [-w, -0.05, -d],
        [w, 0., d],
        if stone_floor {
            [0.51, 0.47, 0.37]
        } else {
            [0.46, 0.32, 0.19]
        },
        if stone_floor { 23. } else { 4. },
    );
    if matches!(b.usage, Use::Stable | Use::Market) {
        for x in [-w, w] {
            for z in [-d, d] {
                block(
                    mesh,
                    b,
                    [x - 0.13, 0., z - 0.13],
                    [x + 0.13, h, z + 0.13],
                    [0.23, 0.14, 0.075],
                    4.,
                );
            }
        }
    } else {
        window_wall(mesh, b, true, wall);
        window_wall(mesh, b, false, wall);
    }
    // Visible timber framing, lintels and stone sills give streets readable scale.
    for x in [-w, w] {
        for z in [-d, d] {
            block(
                mesh,
                b,
                [x - 0.16, 0., z - 0.16],
                [x + 0.16, h + 0.12, z + 0.16],
                [0.23, 0.15, 0.09],
                4.,
            );
        }
    }
    for y in [0.15, h - 0.12] {
        block(
            mesh,
            b,
            [-w - 0.18, y, -d - 0.2],
            [w + 0.18, y + 0.18, -d + 0.1],
            [0.26, 0.17, 0.095],
            4.,
        );
        block(
            mesh,
            b,
            [-w - 0.18, y, d - 0.1],
            [w + 0.18, y + 0.18, d + 0.2],
            [0.26, 0.17, 0.095],
            4.,
        );
    }
    // A single pitch joins the gable, roof skin, fascia and ridge exactly.
    let rise = b.roof_rise();
    let slope = rise / w;
    let roof = [[0.35, 0.22, 0.13], [0.30, 0.35, 0.37], [0.46, 0.29, 0.19]][(b.id % 3) as usize];
    for side in [-1.0, 1.0] {
        let a = b.point(side * (w + 0.48), h - slope * 0.48, -d - 0.42);
        let r = b.point(0.0, h + rise, -d - 0.42);
        let rr = b.point(0.0, h + rise, d + 0.42);
        let aa = b.point(side * (w + 0.48), h - slope * 0.48, d + 0.42);
        let first = mesh.vertices.len();
        if side > 0.0 {
            mesh.quad(a, r, rr, aa, roof, 2.0);
        } else {
            mesh.quad(aa, rr, r, a, roof, 2.0);
        }
        for v in &mut mesh.vertices[first..] {
            let q = b.local(v.position[0], v.position[2]);
            v.uv = [q[1], q[0].abs() * (1.0 + slope * slope).sqrt()];
            v.texture = 119.0;
        }
        block(
            mesh,
            b,
            [side * w - 0.16, h - 0.18, -d - 0.44],
            [side * w + 0.16, h + 0.06, d + 0.44],
            [0.24, 0.16, 0.09],
            4.0,
        );
    }
    for sign in [-1.0, 1.0] {
        let first = mesh.vertices.len();
        let a = b.point(-w, h, sign * d);
        let r = b.point(0.0, h + rise, sign * d);
        let c = b.point(w, h, sign * d);
        if sign < 0.0 {
            mesh.triangle(a, r, c, wall, 2.0);
        } else {
            mesh.triangle(c, r, a, wall, 2.0);
        }
        for v in &mut mesh.vertices[first..] {
            let q = b.local(v.position[0], v.position[2]);
            v.uv = [q[0], v.position[1] - b.floor];
            v.texture = 100.0 + b.wall_material().0;
        }
        // Continuous sloping prisms: no staircase blocks along the gable seam.
        for side in [-1.0_f32, 1.0] {
            let start = mesh.vertices.len();
            let x0 = (side * (w + 0.48)).min(0.0);
            let x1 = (side * (w + 0.48)).max(0.0);
            block(
                mesh,
                b,
                [x0, -0.14, sign * d - 0.18],
                [x1, 0.02, sign * d + 0.18],
                [0.23, 0.15, 0.085],
                4.0,
            );
            for v in &mut mesh.vertices[start..] {
                let x = b.local(v.position[0], v.position[2])[0];
                v.position[1] += h + rise - slope * x.abs();
            }
            for tri in mesh.vertices[start..].chunks_mut(3) {
                let a = glam::Vec3::from_array(tri[0].position);
                let normal = (glam::Vec3::from_array(tri[1].position) - a)
                    .cross(glam::Vec3::from_array(tri[2].position) - a)
                    .normalize()
                    .to_array();
                for v in tri {
                    v.normal = normal;
                }
            }
        }
    }

    block(
        mesh,
        b,
        [-0.13, h + rise - 0.06, -d - 0.48],
        [0.13, h + rise + 0.13, d + 0.48],
        roof,
        19.0,
    );
    block(
        mesh,
        b,
        [-w, h - 0.12, -d],
        [w, h + 0.01, d],
        [0.28, 0.19, 0.11],
        4.0,
    );
    // Wall studs and lower stone course vary the exterior silhouette.
    for x in [-w * 0.58, w * 0.58] {
        for z in [-d, d] {
            block(
                mesh,
                b,
                [x - 0.055, 0.16, z - 0.17],
                [x + 0.055, h, z + 0.17],
                [0.24, 0.15, 0.08],
                4.0,
            );
        }
    }
    if b.id % 5 != 1 {
        for sign in [-1.0, 1.0] {
            for (x0, x1) in if sign < 0.0 {
                vec![(-w, -0.96), (0.96, w)]
            } else {
                vec![(-w, w)]
            } {
                block(
                    mesh,
                    b,
                    [x0, 0.05, sign * d - 0.14],
                    [x1, 0.45, sign * d + 0.14],
                    [0.43, 0.43, 0.37],
                    17.0,
                );
            }
        }
    }
    if b.usage.public() {
        // Hanging shop/guild sign, deliberately no active commerce.
        block(
            mesh,
            b,
            [w - 0.7, 2.9, -d - 1.2],
            [w - 0.56, 3.03, -d + 0.1],
            [0.24, 0.16, 0.09],
            4.,
        );
        block(
            mesh,
            b,
            [w - 1.15, 2.05, -d - 0.92],
            [w - 0.15, 2.8, -d - 0.79],
            match b.usage {
                Use::Inn => [0.63, 0.40, 0.16],
                Use::Arcane => [0.30, 0.25, 0.51],
                Use::Guild | Use::Barracks => [0.50, 0.16, 0.12],
                _ => [0.30, 0.42, 0.31],
            },
            11.,
        );
        block(
            mesh,
            b,
            [w - 0.77, 2.22, -d - 0.95],
            [w - 0.54, 2.64, -d - 0.76],
            [0.82, 0.71, 0.43],
            12.,
        );
    }
    if lod < 2 {
        furniture(mesh, b, lod);
    }
}
pub fn append_chunk(world: &World, mesh: &mut MeshData, cx: i32, cz: i32, lod: u32) {
    let ox = cx as f32 * CHUNK_SIZE;
    let oz = cz as f32 * CHUNK_SIZE;
    for l in world.settlements.layouts_near(
        world,
        ox + CHUNK_SIZE * 0.5,
        oz + CHUNK_SIZE * 0.5,
        CHUNK_SIZE * 0.72,
    ) {
        for wall in &l.walls {
            if (wall.x / CHUNK_SIZE).floor() as i32 == cx
                && (wall.z / CHUNK_SIZE).floor() as i32 == cz
            {
                block(
                    mesh,
                    wall,
                    [-wall.half[0], -3., -0.3],
                    [wall.half[0], wall.height, 0.3],
                    [0.42, 0.40, 0.33],
                    17.,
                );
                for n in [-0.65, 0., 0.65] {
                    let x = n * wall.half[0];
                    block(
                        mesh,
                        wall,
                        [x - 0.5, wall.height, -0.38],
                        [x + 0.5, wall.height + 0.65, 0.38],
                        [0.40, 0.39, 0.32],
                        17.,
                    );
                }
            }
        }
        for b in &l.buildings {
            if (b.x / CHUNK_SIZE).floor() as i32 == cx && (b.z / CHUNK_SIZE).floor() as i32 == cz {
                building(mesh, b, lod);
            }
        }
        // Narrow, subdivided paths follow precisely the same earthwork surface.
        for street in &l.streets {
            for s in street.points.windows(2) {
                let d = [s[1][0] - s[0][0], s[1][1] - s[0][1]];
                let length = d[0].hypot(d[1]);
                let steps = (length / 3.).ceil().max(1.) as usize;
                let across = [
                    -d[1] / length.max(0.001) * street.width,
                    d[0] / length.max(0.001) * street.width,
                ];
                for i in 0..steps {
                    let a = [
                        s[0][0] + d[0] * i as f32 / steps as f32,
                        s[0][1] + d[1] * i as f32 / steps as f32,
                    ];
                    let b = [
                        s[0][0] + d[0] * (i + 1) as f32 / steps as f32,
                        s[0][1] + d[1] * (i + 1) as f32 / steps as f32,
                    ];
                    let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
                    if (mid[0] / CHUNK_SIZE).floor() as i32 != cx
                        || (mid[1] / CHUNK_SIZE).floor() as i32 != cz
                    {
                        continue;
                    }
                    let p = |p: [f32; 2], sign: f32| {
                        let x = p[0] + across[0] * sign;
                        let z = p[1] + across[1] * sign;
                        [
                            x,
                            crate::geometry::terrain_surface_height_lod(world, x, z, lod) + 0.075,
                            z,
                        ]
                    };
                    let first = mesh.vertices.len();
                    mesh.quad(
                        p(a, -1.),
                        p(b, -1.),
                        p(b, 1.),
                        p(a, 1.),
                        [0.47, 0.39, 0.25],
                        2.,
                    );
                    tinted(mesh, first, 0.);
                }
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Door {
    pub angle: f32,
    pub target: f32,
    pub hold: f32,
}
pub fn door_mesh(mesh: &mut MeshData, b: &Building, angle: f32) {
    let mut door = b.clone();
    let hinge = b.point(-0.86, 0., -b.half[1]);
    door.x = hinge[0];
    door.z = hinge[2];
    door.yaw = b.yaw - angle * PI * 0.51;
    block(
        mesh,
        &door,
        [0., 0.03, -0.09],
        [1.72, 2.5, 0.09],
        [0.31, 0.20, 0.115],
        4.,
    );
    for y in [0.38, 1.92] {
        block(
            mesh,
            &door,
            [0.06, y, -0.12],
            [1.65, y + 0.1, 0.12],
            [0.17, 0.17, 0.16],
            12.,
        );
    }
    block(
        mesh,
        &door,
        [1.43, 1.12, -0.16],
        [1.54, 1.25, 0.17],
        [0.56, 0.43, 0.2],
        12.,
    );
}
pub fn blocked(world: &World, x: f32, feet: f32, z: f32) -> bool {
    for l in world.settlements.layouts_near(world, x, z, 1.) {
        for wall in &l.walls {
            if feet < wall.floor + wall.height && feet + 1.8 > wall.floor && wall.inside(x, z, 0.35)
            {
                return true;
            }
        }
        let key = ((x / 16.).floor() as i32, (z / 16.).floor() as i32);
        if let Some(ids) = l.parcels.get(&key) {
            for &i in ids {
                let b = &l.buildings[i];
                if feet > b.floor + b.height || feet + 1.7 < b.floor {
                    continue;
                }
                let p = b.local(x, z);
                let [w, d] = b.half;
                if b.usage != Use::Tent
                    && !matches!(b.usage, Use::Market | Use::Stable)
                    && p[0].abs() < w + 0.45
                    && p[1].abs() < d + 0.45
                {
                    let doorway = p[0].abs() < 0.52 && p[1] < 0.;
                    if (p[0].abs() > w - 0.45 || p[1].abs() > d - 0.45) && !doorway {
                        return true;
                    }
                }
                if b.inside(x, z, 0.4) {
                    let mut hit = false;
                    interior_parts(b, |lo, hi, _, _, solid| {
                        if solid
                            && feet + 1.65 > b.floor + lo[1]
                            && feet < b.floor + hi[1]
                            && p[0] > lo[0] - 0.24
                            && p[0] < hi[0] + 0.24
                            && p[1] > lo[2] - 0.24
                            && p[1] < hi[2] + 0.24
                        {
                            hit = true;
                        }
                    });
                    if hit {
                        return true;
                    }
                }
                if b.door {
                    let angle = world.doors.borrow().get(&b.id).map_or(0., |d| d.angle);
                    let mut leaf = b.clone();
                    let hinge = b.point(-0.86, 0., -d);
                    leaf.x = hinge[0];
                    leaf.z = hinge[2];
                    leaf.yaw -= angle * PI * 0.51;
                    let q = leaf.local(x, z);
                    if q[0] > -0.3 && q[0] < 2.02 && q[1].abs() < 0.38 {
                        return true;
                    }
                }
            }
        }
    }
    false
}
pub fn floor(world: &World, x: f32, z: f32, height: f32) -> f32 {
    let mut h = height;
    for l in world.settlements.layouts_near(world, x, z, 1.) {
        for b in &l.buildings {
            if b.inside(x, z, 0.) {
                h = h.max(b.floor);
            }
        }
    }
    h
}

/// A small independent transparent batch; never present in opaque/shadow meshes.
pub fn glass_mesh(world: &World, eye: [f32; 3]) -> MeshData {
    let mut mesh = MeshData::default();
    let layouts = world.settlements.layouts_near(world, eye[0], eye[2], 100.0);
    let mut buildings: Vec<_> = layouts
        .iter()
        .flat_map(|l| &l.buildings)
        .filter(|b| b.door && (b.x - eye[0]).hypot(b.z - eye[2]) < 100.0)
        .collect();
    buildings.sort_by(|a, b| {
        ((b.x - eye[0]).hypot(b.z - eye[2])).total_cmp(&((a.x - eye[0]).hypot(a.z - eye[2])))
    });
    for b in buildings {
        for face in 0..3 {
            let [w, d] = b.half;
            let point = |x, y| match face {
                0 => b.point(-w, y, x),
                1 => b.point(w, y, x),
                _ => b.point(x, y, d),
            };
            let first = mesh.vertices.len();
            mesh.quad(
                point(-0.83, 1.13),
                point(-0.83, 2.48),
                point(0.83, 2.48),
                point(0.83, 1.13),
                if b.id % 3 == 0 {
                    [0.45, 0.62, 0.58]
                } else {
                    [0.64, 0.68, 0.56]
                },
                13.0,
            );
            for (i, v) in mesh.vertices[first..].iter_mut().enumerate() {
                v.uv = [
                    [0.0, 1.0],
                    [0.0, 0.0],
                    [1.0, 0.0],
                    [0.0, 1.0],
                    [1.0, 0.0],
                    [1.0, 1.0],
                ][i];
                v.texture = 22.0;
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(usage: Use, half: [f32; 2], yaw: f32) -> Building {
        Building {
            id: 20,
            name: "Fixture".into(),
            usage,
            x: 140.0,
            z: -230.0,
            floor: 12.0,
            yaw,
            half,
            height: 3.5,
            door: usage != Use::Tent,
            district: "Crafts".into(),
            street_node: 0,
            porch_node: 1,
            room_node: 2,
        }
    }
    #[test]
    fn furniture_preserves_central_routes_and_a_clear_rear_window() {
        for (usage, half) in [
            (Use::Home, [3.4, 4.2]),
            (Use::Inn, [5.0, 6.0]),
            (Use::Tent, [2.8, 3.3]),
        ] {
            let b = sample(usage, half, 0.7);
            interior_parts(&b, |lo, hi, _, _, solid| {
                assert!(lo.iter().zip(hi).all(|(a, b)| *a <= b));
                if solid && lo[1] < 1.65 && hi[1] > 0.0 && hi[2] > -half[1] && lo[2] < half[1] - 0.5
                {
                    assert!(
                        hi[0] + 0.24 <= -0.30 || lo[0] - 0.24 >= 0.30,
                        "centre aisle blocked by {lo:?}..{hi:?} in {usage:?}"
                    );
                }
                if usage != Use::Tent && lo[2] > half[1] - 0.8 && hi[1] > 1.2 && lo[1] < 2.48 {
                    assert!(hi[0] < -0.85 || lo[0] > 0.85, "rear window occluded");
                }
                if usage == Use::Tent && hi[2] < half[1] {
                    assert!(hi[1] < 0.6);
                }
            });
        }
    }
    #[test]
    fn architecture_uvs_and_roof_pitch_survive_building_rotation() {
        for yaw in [0.0, 0.6, 1.8] {
            let b = sample(Use::Home, [3.8, 4.8], yaw);
            let mut mesh = MeshData::default();
            building(&mut mesh, &b, 0);
            assert!(mesh.vertices.iter().all(|v| v
                .position
                .iter()
                .chain(v.normal.iter())
                .all(|f| f.is_finite())));
            // Transparent glass never enters this opaque/shadow mesh.
            assert!(mesh.vertices.iter().all(|v| v.material != 13.0));
            let roofs: Vec<_> = mesh
                .vertices
                .iter()
                .filter(|v| v.texture == 119.0 && v.normal[1] > 0.1 && v.normal[1] < 0.99)
                .collect();
            assert!(roofs.len() >= 12);
            for v in roofs {
                let q = b.local(v.position[0], v.position[2]);
                let expected =
                    b.floor + b.height + b.roof_rise() - q[0].abs() * b.roof_rise() / b.half[0];
                assert!((v.position[1] - expected).abs() < 0.005);
            }
            let mut cube = MeshData::default();
            block(&mut cube, &b, [-1., 0., -2.], [1., 2., 2.], [0.5; 3], 20.0);
            for v in cube.vertices.iter().take(6) {
                let p = b.local(v.position[0], v.position[2]);
                assert!((v.uv[0] - p[0]).abs() < 0.001);
                assert!((v.uv[1] - (v.position[1] - b.floor)).abs() < 0.001);
                assert_eq!(v.texture, 120.0);
            }
        }
    }
}
