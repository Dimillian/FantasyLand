//! Small ballistic sheets at real steep river reaches. The river profile owns
//! both endpoints; no unrelated decorative water plane or new render pass.
use crate::{
    geometry::{terrain_surface_height_lod, MeshData, CHUNK_SIZE},
    world::World,
};
use serde::Serialize;
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Fall {
    pub id: u32,
    pub lip: [f32; 3],
    pub toe: [f32; 3],
    pub width: f32,
}
impl Fall {
    pub fn point(&self, u: f32, side: f32) -> [f32; 3] {
        let dx = self.toe[0] - self.lip[0];
        let dz = self.toe[2] - self.lip[2];
        let len = dx.hypot(dz).max(0.01);
        let horizontal = u;
        let vertical = u * u;
        let spread = self.width * (0.85 + 0.15 * u) * side;
        [
            self.lip[0] + dx * horizontal - dz / len * spread,
            self.lip[1] + (self.toe[1] - self.lip[1]) * vertical,
            self.lip[2] + dz * horizontal + dx / len * spread,
        ]
    }
}
pub fn append(world: &World, mesh: &mut MeshData, cx: i32, cz: i32, lod: u32) {
    if lod > 2 {
        return;
    }
    let center = [
        (cx as f32 + 0.5) * CHUNK_SIZE,
        (cz as f32 + 0.5) * CHUNK_SIZE,
    ];
    let mut emitted = 0;
    for f in world.waterfalls_near(center[0], center[1], CHUNK_SIZE) {
        if (f.lip[0] / CHUNK_SIZE).floor() as i32 != cx
            || (f.lip[2] / CHUNK_SIZE).floor() as i32 != cz
        {
            continue;
        }
        if emitted >= 2 {
            break;
        }
        emitted += 1;
        let rows = if lod == 0 { 10 } else { 6 };
        let columns = 4;
        let direction = [f.toe[0] - f.lip[0], f.toe[2] - f.lip[2]];
        let length = direction[0].hypot(direction[1]).max(0.01);
        let color = [direction[0] / length * 8., direction[1] / length * 8., -4.];
        for row in 0..rows {
            for col in 0..columns {
                let u = row as f32 / rows as f32;
                let v = (row + 1) as f32 / rows as f32;
                let a = col as f32 / columns as f32 * 2. - 1.;
                let b = (col + 1) as f32 / columns as f32 * 2. - 1.;
                let points = [f.point(u, a), f.point(v, a), f.point(v, b), f.point(u, b)];
                // Trim to the rendered river banks, including coarse LOD triangles.
                if points
                    .iter()
                    .any(|p| p[1] < terrain_surface_height_lod(world, p[0], p[2], lod) + 0.02)
                {
                    continue;
                }
                let start = mesh.vertices.len();
                mesh.quad(points[3], points[2], points[1], points[0], color, 4.);
                for v in &mut mesh.vertices[start..] {
                    v.uv[1] = 1.;
                }
            }
        }
    }
}
