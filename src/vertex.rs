//! Compact upload format. Terrain/water positions and data remain full precision;
//! normalized directions and 0..1 card UVs need fewer bytes on the GPU.
use crate::geometry::Vertex;
use bytemuck::{Pod, Zeroable};
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PackedVertex {
    position: [f32; 3],
    normal: [i8; 4],
    color: [f32; 3],
    material: f32,
    uv: [u16; 2],
    texture: f32,
}
impl From<&Vertex> for PackedVertex {
    fn from(v: &Vertex) -> Self {
        Self {
            position: v.position,
            normal: [
                (v.normal[0].clamp(-1., 1.) * 127.).round() as i8,
                (v.normal[1].clamp(-1., 1.) * 127.).round() as i8,
                (v.normal[2].clamp(-1., 1.) * 127.).round() as i8,
                0,
            ],
            color: v.color,
            material: v.material,
            uv: v.uv.map(|u| {
                // Architecture carries signed metric UVs; preserve repeats in
                // the same four bytes as normalized foliage/card UVs.
                let u = if (100.0..200.0).contains(&v.texture) {
                    u / 128.0 + 0.5
                } else {
                    u
                };
                (u.clamp(0., 1.) * 65535.).round() as u16
            }),
            texture: v.texture,
        }
    }
}
pub const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0=>Float32x3,1=>Snorm8x4,2=>Float32x3,3=>Float32,4=>Unorm16x2,5=>Float32];
pub fn pack(vertices: &[Vertex], indices: &[u32]) -> (Vec<PackedVertex>, Vec<u32>) {
    let mut unique = std::collections::HashMap::<[u32; 10], u32>::with_capacity(vertices.len() / 2);
    let mut packed = Vec::with_capacity(vertices.len());
    let mut remap = Vec::with_capacity(vertices.len());
    for vertex in vertices {
        let v = PackedVertex::from(vertex);
        let key: [u32; 10] = bytemuck::cast(v);
        let next = packed.len() as u32;
        let index = *unique.entry(key).or_insert_with(|| {
            packed.push(v);
            next
        });
        remap.push(index);
    }
    (packed, indices.iter().map(|i| remap[*i as usize]).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metric_architecture_uvs_survive_gpu_packing_without_clamping() {
        let v = Vertex {
            position: [0.; 3],
            normal: [0., 1., 0.],
            color: [0.5; 3],
            material: 2.,
            uv: [-12.25, 8.5],
            texture: 120.,
        };
        let packed = PackedVertex::from(&v);
        for i in 0..2 {
            let decoded = (packed.uv[i] as f32 / 65535. - 0.5) * 128.;
            assert!((decoded - v.uv[i]).abs() < 0.0011);
        }
        let card = PackedVertex::from(&Vertex {
            uv: [0., 1.],
            texture: 5.,
            ..v
        });
        assert_eq!(card.uv, [0, 65535]);
    }
    #[test]
    fn packing_preserves_water_and_shares_card_corners() {
        let a = Vertex {
            position: [-77000.25, 0.123, 55123.],
            normal: [0., 1., 0.],
            color: [-1.4, 2.2, -184.3],
            material: 4.,
            uv: [0., 1.],
            texture: -1.,
        };
        let (vertices, indices) = pack(&[a, a, a], &[0, 1, 2]);
        assert_eq!(std::mem::size_of::<PackedVertex>(), 40);
        assert_eq!(vertices.len(), 1);
        assert_eq!(indices, [0, 0, 0]);
        assert_eq!(vertices[0].position, a.position);
        assert_eq!(vertices[0].color, a.color);
        assert_eq!(vertices[0].uv, [0, 65535]);
    }
}
