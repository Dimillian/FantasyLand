//! Cultural landmark and campsite mesh recipes, grounded by shared terrain helpers.
use super::*;

pub(super) fn banner(mesh: &mut MeshData, base: [f32; 3], height: f32, seed: u32) {
    trunk(mesh, base, 0.16, height, 0.0, [0.37, 0.27, 0.16]);
    let red = mix([0.48, 0.17, 0.12], [0.27, 0.31, 0.42], random(seed, 135));
    let [x, y, z] = base;
    let a = [x + 0.12, y + height - 0.3, z];
    let b = [x + 2.9, y + height - 0.45, z + 0.2];
    let c = [x + 2.65, y + height - 2.0, z + 0.3];
    let d = [x + 0.12, y + height - 2.25, z];
    mesh.quad(a, b, c, d, red, 5.0);
    mesh.quad(d, c, b, a, mul(red, 0.95), 5.0);
    box_mesh(
        mesh,
        [x + 1.2, y + height - 1.15, z + 0.10],
        [0.24, 1.35, 0.025],
        0.0,
        [0.73, 0.62, 0.36],
        5.0,
    );
}
// Structural heights follow the displayed terrain. Footings extend through all
// nearby terrain LODs so a structure straddling a chunk boundary cannot float.
pub(super) fn structural_ground_range(
    world: &World,
    center: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    lod: u32,
) -> (f32, f32) {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    let nx = (size[0] / 0.5).ceil().max(1.0) as u32;
    let nz = (size[1] / 0.5).ceil().max(1.0) as u32;
    for iz in 0..=nz {
        for ix in 0..=nx {
            let dx = (ix as f32 / nx as f32 - 0.5) * size[0];
            let dz = (iz as f32 / nz as f32 - 0.5) * size[1];
            let x = center[0] + dx * yaw.cos() + dz * yaw.sin();
            let z = center[1] - dx * yaw.sin() + dz * yaw.cos();
            let ground = terrain_surface_height_lod(world, x, z, lod);
            low = low.min(ground);
            high = high.max(ground);
        }
    }
    (low, high)
}
pub(super) fn structural_footing_bottom(
    world: &World,
    center: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    lod: u32,
) -> f32 {
    let mut bottom = structural_ground_range(world, center, size, yaw, lod).0;
    for level in 0..=3 {
        bottom = bottom.min(structural_ground_range(world, center, size, yaw, level).0);
    }
    bottom - 0.25
}
pub(super) fn structural_support(
    world: &World,
    mesh: &mut MeshData,
    center: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    top: f32,
    color: [f32; 3],
    material: f32,
    lod: u32,
) {
    let bottom = structural_footing_bottom(world, center, size, yaw, lod);
    box_mesh(
        mesh,
        [center[0], (bottom + top) * 0.5, center[1]],
        [size[0], (top - bottom).max(0.02), size[1]],
        yaw,
        color,
        material,
    );
}
pub(super) fn structural_foundation(
    world: &World,
    mesh: &mut MeshData,
    center: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    rise: f32,
    color: [f32; 3],
    lod: u32,
) -> f32 {
    let top = structural_ground_range(world, center, size, yaw, lod).1 + rise;
    structural_support(world, mesh, center, size, yaw, top, color, 2.0, lod);
    top
}
pub(super) fn grounded_banner(
    world: &World,
    mesh: &mut MeshData,
    center: [f32; 2],
    height: f32,
    seed: u32,
    lod: u32,
) {
    let ground = structural_ground_range(world, center, [0.34, 0.34], 0.0, lod).1;
    let bottom = structural_footing_bottom(world, center, [0.34, 0.34], 0.0, lod);
    banner(
        mesh,
        [center[0], bottom, center[1]],
        ground + height - bottom,
        seed,
    );
}
pub(super) fn structural_stone(
    world: &World,
    mesh: &mut MeshData,
    center: [f32; 2],
    scale: f32,
    seed: u32,
    lod: u32,
) {
    let size = scale * (1.2 + random(seed, 90) * 1.4);
    let y = structural_ground_range(world, center, [size * 2.4, size * 2.1], 0.0, lod).1;
    // The short buried core supports the faceted lower shell at steep or mixed
    // LOD edges; the visible silhouette stays the ordinary generated stone.
    structural_support(
        world,
        mesh,
        center,
        [size * 0.85, size * 0.7],
        0.0,
        y + size * 0.43,
        [0.43, 0.44, 0.40],
        2.0,
        lod,
    );
    rock(mesh, [center[0], y - size * 0.10, center[1]], scale, seed);
}
pub(super) fn settlement_marker(
    world: &World,
    mesh: &mut MeshData,
    base: [f32; 3],
    seed: u32,
    kind: &str,
    lod: u32,
) {
    let [x, _, z] = base;
    let y = structural_ground_range(world, [x, z], [7.1, 7.1], 0.0, lod).1 + 0.12;
    let height = if kind == "town" { 12.0 } else { 8.0 };
    let wood = [0.35, 0.27, 0.18];
    for dx in [-2.8, 2.8] {
        for dz in [-2.8, 2.8] {
            structural_support(
                world,
                mesh,
                [x + dx, z + dz],
                [0.6, 0.6],
                0.0,
                y + height + 0.2,
                wood,
                3.0,
                lod,
            );
        }
    }
    let deck = y + height - 2.0;
    box_mesh(mesh, [x, deck, z], [7.1, 0.45, 7.1], 0.0, wood, 3.0);
    cone(
        mesh,
        [x, y + height + 0.2, z],
        5.3,
        3.2,
        4,
        PI / 4.0,
        [0.36, 0.23, 0.17],
        3.0,
    );
    grounded_banner(world, mesh, [x + 3.4, z + 3.4], height + 5.0, seed, lod);
    let ladder_ground = structural_ground_range(world, [x - 2.65, z + 3.5], [1.2, 0.3], 0.0, lod).1;
    for dx in [-3.17, -2.12] {
        structural_support(
            world,
            mesh,
            [x + dx, z + 3.5],
            [0.15, 0.15],
            0.0,
            deck + 0.8,
            wood,
            3.0,
            lod,
        );
    }
    let rungs = ((deck - ladder_ground) / 0.7).ceil().max(1.0) as u32;
    for j in 0..rungs {
        let h = ladder_ground + (deck - ladder_ground) * (j + 1) as f32 / rungs as f32;
        box_mesh(
            mesh,
            [x - 2.65, h, z + 3.5],
            [1.15, 0.12, 0.28],
            0.0,
            wood,
            3.0,
        );
    }
    signpost(world, mesh, [x + 8.0, 0.0, z + 5.0], seed, lod);
    camp(world, mesh, [x - 10.0, 0.0, z + 8.0], seed ^ 31, lod);
}
pub(super) fn signpost(world: &World, mesh: &mut MeshData, base: [f32; 3], seed: u32, lod: u32) {
    let [x, _, z] = base;
    let yaw = random(seed, 141) * 0.5;
    let y = structural_ground_range(world, [x, z], [0.34, 0.34], yaw, lod).1;
    structural_support(
        world,
        mesh,
        [x, z],
        [0.34, 0.34],
        yaw,
        y + 5.2,
        [0.35, 0.25, 0.15],
        3.0,
        lod,
    );
    box_mesh(
        mesh,
        [x, y + 4.25, z],
        [3.6, 0.75, 0.20],
        yaw,
        [0.49, 0.36, 0.20],
        3.0,
    );
    box_mesh(
        mesh,
        [x - 0.35, y + 3.2, z],
        [2.9, 0.68, 0.20],
        yaw,
        [0.43, 0.31, 0.18],
        3.0,
    );
    box_mesh(
        mesh,
        [
            x + 0.92 * yaw.cos() + 0.13 * yaw.sin(),
            y + 4.25,
            z - 0.92 * yaw.sin() + 0.13 * yaw.cos(),
        ],
        [0.75, 0.12, 0.06],
        yaw,
        [0.18, 0.17, 0.12],
        3.0,
    );
}
pub(super) fn landmark_mesh(
    world: &World,
    mesh: &mut MeshData,
    base: [f32; 3],
    seed: u32,
    kind: &str,
    lod: u32,
) {
    let [x, _, z] = base;
    match kind {
        "watchtower" => {
            let stone = [0.47, 0.46, 0.39];
            let y = structural_ground_range(world, [x, z], [7.3, 7.3], 0.0, lod).1 + 0.12;
            for dx in [-2.7, 2.7] {
                for dz in [-2.7, 2.7] {
                    structural_support(
                        world,
                        mesh,
                        [x + dx, z + dz],
                        [1.5, 1.5],
                        0.0,
                        y + 18.0,
                        stone,
                        2.0,
                        lod,
                    );
                }
            }
            for (center, size) in [
                ([x, z - 2.7], [4.0, 1.5]),
                ([x - 2.7, z], [1.5, 4.0]),
                ([x + 2.7, z], [1.5, 4.0]),
            ] {
                structural_support(world, mesh, center, size, 0.0, y + 14.0, stone, 2.0, lod);
            }
            box_mesh(
                mesh,
                [x, y + 9.5, z + 2.7],
                [4.0, 9.0, 1.5],
                0.0,
                stone,
                2.0,
            );
            box_mesh(
                mesh,
                [x, y + 14.4, z],
                [7.3, 0.7, 7.3],
                0.0,
                [0.37, 0.37, 0.33],
                2.0,
            );
            cone(
                mesh,
                [x, y + 18.0, z],
                6.0,
                4.0,
                4,
                PI / 4.0,
                [0.36, 0.23, 0.18],
                3.0,
            );
            grounded_banner(world, mesh, [x + 4.0, z + 4.0], 9.0, seed, lod);
        }
        "ruin" => {
            let stone = [0.48, 0.46, 0.39];
            for i in 1..7 {
                let a = i as f32 * TAU / 8.0;
                let center = [x + a.sin() * 6.5, z + a.cos() * 6.5];
                let h = 3.0 + random(seed, 150 + i) * 7.0;
                let y = structural_ground_range(world, center, [2.5, 1.6], a, lod).1;
                structural_support(world, mesh, center, [2.5, 1.6], a, y + h, stone, 2.0, lod);
            }
            let arch = structural_ground_range(world, [x, z + 6.5], [7.0, 1.6], 0.0, lod).1 + 6.0;
            for dx in [-2.6, 2.6] {
                structural_support(
                    world,
                    mesh,
                    [x + dx, z + 6.5],
                    [1.6, 1.6],
                    0.0,
                    arch,
                    stone,
                    2.0,
                    lod,
                );
            }
            box_mesh(
                mesh,
                [x, arch + 0.7, z + 6.5],
                [7.0, 1.4, 1.6],
                0.0,
                stone,
                2.0,
            );
            for i in 0..5 {
                let a = random(seed, 165 + i) * TAU;
                structural_stone(
                    world,
                    mesh,
                    [x + a.sin() * 9.0, z + a.cos() * 9.0],
                    0.8,
                    seed ^ i,
                    lod,
                );
            }
        }
        "standing_stones" => {
            for i in 0..7 {
                let a = i as f32 * TAU / 7.0;
                let h = 4.0 + random(seed, 180 + i) * 3.5;
                let center = [x + a.sin() * 7.2, z + a.cos() * 7.2];
                let y = structural_ground_range(world, center, [3.2, 2.4], 0.0, lod).1;
                structural_support(
                    world,
                    mesh,
                    center,
                    [1.3, 1.0],
                    0.0,
                    y + h * 0.3,
                    [0.43, 0.45, 0.43],
                    2.0,
                    lod,
                );
                polyhedron(
                    mesh,
                    [center[0], y + h * 0.43, center[1]],
                    [1.6, h * 0.85, 1.2],
                    seed ^ i,
                    [0.43, 0.45, 0.43],
                    2.0,
                );
            }
            structural_foundation(
                world,
                mesh,
                [x, z],
                [3.5, 2.3],
                random(seed, 188),
                1.0,
                [0.46, 0.45, 0.41],
                lod,
            );
        }
        "camp" => camp(world, mesh, base, seed, lod),
        "shrine" => {
            let stone = [0.53, 0.51, 0.43];
            let slab = structural_foundation(world, mesh, [x, z], [6.0, 6.0], 0.0, 0.7, stone, lod);
            box_mesh(mesh, [x, slab + 0.25, z], [4.5, 0.5, 4.5], 0.0, stone, 2.0);
            for dx in [-1.8, 1.8] {
                for dz in [-1.8, 1.8] {
                    box_mesh(
                        mesh,
                        [x + dx, slab + 2.4, z + dz],
                        [0.5, 4.0, 0.5],
                        0.0,
                        stone,
                        2.0,
                    );
                }
            }
            cone(
                mesh,
                [x, slab + 4.4, z],
                3.8,
                2.1,
                4,
                PI / 4.0,
                [0.32, 0.36, 0.34],
                2.0,
            );
            polyhedron(
                mesh,
                [x, slab + 1.6, z],
                [0.9, 1.35, 0.8],
                seed,
                [0.61, 0.56, 0.37],
                2.0,
            );
            grounded_banner(world, mesh, [x + 5.0, z], 6.2, seed, lod);
        }
        _ => {
            let y = structural_foundation(
                world,
                mesh,
                [x, z],
                [2.7, 2.2],
                0.0,
                0.15,
                [0.45, 0.44, 0.39],
                lod,
            );
            for i in 0..4 {
                let f = i as f32;
                polyhedron(
                    mesh,
                    [x, y + 0.6 + f, z],
                    [2.0 - f * 0.34, 1.0, 1.6 - f * 0.28],
                    seed ^ i,
                    [0.45, 0.44, 0.39],
                    2.0,
                );
            }
            signpost(world, mesh, [x + 3.0, 0.0, z], seed, lod);
        }
    }
}
pub(super) fn camp(world: &World, mesh: &mut MeshData, base: [f32; 3], seed: u32, lod: u32) {
    let [x, _, z] = base;
    let y = structural_foundation(
        world,
        mesh,
        [x, z],
        [6.25, 6.25],
        0.0,
        0.10,
        [0.39, 0.35, 0.27],
        lod,
    );
    let cloth = mix([0.57, 0.46, 0.30], [0.44, 0.35, 0.23], random(seed, 200));
    let a = [x - 3.0, y, z - 3.0];
    let b = [x + 3.0, y, z - 3.0];
    let c = [x, y + 3.8, z - 3.0];
    let d = [x - 3.0, y, z + 3.0];
    let e = [x + 3.0, y, z + 3.0];
    let f = [x, y + 3.8, z + 3.0];
    mesh.quad(a, d, f, c, cloth, 5.0);
    mesh.quad(c, f, e, b, mul(cloth, 0.91), 5.0);
    mesh.triangle(a, c, b, mul(cloth, 0.8), 5.0);
    mesh.triangle(d, [x - 0.8, y, z + 3.0], f, mul(cloth, 0.85), 5.0);
    mesh.triangle(f, [x + 0.8, y, z + 3.0], e, mul(cloth, 0.85), 5.0);
    for dz in [-3.0, 3.0] {
        trunk(
            mesh,
            [x, y - 0.04, z + dz],
            0.09,
            3.85,
            0.0,
            [0.34, 0.25, 0.16],
        );
    }
    for dx in [-3.15, 3.15] {
        let center = [x + dx, z + 3.2];
        let ground = structural_ground_range(world, center, [0.16, 0.16], 0.0, lod).1;
        structural_support(
            world,
            mesh,
            center,
            [0.16, 0.16],
            0.0,
            ground + 1.3,
            [0.34, 0.25, 0.16],
            3.0,
            lod,
        );
    }
    let fire = structural_foundation(
        world,
        mesh,
        [x + 6.0, z],
        [4.0, 4.0],
        0.0,
        0.04,
        [0.33, 0.31, 0.25],
        lod,
    );
    for i in 0..7 {
        let a = i as f32 * TAU / 7.0;
        rock(
            mesh,
            [x + 6.0 + a.sin() * 1.2, fire, z + a.cos() * 1.2],
            0.24,
            seed ^ i,
        );
    }
    box_mesh(
        mesh,
        [x + 6.0, fire + 0.18, z],
        [1.8, 0.4, 0.38],
        0.8,
        [0.24, 0.18, 0.12],
        3.0,
    );
    box_mesh(
        mesh,
        [x + 6.0, fire + 0.34, z],
        [1.8, 0.4, 0.38],
        -0.8,
        [0.28, 0.20, 0.13],
        3.0,
    );
    let flame_start = mesh.vertices.len();
    cone(
        mesh,
        [x + 6.0, fire + 0.40, z],
        0.35,
        0.75,
        5,
        0.0,
        [1.0, 0.66, 0.19],
        10.0,
    );
    for vertex in &mut mesh.vertices[flame_start..] {
        let dx = vertex.position[0] - (x + 6.0);
        let dz = vertex.position[2] - z;
        vertex.uv = [
            0.5 + dx.atan2(dz) / TAU,
            ((vertex.position[1] - fire - 0.40) / 0.75).clamp(0.0, 1.0),
        ];
        vertex.texture = 16.0;
    }
    // A fallen seat/log follows its own local slope. Its lower endpoints are
    // embedded, and small support feet also reach the adjacent terrain LODs.
    let a = [x + 4.0 - 1.5 * 0.1_f32.cos(), z + 3.0 + 1.5 * 0.1_f32.sin()];
    let b = [x + 4.0 + 1.5 * 0.1_f32.cos(), z + 3.0 - 1.5 * 0.1_f32.sin()];
    let ya = structural_ground_range(world, a, [0.6, 0.6], 0.0, lod).0 + 0.50;
    let yb = structural_ground_range(world, b, [0.6, 0.6], 0.0, lod).0 + 0.50;
    sloped_box(mesh, a, b, ya, yb, 0.6, 0.55, [0.35, 0.26, 0.17], 3.0);
    for (center, top) in [(a, ya), (b, yb)] {
        structural_support(
            world,
            mesh,
            center,
            [0.42, 0.45],
            0.1,
            top - 0.1,
            [0.35, 0.26, 0.17],
            3.0,
            lod,
        );
    }
}

// Each deck endpoint meets the higher of the bank or the river clearance.
// Adjacent spans share endpoints, so bank ramps and walking height are continuous.
