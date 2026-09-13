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
    tinted(mesh, start, texture);
}
fn window_wall(mesh: &mut MeshData, b: &Building, side: bool, color: [f32; 3]) {
    let [w, d] = b.half;
    let h = b.height;
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
                block(mesh, b, [x - 0.12, y0, z0], [x + 0.12, y1, z1], color, 18.);
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
                block(mesh, b, [x0, y0, z - 0.12], [x1, y1, z + 0.12], color, 18.);
            }
            if low > 0. {
                block(
                    mesh,
                    b,
                    [-opening, 0., z - 0.12],
                    [opening, low, z + 0.12],
                    color,
                    18.,
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
fn furniture(mesh: &mut MeshData, b: &Building) {
    let [w, d] = b.half;
    let wood = [0.32, 0.21, 0.12];
    if let Some(source) = b.torch() {
        let z = -d + 1.25;
        block(
            mesh,
            b,
            [-w + 0.12, 1.65, z - 0.06],
            [-w + 0.45, 1.76, z + 0.06],
            [0.18, 0.17, 0.15],
            12.,
        );
        block(
            mesh,
            b,
            [-w + 0.36, 1.65, z - 0.055],
            [-w + 0.48, 2.0, z + 0.055],
            [0.25, 0.14, 0.055],
            4.,
        );
        let first = mesh.vertices.len();
        mesh.triangle(
            [source[0] - 0.13, source[1] - 0.18, source[2]],
            [source[0], source[1] + 0.27, source[2]],
            [source[0] + 0.13, source[1] - 0.18, source[2]],
            [1., 0.48, 0.08],
            10.,
        );
        for (v, uv) in mesh.vertices[first..]
            .iter_mut()
            .zip([[0., 1.], [0.5, 0.], [1., 1.]])
        {
            v.uv = uv;
            v.texture = 16.;
        }
    }

    // Hearth, masonry flue, burning logs. Flame emission is an existing HDR material.
    block(
        mesh,
        b,
        [-w + 0.2, 0., d - 1.6],
        [-w + 1.7, 0.3, d - 0.2],
        [0.30, 0.31, 0.29],
        17.,
    );
    block(
        mesh,
        b,
        [-w + 0.3, 0.2, d - 1.1],
        [-w + 0.55, b.height + 0.8, d - 0.45],
        [0.34, 0.34, 0.31],
        17.,
    );
    block(
        mesh,
        b,
        [-w + 0.5, 0.3, d - 1.2],
        [-w + 1.4, 0.45, d - 0.55],
        [0.17, 0.085, 0.03],
        4.,
    );
    let f = b.point(-w + 1., 0.95, d - 0.8);
    let a = b.point(-w + 0.7, 0.45, d - 0.8);
    let c = b.point(-w + 1.3, 0.45, d - 0.8);
    let start = mesh.vertices.len();
    mesh.triangle(a, f, c, [1., 0.45, 0.06], 10.);
    for (v, uv) in mesh.vertices[start..]
        .iter_mut()
        .zip([[0., 1.], [0.5, 0.], [1., 1.]])
    {
        v.uv = uv;
        v.texture = 16.;
    }
    // A clear route remains down the middle of every occupied room.
    let beds = if b.usage == Use::Barracks {
        4
    } else if b.usage == Use::Inn {
        3
    } else {
        2
    };
    for n in 0..beds {
        let x = w - 1.7;
        let z = d - 2.8 - n as f32 * 2.2;
        if z < -d + 1.8 {
            break;
        }
        block(mesh, b, [x, 0.08, z], [x + 1.25, 0.36, z + 1.85], wood, 4.);
        block(
            mesh,
            b,
            [x + 0.05, 0.36, z + 0.06],
            [x + 1.2, 0.58, z + 1.8],
            [0.34, 0.36, 0.31],
            11.,
        );
        block(
            mesh,
            b,
            [x + 0.08, 0.58, z + 1.3],
            [x + 1.15, 0.69, z + 1.76],
            [0.71, 0.68, 0.54],
            11.,
        );
    }
    block(
        mesh,
        b,
        [-w + 0.5, 0.72, -1.5],
        [-w + 2.6, 0.86, 0.2],
        wood,
        4.,
    );
    for x in [-w + 0.62, -w + 2.4] {
        for z in [-1.38, 0.08] {
            block(mesh, b, [x, 0., z], [x + 0.12, 0.75, z + 0.12], wood, 4.);
        }
    }
    block(
        mesh,
        b,
        [-w + 0.5, 0.0, -d + 1.],
        [-w + 1.8, 0.6, -d + 1.8],
        [0.30, 0.19, 0.105],
        4.,
    );
    // Shelves, provisions and books distinguish work interiors without services.
    if b.usage.public() {
        for y in [0.7, 1.35, 2.] {
            block(
                mesh,
                b,
                [-1.4, y, d - 0.8],
                [1.4, y + 0.1, d - 0.28],
                wood,
                4.,
            );
            for n in 0..6 {
                let x = -1.2 + n as f32 * 0.42;
                block(
                    mesh,
                    b,
                    [x, y + 0.1, d - 0.68],
                    [x + 0.25, y + 0.38, d - 0.32],
                    if matches!(b.usage, Use::Arcane | Use::Guild | Use::Temple) {
                        [0.24 + n as f32 * 0.045, 0.23, 0.34]
                    } else {
                        [0.52, 0.40, 0.21]
                    },
                    11.,
                );
            }
        }
    }
    if b.usage == Use::Smithy {
        block(
            mesh,
            b,
            [-1., 0., -0.2],
            [0.2, 0.75, 0.7],
            [0.22, 0.23, 0.24],
            12.,
        );
        block(
            mesh,
            b,
            [-1.3, 0.7, -0.35],
            [0.45, 0.92, 0.85],
            [0.31, 0.33, 0.35],
            12.,
        );
    }
}
fn building(mesh: &mut MeshData, b: &Building, lod: u32) {
    let [w, d] = b.half;
    let h = b.height;
    let wall = match b.id % 4 {
        0 => [0.64, 0.60, 0.46],
        1 => [0.55, 0.55, 0.49],
        2 => [0.64, 0.54, 0.42],
        _ => [0.59, 0.62, 0.56],
    };
    if b.usage == Use::Tent {
        let start = mesh.vertices.len();
        for side in [-1., 1.] {
            mesh.quad(
                b.point(side * w, 0., -d),
                b.point(0., 2.7, -d),
                b.point(0., 2.7, d),
                b.point(side * w, 0., d),
                [0.59, 0.46, 0.28],
                5.,
            );
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
            furniture(mesh, b);
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
    block(mesh, b, [-w, -0.05, -d], [w, 0., d], [0.42, 0.29, 0.17], 4.);
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
    let start = mesh.vertices.len();
    let roof = match b.id % 3 {
        0 => [0.34, 0.20, 0.13],
        1 => [0.27, 0.32, 0.33],
        _ => [0.47, 0.29, 0.16],
    };
    for side in [-1., 1.] {
        mesh.quad(
            b.point(side * (w + 0.5), h, -d - 0.5),
            b.point(0., h + 2.2, -d - 0.5),
            b.point(0., h + 2.2, d + 0.5),
            b.point(side * (w + 0.5), h, d + 0.5),
            roof,
            2.,
        );
    }
    for sign in [-1., 1.] {
        mesh.triangle(
            b.point(-w, h, sign * d),
            b.point(0., h + 2.2, sign * d),
            b.point(w, h, sign * d),
            wall,
            2.,
        );
    }
    tinted(mesh, start, 19.);
    block(
        mesh,
        b,
        [-w, h - 0.08, -d],
        [w, h + 0.02, d],
        [0.25, 0.17, 0.105],
        4.,
    );
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
        furniture(mesh, b);
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
                if b.inside(x, z, 0.) && feet < b.floor + 0.7 {
                    if (p[0] > -w + 0.15 && p[0] < -w + 2.95 && p[1] > -1.85 && p[1] < 0.55)
                        || (p[0] > w - 2.0 && p[0] < w - 0.1 && p[1] > d - 5.4 && p[1] < d - 0.5)
                        || (p[0] < -w + 2.0 && p[1] > d - 1.95)
                    {
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
