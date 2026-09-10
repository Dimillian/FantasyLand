//! Coarse, streamed terrain continues the actual world beyond the detailed chunks.
use crate::{
    geometry::{MeshData, Vertex},
    world::{Biome, World, WORLD_SIZE},
};
use glam::Vec3;

pub const PATCH_SIZE: f32 = 1536.0;

pub fn patch(world: &World, cx: i32, cz: i32) -> MeshData {
    const N: usize = 16;
    let mut grid = Vec::with_capacity((N + 1) * (N + 1));
    for z in 0..=N {
        for x in 0..=N {
            let wx = cx as f32 * PATCH_SIZE + x as f32 * PATCH_SIZE / N as f32;
            let wz = cz as f32 * PATCH_SIZE + z as f32 * PATCH_SIZE / N as f32;
            let sample = world.sample(wx, wz);
            let mut color = match sample.biome {
                Biome::Grassland => [0.39, 0.43, 0.20],
                Biome::Forest => [0.28, 0.35, 0.17],
                Biome::PineForest => [0.29, 0.36, 0.26],
                Biome::Moor => [0.40, 0.39, 0.28],
                Biome::Alpine => [0.49, 0.51, 0.48],
                Biome::Desert => [0.63, 0.51, 0.32],
                Biome::Wetland => [0.31, 0.37, 0.25],
            };
            if sample.height < sample.water_height {
                color = [0.20, 0.35, 0.40];
            }
            grid.push((
                Vec3::new(wx, sample.height.max(sample.water_height), wz),
                color,
            ));
        }
    }
    let mut mesh = MeshData::default();
    let at = |x: usize, z: usize| grid[z * (N + 1) + x];
    let half = WORLD_SIZE * 0.5;
    for z in 0..N {
        for x in 0..N {
            let a = at(x, z);
            let b = at(x, z + 1);
            let c = at(x + 1, z + 1);
            let d = at(x + 1, z);
            if a.0.x < -half || a.0.z < -half || c.0.x > half || c.0.z > half {
                continue;
            }
            for points in [[a, b, d], [b, c, d]] {
                let normal = (points[1].0 - points[0].0)
                    .cross(points[2].0 - points[0].0)
                    .normalize_or_zero()
                    .to_array();
                let color = std::array::from_fn(|i| {
                    (points[0].1[i] + points[1].1[i] + points[2].1[i]) / 3.
                });
                let start = mesh.vertices.len() as u32;
                for (position, _) in points {
                    mesh.vertices.push(Vertex {
                        position: position.to_array(),
                        normal,
                        color,
                        material: 7.,
                    });
                }
                mesh.indices.extend([start, start + 1, start + 2]);
            }
        }
    }
    mesh
}
