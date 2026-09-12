//! Procedural plant communities built from tiny textured cutout meshes. Local
//! UVs preserve readable leaf/grass silhouettes while all roots stay at y=0.
//! Each instanced template fits the existing 72-vertex / 24-triangle budget.
use crate::{
    geometry::MeshData,
    world::{hash, rand01},
};
use std::f32::consts::{FRAC_PI_2, TAU};

pub const KINDS: u32 = 8;
pub const TRIANGLES: u32 = 24;
fn r(seed: u32, salt: u32) -> f32 {
    rand01(hash(seed, salt as i32, 197))
}
fn point(base: [f32; 3], a: f32, along: f32, side: f32, y: f32) -> [f32; 3] {
    [
        base[0] + a.sin() * along + a.cos() * side,
        base[1] + y,
        base[2] + a.cos() * along - a.sin() * side,
    ]
}
fn card(m: &mut MeshData, corners: [[f32; 3]; 4], color: [f32; 3], texture: f32, v: [f32; 2]) {
    let first = m.vertices.len();
    m.quad(corners[0], corners[1], corners[2], corners[3], color, 6.);
    let uv = [
        [0., v[0]],
        [1., v[0]],
        [1., v[1]],
        [0., v[0]],
        [1., v[1]],
        [0., v[1]],
    ];
    for (vertex, uv) in m.vertices[first..].iter_mut().zip(uv) {
        vertex.uv = uv;
        vertex.texture = texture;
    }
}
fn upright(
    m: &mut MeshData,
    base: [f32; 3],
    a: f32,
    h: f32,
    width: f32,
    bend: f32,
    c: [f32; 3],
    texture: f32,
) {
    card(
        m,
        [
            point(base, a, 0., -width, 0.),
            point(base, a, 0., width, 0.),
            point(base, a, bend, width * 0.86, h),
            point(base, a, bend, -width * 0.86, h),
        ],
        c,
        texture,
        [1., 0.],
    );
}
fn tuft(m: &mut MeshData, seed: u32, tall: bool) {
    for i in 0..6 {
        let a = r(seed, 10 + i) * TAU;
        let radius = r(seed, 30 + i) * 0.36;
        let base = [a.sin() * radius, 0., a.cos() * radius];
        upright(
            m,
            base,
            a + i as f32 * 0.78,
            if tall {
                0.82 + r(seed, 50 + i) * 0.66
            } else {
                0.30 + r(seed, 50 + i) * 0.38
            },
            if tall {
                0.16 + r(seed, 70 + i) * 0.10
            } else {
                0.19 + r(seed, 70 + i) * 0.14
            },
            0.07 + r(seed, 90 + i) * 0.15,
            [0.87 + r(seed, 110 + i) * 0.13; 3],
            7.,
        );
    }
}
fn reed_bed(m: &mut MeshData, seed: u32) {
    tuft(m, seed, true);
    // The narrower warm seed sprays read as reed heads above the green blades.
    for i in 0..3 {
        let a = r(seed, 140 + i) * TAU;
        let base = [
            a.sin() * 0.23,
            0.89 + r(seed, 150 + i) * 0.26,
            a.cos() * 0.23,
        ];
        upright(m, base, a, 0.29, 0.045, 0.025, [0.40, 0.28, 0.14], 6.);
    }
}
fn fern_bed(m: &mut MeshData, seed: u32) {
    // Five bent fronds share the same texture across both sections; a common
    // root and dipped tip form a rosette instead of a flat cross billboard.
    for i in 0..5 {
        let a = i as f32 * TAU / 5. + r(seed, 160) * TAU;
        let length = 0.68 + r(seed, 170 + i) * 0.34;
        let base = [0.; 3];
        let c = [0.88 + r(seed, 180 + i) * 0.12; 3];
        let bottom = [
            point(base, a, 0., -0.045, 0.),
            point(base, a, 0., 0.045, 0.),
        ];
        let middle = [
            point(base, a, length * 0.48, -0.23, 0.44),
            point(base, a, length * 0.48, 0.23, 0.44),
        ];
        let tip = [
            point(base, a, length, -0.16, 0.27),
            point(base, a, length, 0.16, 0.27),
        ];
        card(
            m,
            [bottom[0], bottom[1], middle[1], middle[0]],
            c,
            8.,
            [1., 0.5],
        );
        card(m, [middle[0], middle[1], tip[1], tip[0]], c, 8., [0.5, 0.]);
    }
}
fn petal(m: &mut MeshData, center: [f32; 3], yaw: f32, tilt: f32, radius: f32, c: [f32; 3]) {
    let side = [yaw.cos() * radius, 0., -yaw.sin() * radius];
    let rise = [
        -yaw.sin() * tilt.cos() * radius,
        tilt.sin() * radius,
        -yaw.cos() * tilt.cos() * radius,
    ];
    let corner =
        |sx: f32, sy: f32| std::array::from_fn(|i| center[i] + side[i] * sx + rise[i] * sy);
    card(
        m,
        [
            corner(-1., -1.),
            corner(1., -1.),
            corner(1., 1.),
            corner(-1., 1.),
        ],
        c,
        9.,
        [1., 0.],
    );
}
fn flower_bed(m: &mut MeshData, variant: u32, seed: u32) {
    let blue = variant >= 6;
    let c = if blue {
        [0.46, 0.49, 0.95]
    } else if variant < 3 {
        [0.98, 0.95, 0.79]
    } else {
        [1.0, 0.77, 0.19]
    };
    for i in 0..3 {
        let a = r(seed, 190 + i) * TAU;
        let base = [a.sin() * 0.36, 0., a.cos() * 0.36];
        let h = if blue {
            0.73 + r(seed, 200 + i) * 0.27
        } else {
            0.33 + r(seed, 200 + i) * 0.26
        };
        upright(m, base, a, h, 0.14, 0.05, [0.93; 3], 7.);
        let top = point(base, a, 0.05, 0., h);
        let radius = 0.105 + r(seed, 210 + i) * 0.055;
        petal(m, top, a, 0.28, radius, c);
        petal(
            m,
            [top[0], top[1] + if blue { 0.075 } else { 0.008 }, top[2]],
            a + FRAC_PI_2,
            1.15,
            radius * if blue { 0.95 } else { 0.66 },
            c,
        );
    }
}
fn bush(m: &mut MeshData, seed: u32, heather: bool) {
    for i in 0..3 {
        let a = r(seed, 220 + i) * TAU;
        let base = [a.sin() * 0.28, 0., a.cos() * 0.28];
        let h = 0.31 + r(seed, 230 + i) * 0.30;
        let radius = 0.23 + r(seed, 240 + i) * 0.11;
        for side in 0..2 {
            upright(
                m,
                base,
                a + side as f32 * FRAC_PI_2,
                h,
                radius,
                0.055,
                [0.90 + side as f32 * 0.07; 3],
                5.,
            );
        }
        if heather {
            let top = point(base, a, 0.05, 0., h * 0.80);
            petal(m, top, a, 0.55, radius * 0.77, [0.73, 0.42, 0.67]);
        } else {
            upright(
                m,
                [base[0], h * 0.22, base[2]],
                a + 0.64,
                h * 0.84,
                radius * 0.87,
                0.12,
                [0.96; 3],
                5.,
            );
        }
    }
}
fn seed_bed(m: &mut MeshData, seed: u32) {
    tuft(m, seed, false);
    for i in 0..3 {
        let a = r(seed, 270 + i) * TAU;
        let base = [a.sin() * 0.28, 0., a.cos() * 0.28];
        upright(
            m,
            base,
            a,
            0.83 + r(seed, 280 + i) * 0.24,
            0.095,
            0.18,
            [0.82, 0.66, 0.33],
            6.,
        );
    }
}
fn litter(m: &mut MeshData, seed: u32) {
    for i in 0..9 {
        let a = r(seed, 290 + i) * TAU;
        let radius = r(seed, 310 + i) * 0.63;
        let base = [a.sin() * radius, 0., a.cos() * radius];
        let c = if i % 3 == 0 {
            [0.43, 0.45, 0.20]
        } else {
            [0.53, 0.34, 0.15]
        };
        card(
            m,
            [
                point(base, a, -0.16, -0.10, 0.021),
                point(base, a, -0.16, 0.10, 0.028),
                point(base, a, 0.19, 0.09, 0.055),
                point(base, a, 0.19, -0.09, 0.045),
            ],
            c,
            5.,
            [1., 0.],
        );
    }
}
pub fn template(kind: u32, variant: u32) -> MeshData {
    let seed = hash(0x434f5645, kind as i32, variant as i32);
    let mut mesh = MeshData::default();
    match kind {
        1 => reed_bed(&mut mesh, seed),
        2 => fern_bed(&mut mesh, seed),
        3 => flower_bed(&mut mesh, variant, seed),
        4 => bush(&mut mesh, seed, true),
        5 => seed_bed(&mut mesh, seed),
        6 => bush(&mut mesh, seed, false),
        7 => litter(&mut mesh, seed),
        _ => tuft(&mut mesh, seed, false),
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_cutout_communities_fit_gpu_templates_and_keep_root_and_palette_contracts() {
        for kind in 0..KINDS {
            for variant in 0..8 {
                let mesh = template(kind, variant);
                let repeat = template(kind, variant);
                assert!(mesh.vertices.len() <= (TRIANGLES * 3) as usize);
                assert_eq!(mesh.vertices.len() % 6, 0);
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&mesh.vertices),
                    bytemuck::cast_slice::<_, u8>(&repeat.vertices)
                );
                assert!(mesh.vertices.iter().all(|v| v.position[1] >= 0.
                    && (5.0..=9.0).contains(&v.texture)
                    && v.uv
                        .iter()
                        .all(|u| u.is_finite() && (0.0..=1.0).contains(u))));
                if kind != 7 {
                    assert!(
                        mesh.vertices.iter().any(|v| v.position[1] == 0.),
                        "plant community {kind} floats"
                    );
                }
                if kind == 3 {
                    assert!(
                        mesh.vertices
                            .iter()
                            .any(|v| v.texture == 9. && v.color[0] != v.color[1]),
                        "flowers must retain their own palette instead of grass tint"
                    );
                }
            }
        }
    }
}
