//! Original procedural costume plates: 48×96, four views, four strides and
//! four distinct silhouettes per profession. Shared palette slots keep clothing
//! recolourable without a texture allocation for each citizen.
pub const FRAME_W: usize = 48;
pub const FRAME_H: usize = 96;
pub const VARIANTS: u32 = 4;
pub const WIDTH: u32 = FRAME_W as u32 * 4;
pub const HEIGHT: u32 = FRAME_H as u32 * 4;
pub const LAYERS: u32 = 16 * VARIANTS;
#[derive(Clone, Copy, PartialEq)]
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
            pixels: vec![0; FRAME_W * FRAME_H * 4],
        }
    }
    fn dot(&mut self, x: i32, y: i32, s: Slot, v: f32) {
        if x < 1 || x >= FRAME_W as i32 - 1 || y < 1 || y >= FRAME_H as i32 - 1 {
            return;
        }
        let i = (y as usize * FRAME_W + x as usize) * 4;
        self.pixels[i..i + 4].copy_from_slice(&[
            (v.clamp(0., 1.) * 255.) as u8,
            s as u8 * 24,
            0,
            255,
        ]);
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, s: Slot, v: f32) {
        for yy in y.floor() as i32..(y + h).ceil() as i32 {
            for xx in x.floor() as i32..(x + w).ceil() as i32 {
                self.dot(xx, yy, s, v);
            }
        }
    }
    fn ellipse(&mut self, x: f32, y: f32, rx: f32, ry: f32, s: Slot, v: f32) {
        for yy in (y - ry).floor() as i32..=(y + ry).ceil() as i32 {
            for xx in (x - rx).floor() as i32..=(x + rx).ceil() as i32 {
                let dx = (xx as f32 - x) / rx;
                let dy = (yy as f32 - y) / ry;
                if dx * dx + dy * dy <= 1. {
                    self.dot(
                        xx,
                        yy,
                        s,
                        v - dx * 0.17 - dy * 0.07 - (dx * dx + dy * dy) * 0.10,
                    );
                }
            }
        }
    }
    fn line(&mut self, a: [f32; 2], b: [f32; 2], r: f32, s: Slot, v: f32) {
        let steps = ((a[0] - b[0]).hypot(a[1] - b[1]) * 1.5).ceil().max(1.) as usize;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.ellipse(
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                r,
                r,
                s,
                v,
            );
        }
    }
    fn poly(&mut self, p: &[[f32; 2]], s: Slot, v: f32) {
        let minx = p.iter().map(|q| q[0]).fold(100., f32::min).floor() as i32;
        let maxx = p.iter().map(|q| q[0]).fold(0., f32::max).ceil() as i32;
        let miny = p.iter().map(|q| q[1]).fold(100., f32::min).floor() as i32;
        let maxy = p.iter().map(|q| q[1]).fold(0., f32::max).ceil() as i32;
        for y in miny..=maxy {
            for x in minx..=maxx {
                let mut inside = false;
                let mut j = p.len() - 1;
                for i in 0..p.len() {
                    let a = p[i];
                    let b = p[j];
                    if (a[1] > y as f32) != (b[1] > y as f32)
                        && (x as f32) < (b[0] - a[0]) * (y as f32 - a[1]) / (b[1] - a[1]) + a[0]
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                if inside {
                    let fold = (x as f32 * 0.81 + y as f32 * 0.05).sin() * 0.055;
                    self.dot(x, y, s, v + fold - (x - minx) as f32 * 0.003);
                }
            }
        }
    }
}
fn frame(role: u32, variant: u32, dir: u32, pose: u32) -> Frame {
    use Slot::*;
    let mut f = Frame::new();
    // Draw one profile, then mirror the entire plate, including equipment.
    let side = dir == 1 || dir == 3;
    let back = dir == 2;
    let stride = [0., 3.2, 0., -3.2][pose as usize];
    let bob = if pose % 2 == 1 { -0.7 } else { 0. };
    let cx = 23.;
    let broad = variant == 2;
    let slender = variant == 0;
    let elder = variant == 3;
    let shoulder = if side {
        4.2
    } else if broad {
        9.0
    } else if slender {
        6.0
    } else {
        7.3
    };
    let robe = matches!(role, 4 | 5 | 10 | 14) || (slender && matches!(role, 0 | 8 | 11));
    let armor = matches!(role, 2 | 3 | 12);
    let ranger = matches!(role, 6 | 7);
    let apron = matches!(role, 1 | 9 | 11);
    let cloak = matches!(role, 2 | 3 | 4 | 5 | 6 | 7 | 8 | 10 | 12 | 13 | 14 | 15);
    if cloak {
        f.poly(
            &[
                [cx - shoulder, 29.],
                [cx + shoulder, 29.],
                [cx + shoulder + 4., 76.],
                [cx + 3., 80.],
                [cx - shoulder - 3., 76.],
            ],
            Trousers,
            0.67,
        );
    }
    if ranger {
        f.line([10., 26.], [12., 65.], 1.3, Leather, 0.68);
        for y in 24..68 {
            let x = 7. + ((y as f32 - 24.) / 44. * std::f32::consts::PI).sin() * 4.;
            f.dot(x as i32, y, Leather, 0.82);
        }
        f.line([7., 24.], [7., 68.], 0.35, Linen, 0.75);
        f.rect(29., 25., 5., 23., Leather, 0.55);
        for x in [29., 31., 33.] {
            f.line([x, 18.], [x - 1., 34.], 0.4, Accent, 0.8);
            f.line([x - 2., 18.], [x + 1., 21.], 0.6, Linen, 0.8);
        }
    }
    if role == 15 || role == 13 {
        f.rect(12., 29., 16., 24., Leather, 0.53);
        f.rect(14., 27., 12., 4., Linen, 0.73);
    }
    // Articulated legs with kneecaps, hose, boot cuffs and separate soles.
    for sign in [-1., 1.] {
        let x = cx + sign * if side { 2.0 } else { 3.7 };
        let dx = stride * sign;
        f.line(
            [x, 53. + bob],
            [x + dx * 0.4, 72.],
            if broad { 3.4 } else { 2.6 },
            Trousers,
            if sign < 0. { 0.60 } else { 0.79 },
        );
        f.line([x + dx * 0.4, 72.], [x + dx, 87.], 2.5, Leather, 0.68);
        f.rect(x + dx * 0.5 - 3., 73., 6., 3., Leather, 0.87);
        f.ellipse(x + dx + 1., 89., 3.8, 2.1, Leather, 0.70);
        f.rect(x + dx - 2.5, 90., 6.5, 1., Leather, 0.30);
        if armor {
            f.line([x + dx * 0.3, 67.], [x + dx * 0.6, 78.], 1.3, Metal, 0.68);
        }
    }
    let hem = if robe {
        86.
    } else if apron {
        64.
    } else {
        62.
    };
    f.poly(
        &[
            [cx - shoulder + 1., 43.],
            [cx + shoulder - 1., 43.],
            [cx + shoulder + if robe { 5. } else { 1. }, hem],
            [cx + 1., hem + 1.],
            [cx, hem - 4.],
            [cx - shoulder - if robe { 4. } else { 1. }, hem],
        ],
        Cloth,
        0.81,
    );
    f.poly(
        &[
            [cx - shoulder, 28. + bob],
            [cx + shoulder, 28. + bob],
            [cx + shoulder - 1., 44.],
            [cx + shoulder - 2., 53.],
            [cx - shoulder + 2., 53.],
            [cx - shoulder - 1., 39.],
        ],
        if armor { Metal } else { Cloth },
        0.83,
    );
    // Sleeves taper at elbows; hands are visibly separate from cuffs.
    for sign in [-1., 1.] {
        let x = cx + sign * shoulder;
        let swing = if side {
            stride * sign
        } else {
            stride * sign * 0.45
        };
        let elbow = [x + sign * 2., 43. + swing];
        let hand = [x + sign * 0.5, 57. + swing];
        f.line(
            [x, 31. + bob],
            elbow,
            if robe { 3.7 } else { 3.0 },
            if armor { Metal } else { Cloth },
            if sign < 0. { 0.68 } else { 0.83 },
        );
        f.line(
            elbow,
            hand,
            if robe { 3.0 } else { 2.1 },
            if role == 9 { Skin } else { Cloth },
            0.76,
        );
        f.rect(
            hand[0] - 2.6,
            hand[1] - 3.,
            5.,
            2.4,
            if armor { Leather } else { Linen },
            0.8,
        );
        f.ellipse(hand[0], hand[1], 1.9, 3., Skin, 0.85);
    }
    if !back {
        // Open collar, leather lacing, waist sash and a tiny brass buckle.
        f.poly(
            &[
                [cx - 4., 27.],
                [cx, 35.],
                [cx + 4., 27.],
                [cx + 2., 38.],
                [cx - 2., 38.],
            ],
            Linen,
            0.84,
        );
        for y in (32..41).step_by(3) {
            f.line(
                [cx - 1., y as f32],
                [cx + 1., y as f32 + 1.],
                0.4,
                Leather,
                0.40,
            );
        }
        if apron {
            f.poly(
                &[
                    [cx - 4., 35.],
                    [cx + 4., 35.],
                    [cx + 6., 66.],
                    [cx - 6., 66.],
                ],
                if role == 9 { Leather } else { Linen },
                0.78,
            );
            f.line(
                [cx - 4., 35.],
                [cx - 3., 28.],
                0.7,
                if role == 9 { Leather } else { Linen },
                0.76,
            );
        }
        f.rect(cx - shoulder, 49., shoulder * 2., 3., Leather, 0.38);
        f.rect(cx - 1.5, 49., 3., 3., Accent, 0.95);
        f.dot(cx as i32, 50, Leather, 0.35);
        f.ellipse(cx + shoulder - 1., 55., 2.6, 3.4, Leather, 0.65);
        f.rect(cx + shoulder - 3., 52., 4., 2., Leather, 0.82);
        if robe {
            for x in [cx - shoulder + 1., cx + shoulder - 1.] {
                f.line([x, 54.], [x + (x - cx) * 0.30, 83.], 0.6, Accent, 0.75);
            }
            f.rect(
                cx - shoulder - 3.,
                83.,
                shoulder * 2. + 6.,
                2.,
                Accent,
                0.72,
            );
        }
        if armor {
            for sign in [-1., 1.] {
                f.ellipse(cx + sign * shoulder, 31., 4., 3., Metal, 0.93);
                f.line(
                    [cx + sign * 3., 37.],
                    [cx + sign * 5., 47.],
                    0.6,
                    Metal,
                    0.98,
                );
            }
            f.poly(
                &[
                    [cx - 4., 33.],
                    [cx + 4., 33.],
                    [cx + 3., 57.],
                    [cx, 61.],
                    [cx - 3., 57.],
                ],
                Cloth,
                0.75,
            );
            f.line([cx, 36.], [cx, 46.], 0.7, Accent, 0.88);
            f.line([cx - 2., 40.], [cx + 2., 40.], 0.7, Accent, 0.88);
        }
    }
    // Neck, cheek planes, nose bridge, eyelids, mouth and directional ears.
    f.rect(cx - 2., 23. + bob, 4., 6., Skin, 0.69);
    let facew = if side {
        3.7
    } else if broad {
        5.6
    } else {
        4.6
    };
    if slender {
        f.ellipse(cx, 20. + bob, 6., 10., Hair, 0.65);
    }
    f.ellipse(cx, 18. + bob, facew, 7.3, Skin, 0.90);
    f.ellipse(cx, 12. + bob, facew + 0.4, 3.4, Hair, 0.76);
    if back {
        f.ellipse(cx, 18. + bob, facew + 0.5, 7., Hair, 0.72);
        if slender {
            f.line([cx + 3., 20.], [cx + 4., 34.], 1.3, Hair, 0.82);
        }
    } else {
        f.line(
            [cx - facew + 0.3, 13.],
            [cx - facew + 0.4, 20.],
            0.9,
            Hair,
            0.61,
        );
        if !side {
            f.line(
                [cx + facew - 0.5, 13.],
                [cx + facew - 0.5, 19.],
                0.8,
                Hair,
                0.46,
            );
        }
        let nose = if side { cx + 3.4 } else { cx + 0.5 };
        for ex in if side {
            vec![cx + 1.5]
        } else {
            vec![cx - 2.4, cx + 2.4]
        } {
            f.rect(ex - 1., 16., 2., 1., Hair, 0.43);
            f.dot(ex as i32, 18, Hair, 0.30);
            f.dot(ex as i32 - 1, 18, Skin, 0.98);
        }
        f.line([nose, 18.], [nose, 21.], 0.7, Skin, 0.99);
        f.rect(cx - 1., 23., 3., 1., Skin, 0.42);
        if (variant == 1 || broad) && !slender {
            f.poly(
                &[
                    [cx - 4., 21.],
                    [cx - 2., 24.],
                    [cx + 3., 22.],
                    [cx + 3., 26.],
                    [cx, 28.],
                    [cx - 3., 25.],
                ],
                Hair,
                0.67,
            );
        }
        if elder {
            f.line([cx - 3., 19.], [cx - 1., 20.], 0.4, Skin, 0.51);
            f.line([cx + 1., 20.], [cx + 3., 19.], 0.4, Skin, 0.51);
        }
    }
    if armor {
        f.ellipse(cx, 11., facew + 1.4, 4.4, Metal, 0.85);
        f.rect(cx - facew - 1., 12., facew * 2. + 2., 2., Metal, 0.66);
        if !back {
            f.line([cx - facew, 13.], [cx - facew + 0.5, 22.], 1.2, Metal, 0.77);
            f.line([cx + facew, 13.], [cx + facew - 0.5, 22.], 1.2, Metal, 0.68);
        }
        if role == 2 {
            f.poly(
                &[
                    [cx - 1., 8.],
                    [cx - 2., 2.],
                    [cx + 4., 3.],
                    [cx + 6., 7.],
                    [cx + 1., 6.],
                ],
                Cloth,
                0.86,
            );
        }
        f.line(
            [cx + shoulder + 1., 51.],
            [cx + shoulder + 5., 79.],
            1.3,
            Metal,
            0.72,
        );
        f.line(
            [cx + shoulder - 2., 54.],
            [cx + shoulder + 5., 53.],
            0.8,
            Accent,
            0.88,
        );
    } else if role == 4 || role == 5 {
        f.poly(
            &[
                [cx - 7., 12.],
                [cx - 3., 6.],
                [cx + 1., 2.],
                [cx + 4., 10.],
                [cx + 8., 13.],
            ],
            Cloth,
            0.82,
        );
        f.line([cx - 8., 13.], [cx + 8., 13.], 1., Accent, 0.86);
    } else if ranger || role == 13 || role == 15 {
        f.poly(
            &[[cx - 6., 11.], [cx - 3., 6.], [cx + 4., 7.], [cx + 5., 12.]],
            Cloth,
            0.74,
        );
        f.line([cx - 9., 13.], [cx + 8., 12.], 1., Leather, 0.68);
        f.line([cx + 5., 10.], [cx + 9., 4.], 0.8, Linen, 0.89);
    } else if slender && matches!(role, 1 | 10 | 11) {
        f.poly(
            &[
                [cx - 6., 14.],
                [cx - 5., 9.],
                [cx + 4., 9.],
                [cx + 6., 20.],
                [cx + 4., 24.],
                [cx + 4., 13.],
            ],
            Linen,
            0.90,
        );
    } else if !elder && matches!(role, 0 | 1 | 8 | 11) {
        f.ellipse(cx - 1., 10., 6., 2.4, Cloth, 0.8);
        f.rect(cx - 5., 11., 10., 1.5, Accent, 0.67);
    }
    // Profession-defining held objects, confined to the correct side of the body.
    if matches!(role, 4 | 5 | 10) {
        f.line([39., 26.], [39., 90.], 1., Leather, 0.66);
        f.ellipse(39., 24., 2.6, 3.8, Accent, 0.94);
        f.ellipse(38., 23., 1., 1.8, Linen, 0.96);
    }
    if role == 9 {
        f.line([36., 53.], [39., 66.], 1., Leather, 0.75);
        f.rect(35., 63., 9., 4., Metal, 0.67);
    }
    if role == 14 || (role == 8 && broad) {
        f.rect(30., 45., 9., 13., Leather, 0.66);
        f.rect(31., 46., 6., 11., Linen, 0.87);
        f.rect(30., 45., 2., 13., Cloth, 0.6);
        f.ellipse(32., 55., 2., 2., Skin, 0.8);
    }
    if role == 11 {
        f.line([31., 55.], [43., 55.], 0.8, Leather, 0.70);
        f.rect(35., 48., 4., 6., Accent, 0.82);
        f.rect(40., 49., 3., 5., Linen, 0.83);
    }
    if back && cloak {
        f.poly(
            &[
                [cx - shoulder + 1., 29.],
                [cx + shoulder - 1., 29.],
                [cx + shoulder + 2., 75.],
                [cx, 79.],
                [cx - shoulder - 2., 75.],
            ],
            Cloth,
            0.69,
        );
        f.line([cx - 2., 32.], [cx - 3., 73.], 0.5, Cloth, 0.85);
    }
    // Occupational silhouettes remain distinct even before palette variation.
    if role == 4 && !back {
        f.poly(
            &[
                [cx - 6., 27.],
                [cx - 8., 21.],
                [cx - 4., 26.],
                [cx, 30.],
                [cx + 5., 24.],
                [cx + 8., 22.],
                [cx + 6., 30.],
            ],
            Trousers,
            0.61,
        );
        f.ellipse(cx, 38., 2.2, 3., Accent, 0.96);
    }
    if role == 7 {
        for n in 0..9 {
            f.ellipse(
                cx - shoulder + n as f32 * shoulder * 0.25,
                30. + (n % 2) as f32,
                2.,
                3.,
                Linen,
                0.58,
            );
        }
        if !back {
            f.line([cx - 6., 33.], [cx + 5., 50.], 1., Leather, 0.5);
        }
    }
    if role == 12 {
        f.ellipse(cx - shoulder - 1., 46., 5.4, 9., Leather, 0.65);
        f.ellipse(cx - shoulder - 1., 46., 1.9, 2.4, Metal, 0.92);
        f.line(
            [cx - shoulder - 5., 40.],
            [cx - shoulder + 2., 40.],
            0.6,
            Metal,
            0.66,
        );
    }
    if role == 15 && !back {
        f.line(
            [cx - shoulder + 1., 30.],
            [cx + shoulder - 1., 53.],
            1.0,
            Leather,
            0.54,
        );
        f.rect(cx + shoulder - 3., 51., 6., 8., Leather, 0.63);
        f.rect(cx + shoulder - 2., 52., 4., 2., Linen, 0.93);
        f.line(
            [cx - shoulder - 2., 43.],
            [cx - shoulder - 1., 56.],
            1.1,
            Linen,
            0.88,
        );
        f.rect(cx - shoulder - 3., 48., 4., 2., Accent, 0.80);
    }
    if role == 8 && !back {
        f.line([cx - 5., 28.], [cx, 37.], 0.7, Accent, 0.93);
        f.line([cx + 5., 28.], [cx, 37.], 0.7, Accent, 0.93);
        f.ellipse(cx, 38., 1.7, 2.2, Accent, 0.95);
    }
    // Edge shadows and sparse material marks, not uniform noisy stippling.
    let source = f.pixels.clone();
    for y in 1..FRAME_H - 1 {
        for x in 1..FRAME_W - 1 {
            let i = (y * FRAME_W + x) * 4;
            if source[i + 3] == 0 {
                continue;
            }
            let edge = [i - 4, i + 4, i - FRAME_W * 4, i + FRAME_W * 4]
                .iter()
                .any(|&j| source[j + 3] == 0);
            let shade = source[i] as f32;
            if edge {
                f.pixels[i] = (shade * 0.65) as u8;
            } else if source[i + 1] == Metal as u8 * 24 && y > 28 && (x + y * 2) % 7 == 0 {
                f.pixels[i] = (shade * 0.72) as u8;
            } else if source[i + 1] == Cloth as u8 * 24 && (x * 3 + y) % 19 == 0 {
                f.pixels[i] = (shade * 0.9) as u8;
            }
        }
    }
    if dir == 3 {
        for y in 0..FRAME_H {
            for x in 0..FRAME_W / 2 {
                for c in 0..4 {
                    f.pixels.swap(
                        (y * FRAME_W + x) * 4 + c,
                        (y * FRAME_W + FRAME_W - 1 - x) * 4 + c,
                    );
                }
            }
        }
    }
    f
}
pub fn generate() -> Vec<u8> {
    // Only shade + material ID are needed; slot zero is transparent. RG8 halves
    // GPU memory versus RGBA while preserving every costume pixel exactly.
    let mut out = vec![0; WIDTH as usize * HEIGHT as usize * LAYERS as usize * 2];
    for variant in 0..VARIANTS {
        for role in 0..16 {
            for dir in 0..4 {
                for pose in 0..4 {
                    let f = frame(role, variant, dir, pose);
                    let layer = variant * 16 + role;
                    for y in 0..FRAME_H {
                        for x in 0..FRAME_W {
                            let dst = (((layer as usize * HEIGHT as usize
                                + pose as usize * FRAME_H
                                + y)
                                * WIDTH as usize)
                                + dir as usize * FRAME_W
                                + x)
                                * 2;
                            let src = (y * FRAME_W + x) * 4;
                            out[dst..dst + 2].copy_from_slice(&f.pixels[src..src + 2]);
                        }
                    }
                }
            }
        }
    }
    out
}
/// Colour reference plates use the same slots and shading as the GPU shader.
pub fn preview(role: u32, variant: u32, dir: u32, pose: u32) -> Vec<u8> {
    let mut f = frame(role, variant, dir, pose).pixels;
    let cloth = [
        [0.36, 0.48, 0.38],
        [0.48, 0.25, 0.20],
        [0.28, 0.38, 0.54],
        [0.48, 0.37, 0.24],
    ][role as usize % 4];
    for p in f.chunks_mut(4) {
        if p[3] == 0 {
            continue;
        }
        let shade = p[0] as f32 / 255. * 0.98 + 0.12;
        let c = match p[1] / 24 {
            1 => [0.82, 0.62, 0.43],
            2 => {
                if variant == 3 {
                    [0.66, 0.63, 0.54]
                } else {
                    [0.28, 0.16, 0.08]
                }
            }
            3 => cloth,
            4 => cloth.map(|v| v * 0.56),
            5 => [0.30, 0.18, 0.095],
            6 => [0.53, 0.57, 0.58],
            7 => [0.79, 0.74, 0.59],
            _ => [0.69, 0.48, 0.17],
        };
        for i in 0..3 {
            p[i] = (c[i] * shade * 255.).min(255.) as u8;
        }
    }
    f
}
pub fn texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Procedural costume plates, 48x96, four silhouettes"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: LAYERS,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        t.as_image_copy(),
        &generate(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(WIDTH * 2),
            rows_per_image: Some(HEIGHT),
        },
        t.size(),
    );
    t
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn costumes_have_clear_edges_valid_slots_and_distinct_variants() {
        let mut unique = std::collections::HashSet::new();
        for role in 0..16 {
            for variant in 0..VARIANTS {
                for dir in 0..4 {
                    for pose in 0..4 {
                        let p = frame(role, variant, dir, pose).pixels;
                        let mut count = 0;
                        for y in 0..FRAME_H {
                            for x in 0..FRAME_W {
                                let i = (y * FRAME_W + x) * 4;
                                if p[i + 3] > 0 {
                                    count += 1;
                                    assert!((1..=8).contains(&(p[i + 1] / 24)));
                                }
                                if x == 0 || x == FRAME_W - 1 || y == 0 || y == FRAME_H - 1 {
                                    assert_eq!(p[i + 3], 0);
                                }
                            }
                        }
                        assert!(count > 500 && count < 3000);
                        if dir == 0 && pose == 0 {
                            assert!(unique.insert(p), "duplicate costume {role}/{variant}");
                        }
                    }
                }
            }
        }
    }
}
