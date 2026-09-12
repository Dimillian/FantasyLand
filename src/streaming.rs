//! Worker-safe generation and packed transfer packets. No GPU or DOM access.
use crate::{cover, geometry, horizon, vertex, world::World};
use glam::Vec3;
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Job {
    pub kind: u32, // terrain, props, horizon, canopy, cover
    pub x: i32,
    pub z: i32,
    pub lod: u32,
    pub detail: u8,
}

pub struct PackedMesh {
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub bounds: [Vec3; 2],
}
impl PackedMesh {
    pub fn new(data: geometry::MeshData) -> Self {
        let mut bounds = [Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)];
        for v in &data.vertices {
            let p = Vec3::from_array(v.position);
            bounds[0] = bounds[0].min(p);
            bounds[1] = bounds[1].max(p);
        }
        if data.indices.is_empty() {
            bounds = [Vec3::ZERO; 2];
        }
        bounds[0] -= Vec3::ONE;
        bounds[1] += Vec3::ONE;
        let (vertices, indices) = vertex::pack(&data.vertices, &data.indices);
        Self {
            vertices: bytemuck::cast_slice(&vertices).to_vec(),
            indices: bytemuck::cast_slice(&indices).to_vec(),
            bounds,
        }
    }
}
pub enum Payload {
    Meshes(Vec<PackedMesh>),
    Cover(cover::TileData),
}

pub fn generate(world: &World, job: Job) -> Payload {
    let Job {
        kind,
        x,
        z,
        lod,
        detail,
    } = job;
    let meshes = match kind {
        0 => vec![
            geometry::terrain_chunk(world, x, z, lod),
            geometry::water_chunk(world, x, z, lod),
        ],
        1 => {
            if detail > 0 {
                vec![
                    geometry::props_chunk_at_lod(world, x, z, false, lod),
                    geometry::reflection_props_chunk(world, x, z, lod),
                ]
            } else {
                vec![
                    geometry::distant_props_chunk_at_lod(world, x, z, lod),
                    geometry::MeshData::default(),
                ]
            }
        }
        2 => horizon::patch_parts(world, x, z).into_iter().collect(),
        3 => vec![horizon::canopy(world, x, z)],
        4 => return Payload::Cover(cover::tile_data(world, x, z)),
        _ => vec![],
    };
    Payload::Meshes(meshes.into_iter().map(PackedMesh::new).collect())
}

fn word(out: &mut Vec<u8>, n: u32) {
    out.extend(n.to_le_bytes());
}
fn floats(out: &mut Vec<u8>, values: &[f32]) {
    out.extend(bytemuck::cast_slice(values));
}
pub fn encode(payload: Payload) -> Vec<u8> {
    let mut out = Vec::new();
    word(&mut out, 0x46534c31);
    match payload {
        Payload::Meshes(meshes) => {
            word(&mut out, meshes.len() as u32);
            for mesh in meshes {
                floats(&mut out, &mesh.bounds[0].to_array());
                floats(&mut out, &mesh.bounds[1].to_array());
                word(&mut out, mesh.vertices.len() as u32);
                word(&mut out, mesh.indices.len() as u32);
                out.extend(mesh.vertices);
                out.extend(mesh.indices);
            }
        }
        Payload::Cover(tile) => {
            word(&mut out, u32::MAX);
            word(&mut out, tile.seed);
            floats(&mut out, &tile.origin);
            floats(&mut out, &tile.bounds_min);
            floats(&mut out, &tile.bounds_max);
            word(&mut out, tile.instances.len() as u32);
            out.extend(bytemuck::cast_slice(&tile.instances));
            floats(&mut out, &tile.ranks);
            floats(&mut out, &tile.heights);
            word(&mut out, tile.meadow.len() as u32);
            out.extend(bytemuck::cast_slice(&tile.meadow));
        }
    }
    out
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if count > self.0.len() {
            return None;
        }
        let (value, rest) = self.0.split_at(count);
        self.0 = rest;
        Some(value)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_bits(self.u32()?))
    }
    fn array<const N: usize>(&mut self) -> Option<[f32; N]> {
        let mut a = [0.; N];
        for f in &mut a {
            *f = self.f32()?;
            if !f.is_finite() {
                return None;
            }
        }
        Some(a)
    }
}
pub fn decode(bytes: &[u8]) -> Option<Payload> {
    let mut r = Reader(bytes);
    if r.u32()? != 0x46534c31 {
        return None;
    }
    let count = r.u32()?;
    let result = if count == u32::MAX {
        let seed = r.u32()?;
        let origin = r.array()?;
        let bounds_min = r.array()?;
        let bounds_max = r.array()?;
        if (0..3).any(|i| bounds_min[i] > bounds_max[i]) {
            return None;
        }
        let n = r.u32()? as usize;
        if n > 1024 {
            return None;
        }
        let instances: Vec<cover::CoverInstance> = r
            .take(n * std::mem::size_of::<cover::CoverInstance>())?
            .chunks_exact(std::mem::size_of::<cover::CoverInstance>())
            .map(bytemuck::pod_read_unaligned)
            .collect();
        if instances.iter().any(|p| {
            p.placement
                .iter()
                .chain(p.rotation.iter())
                .any(|v| !v.is_finite())
                || p.placement[2] <= 0.
                || p.placement[2] > 16.
                || p.data[1] >= crate::plants::KINDS * 8
        }) {
            return None;
        }
        let mut ranks = Vec::with_capacity(n);
        for _ in 0..n {
            ranks.push(r.f32()?);
        }
        if ranks.iter().any(|v| !v.is_finite() || *v < 0. || *v > 4.)
            || ranks.windows(2).any(|p| p[0] > p[1])
        {
            return None;
        }
        let heights = r.array::<121>()?.to_vec();
        let n = r.u32()? as usize;
        if n > 1024 {
            return None;
        }
        let meadow: Vec<crate::meadow::MeadowCell> = r
            .take(n * 16)?
            .chunks_exact(16)
            .map(bytemuck::pod_read_unaligned)
            .collect();
        if meadow.iter().any(|c| !c.valid()) {
            return None;
        }
        Payload::Cover(cover::TileData {
            seed,
            origin,
            bounds_min,
            bounds_max,
            instances,
            ranks,
            heights,
            meadow,
        })
    } else {
        if count > 3 {
            return None;
        }
        let mut meshes = Vec::new();
        for _ in 0..count {
            let bounds = [Vec3::from_array(r.array()?), Vec3::from_array(r.array()?)];
            if (0..3).any(|i| bounds[0][i] > bounds[1][i]) {
                return None;
            }
            let nv = r.u32()? as usize;
            let ni = r.u32()? as usize;
            if nv % 40 != 0 || ni % 12 != 0 {
                return None;
            }
            let vertices = r.take(nv)?.to_vec();
            let indices = r.take(ni)?.to_vec();
            if indices
                .chunks_exact(4)
                .any(|i| u32::from_le_bytes(i.try_into().unwrap()) as usize >= nv / 40)
            {
                return None;
            }
            meshes.push(PackedMesh {
                vertices,
                indices,
                bounds,
            });
        }
        Payload::Meshes(meshes)
    };
    r.0.is_empty().then_some(result)
}

#[wasm_bindgen]
pub struct StreamGenerator {
    world: World,
}
#[wasm_bindgen]
impl StreamGenerator {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32) -> Self {
        Self {
            world: World::new(seed),
        }
    }
    pub fn generate(&self, kind: u32, x: i32, z: i32, lod: u32, detail: u8) -> Vec<u8> {
        encode(generate(
            &self.world,
            Job {
                kind,
                x,
                z,
                lod: lod.min(3),
                detail: detail.min(1),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cover_transfer_rejects_invalid_geometry_and_bounds() {
        let tile = || cover::TileData {
            seed: 1337,
            origin: [0.; 2],
            bounds_min: [0.; 3],
            bounds_max: [48., 2., 48.],
            heights: vec![0.; 121],
            ranks: vec![1.],
            meadow: Vec::new(),
            instances: vec![cover::CoverInstance {
                placement: [12., 12., 1.],
                rotation: [0., 1.],
                data: [0, 63],
            }],
        };
        assert!(decode(&encode(Payload::Cover(tile()))).is_some());
        for case in 0..4 {
            let mut bad = tile();
            match case {
                0 => bad.instances[0].placement[0] = f32::NAN,
                1 => bad.instances[0].placement[2] = -1.,
                2 => bad.instances[0].data[1] = 64,
                _ => bad.bounds_max[0] = -1.,
            }
            assert!(decode(&encode(Payload::Cover(bad))).is_none());
        }
    }
    #[test]
    fn packed_transfer_round_trip_and_truncation() {
        let mut m = geometry::MeshData::default();
        m.quad(
            [0., 0., 0.],
            [1., 0., 0.],
            [1., 1., 0.],
            [0., 1., 0.],
            [0.5; 3],
            1.,
        );
        let expected = PackedMesh::new(m);
        let vertices = expected.vertices.clone();
        let bytes = encode(Payload::Meshes(vec![expected]));
        let Some(Payload::Meshes(result)) = decode(&bytes) else {
            panic!("invalid packet")
        };
        assert_eq!(result[0].vertices, vertices);
        assert_eq!(result[0].indices.len(), 24);
        assert!(decode(&bytes[..bytes.len() - 1]).is_none());
        assert!(decode(&[0; 8]).is_none());
    }
}
