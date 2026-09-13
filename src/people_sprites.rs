//! Runtime paper-doll atlas: 32x64 pixels, four directions and four walk poses.
//! Pixels encode a material slot, not a character's final colour. One small
//! shared atlas supports every resident's skin, hair, clothing and equipment.
pub const WIDTH: u32 = 128;
pub const HEIGHT: u32 = 256;
pub const LAYERS: u32 = 16;
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Slot {
    Skin = 1,
    Hair = 2,
    Cloth = 3,
    Trousers = 4,
    Leather = 5,
    Metal = 6,
    Linen = 7,
    Accent = 8,
}
struct Frame {
    pixels: Vec<u8>,
}
impl Frame {
    fn new() -> Self {
        Self {
            pixels: vec![0; 32 * 64 * 4],
        }
    }
    fn dot(&mut self, x: i32, y: i32, slot: Slot, shade: f32) {
        if !(0..32).contains(&x) || !(0..64).contains(&y) {
            return;
        }
        let i = (y as usize * 32 + x as usize) * 4;
        self.pixels[i..i + 4].copy_from_slice(&[
            (shade.clamp(0., 1.) * 255.) as u8,
            slot as u8 * 24,
            if matches!(slot, Slot::Metal) { 220 } else { 0 },
            255,
        ]);
    }
    fn ellipse(&mut self, c: [f32; 2], rad: [f32; 2], slot: Slot) {
        for y in (c[1] - rad[1] - 1.) as i32..=(c[1] + rad[1] + 1.) as i32 {
            for x in (c[0] - rad[0] - 1.) as i32..=(c[0] + rad[0] + 1.) as i32 {
                let dx = (x as f32 - c[0]) / rad[0];
                let dy = (y as f32 - c[1]) / rad[1];
                if dx * dx + dy * dy <= 1. {
                    self.dot(x, y, slot, (0.78 - dx * 0.17 - dy * 0.06).clamp(0.38, 1.));
                }
            }
        }
    }
    fn line(&mut self, a: [f32; 2], b: [f32; 2], radius: f32, slot: Slot) {
        let steps = ((b[0] - a[0]).hypot(b[1] - a[1]) * 1.5).ceil().max(1.) as usize;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.ellipse(
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
                [radius, radius],
                slot,
            );
        }
    }
    fn rect(&mut self, a: [i32; 2], b: [i32; 2], slot: Slot) {
        for y in a[1]..=b[1] {
            for x in a[0]..=b[0] {
                self.dot(
                    x,
                    y,
                    slot,
                    0.88 - (x - a[0]) as f32 / (b[0] - a[0] + 1) as f32 * 0.18,
                );
            }
        }
    }
}
fn frame(role: u32, dir: u32, pose: u32) -> Frame {
    use Slot::*;
    let mut f = Frame::new();
    let side = dir == 1 || dir == 3;
    let back = dir == 2;
    let flip = if dir == 3 { -1. } else { 1. };
    let stride: f32 = [0., 2.8, 0., -2.8][pose as usize];
    let cy = if pose == 1 || pose == 3 { -0.5 } else { 0. };
    let robe = matches!(role, 4 | 5 | 10 | 14);
    let armor = matches!(role, 2 | 3 | 12);
    let ranger = matches!(role, 6 | 7);
    let shoulder = if side { 3.0 } else { 5.0 };
    let center = 15.5;
    // Equipment behind the torso follows the directional layer order.
    if ranger {
        f.line([8., 18.], [7., 48.], 1., Leather);
        for y in 19..48 {
            let x = 5. + ((y as f32 - 19.) / 29. * std::f32::consts::PI).sin() * 3.;
            f.dot(x as i32, y, Leather, 0.7);
        }
    }
    if matches!(role, 4 | 5 | 10) {
        f.line([26., 13.], [26., 60.], 1., Leather);
        f.ellipse([26., 12.], [2.4, 3.], Accent);
    }
    if matches!(role, 13 | 15) {
        f.rect([7, 21], [13, 38], Leather);
        f.line([9., 20.], [17., 26.], 1., Leather);
    }
    for (n, sign) in [-1., 1.].into_iter().enumerate() {
        let shift = stride * sign;
        let hip = center + sign * if side { 1.5 } else { 2.8 };
        let knee = [hip + shift * 0.4, 46. + cy];
        let foot = [hip + shift, 59. - shift.abs() * 0.15];
        f.line([hip, 35. + cy], knee, 2.2, Trousers);
        f.line(knee, foot, 1.9, Trousers);
        f.ellipse([foot[0] + flip * 0.7, 60.], [2.7, 1.7], Leather);
        let _ = n;
    }
    let far = center - shoulder;
    f.line([far, 23. + cy], [far - 1.2, 33. - stride], 1.9, Cloth);
    f.line(
        [far - 1.2, 33. - stride],
        [far + stride * 0.45, 40. - stride],
        1.5,
        Skin,
    );
    if robe {
        f.line([center, 30. + cy], [center, 49.], 5., Cloth);
        f.rect([10, 39], [21, 50], Cloth);
    }
    f.ellipse(
        [center, 29. + cy],
        [shoulder + 0.6, 10.],
        if armor { Metal } else { Cloth },
    );
    f.rect([11, 35], [20, 37], Leather);
    f.rect([15, 35], [16, 37], Metal);
    let near = center + shoulder;
    f.line(
        [near, 23. + cy],
        [near + 1., 32. + stride],
        2.0,
        if armor { Metal } else { Cloth },
    );
    f.line(
        [near + 1., 32. + stride],
        [near - stride * 0.45, 40. + stride],
        1.6,
        Skin,
    );
    f.line([center, 17. + cy], [center, 22. + cy], 2., Skin);
    f.ellipse(
        [center, 13. + cy],
        [if side { 3.4 } else { 4.3 }, 5.8],
        Skin,
    );
    f.ellipse([center - 0.2, 9. + cy], [4.4, 3.3], Hair);
    if back {
        f.ellipse([center, 13. + cy], [4.3, 5.6], Hair);
    } else {
        f.rect([11, 10], [12, 16], Hair);
        if !side {
            f.rect([19, 10], [20, 16], Hair);
        }
        let face = if side { center + flip * 2.3 } else { center };
        f.dot((face - 1.3) as i32, 13, Hair, 0.35);
        if !side {
            f.dot((face + 2.) as i32, 13, Hair, 0.35);
        }
        f.dot(face as i32, 17, Skin, 0.54);
    }
    if armor {
        f.ellipse([center, 8.5], [4.9, 4.4], Metal);
        f.rect([11, 9], [20, 11], Metal);
        if role == 2 {
            f.line([center, 3.], [center, 6.], 1.7, Accent);
        }
        f.line([22., 35.], [24., 56.], 1.2, Metal);
        f.rect([20, 37], [26, 38], Metal);
    }
    if role == 5 || role == 4 {
        for y in 1..11 {
            let width = (y as f32 / 2.).max(1.);
            f.rect(
                [(center - width) as i32, y],
                [(center + width) as i32, y],
                Cloth,
            );
        }
        f.rect([9, 10], [22, 11], Accent);
    }
    if ranger {
        f.ellipse([center, 8.], [5.1, 3.1], Cloth);
        f.rect([9, 10], [22, 11], Leather);
        f.line([20., 9.], [24., 4.], 1., Linen);
    }
    if role == 9 {
        f.rect([12, 24], [19, 39], Leather);
        f.line([22., 38.], [24., 47.], 1., Leather);
        f.rect([21, 43], [27, 46], Metal);
    }
    if role == 11 {
        f.rect([12, 27], [19, 40], Linen);
    }
    if role == 10 {
        f.line([15., 23.], [15., 35.], 1., Accent);
    }
    if back && matches!(role, 3 | 6 | 7 | 12) {
        f.ellipse(
            [center, 31.],
            [5.4, 7.4],
            if armor { Metal } else { Leather },
        );
    }
    f
}
pub fn generate() -> Vec<u8> {
    let mut out = vec![0; WIDTH as usize * HEIGHT as usize * LAYERS as usize * 4];
    for role in 0..LAYERS {
        for dir in 0..4 {
            for pose in 0..4 {
                let f = frame(role, dir, pose);
                for y in 0..64usize {
                    let dst = (((role as usize * HEIGHT as usize + pose as usize * 64 + y)
                        * WIDTH as usize)
                        + dir as usize * 32)
                        * 4;
                    out[dst..dst + 128].copy_from_slice(&f.pixels[y * 128..(y + 1) * 128]);
                }
            }
        }
    }
    out
}
pub fn texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Layered human paper dolls, 32x64 frames"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: LAYERS,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        t.as_image_copy(),
        &generate(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(WIDTH * 4),
            rows_per_image: Some(HEIGHT),
        },
        t.size(),
    );
    t
}

#[cfg(test)]
mod tests {
    #[test]
    fn paper_doll_frames_have_transparent_edges_and_material_slots() {
        let p = super::generate();
        assert_eq!(p.len(), 128 * 256 * 16 * 4);
        for role in 0..16 {
            for direction in 0..4 {
                for frame in 0..4 {
                    let mut opaque = 0;
                    for y in 0..64 {
                        for x in 0..32 {
                            let i = (((role * 256 + frame * 64 + y) * 128 + direction * 32 + x) * 4)
                                as usize;
                            if p[i + 3] > 0 {
                                opaque += 1;
                                assert!((1..=8).contains(&(p[i + 1] / 24)));
                            }
                            if x == 0 || x == 31 {
                                assert_eq!(p[i + 3], 0);
                            }
                        }
                    }
                    assert!(opaque > 200 && opaque < 1500);
                }
            }
        }
    }
}
