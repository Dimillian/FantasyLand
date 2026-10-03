//! Original procedural costume plates: 48×96, four views, four strides and
//! four distinct silhouettes per profession. Shared palette slots keep clothing
//! recolourable without a texture allocation for each citizen.
pub const FRAME_W: usize = 48;
pub const FRAME_H: usize = 96;
pub const VARIANTS: u32 = 4;
pub const WIDTH: u32 = FRAME_W as u32 * 4;
pub const HEIGHT: u32 = FRAME_H as u32 * 4;
pub const COMBAT_W: usize = 96;
pub const COMBAT_H: usize = 192;
pub const COMBAT_FRAMES: u32 = 24;
pub const LAYERS: u32 = 16 * VARIANTS + COMBAT_FRAMES * 3;
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
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}
impl Frame {
    fn new() -> Self {
        Self::sized(FRAME_W, FRAME_H)
    }
    fn sized(width: usize, height: usize) -> Self {
        Self {width, height, pixels:vec![0; width*height*4]}
    }
    fn dot(&mut self, x: i32, y: i32, s: Slot, v: f32) {
        if x < 1 || x >= self.width as i32 - 1 || y < 1 || y >= self.height as i32 - 1 {
            return;
        }
        let i = (y as usize * self.width + x as usize) * 4;
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
// Separate skeleton plates keep citizen walk cycles intact. Twenty-four frames
// cover locomotion and attack/reaction phases in four directional views.
fn combat_frame(dir: u32, pose: u32) -> Frame {
    use Slot::*;
    let mut f = Frame::sized(COMBAT_W, COMBAT_H);
    let side = dir == 1 || dir == 3;
    let back = dir == 2;
    let walk = pose < 8;
    let cycle = pose as f32 * std::f32::consts::TAU / 8.;
    let stride = if walk { cycle.sin() } else { 0. };
    let wind = if (8..12).contains(&pose) { (pose - 8) as f32 / 3. } else { 0. };
    let strike = if (12..15).contains(&pose) { (pose - 12) as f32 / 2. } else { 0. };
    let recover = if (15..18).contains(&pose) { (pose - 15) as f32 / 2. } else { 0. };
    let hurt = if pose == 18 { -5. } else if pose == 19 { -9. } else { 0. };
    let fall = if pose >= 20 { (pose - 20) as f32 / 3. } else { 0. };
    let bob = if walk { cycle.cos().abs() * 1.8 } else { wind * -2. };
    let p = |x: f32, y: f32| {
        let scale = if side { 0.62 } else { 1. };
        let x = 48. + (x - 48.) * scale + hurt * (1. - y / 190.) + fall * (182. - y) * 0.16;
        let y = y + bob * (1. - y / 190.) + (182. - y) * fall * 0.90;
        [if dir == 3 { COMBAT_W as f32 - x } else { x }, y]
    };
    fn bone(f: &mut Frame, a: [f32; 2], b: [f32; 2], r: f32) {
        f.line(a, b, r, Slot::Linen, 0.56);
        f.line([a[0]-0.65,a[1]], [b[0]-0.65,b[1]], (r*0.40).max(0.55), Slot::Linen, 0.96);
        for q in [a,b] { f.ellipse(q[0],q[1],r*1.18,r*0.85,Slot::Linen,0.77); }
    }
    // Independent hips, knees and ankles: a planted foot / lifted toe gait.
    for (sign, phase) in [(-1.,stride),(1.,-stride)] {
        let hip = p(48.+sign*7.,112.);
        let knee = p(48.+sign*8.+phase*5.,143.-phase.max(0.)*5.);
        let ankle = p(48.+sign*9.+phase*8.,177.-phase.max(0.)*8.);
        bone(&mut f,hip,knee,2.8);
        bone(&mut f,[knee[0]-1.5,knee[1]],[ankle[0]-1.5,ankle[1]],1.8);
        bone(&mut f,[knee[0]+2.,knee[1]+1.],[ankle[0]+2.,ankle[1]-1.],1.0);
        for toe in 0..4 { bone(&mut f,[ankle[0]+toe as f32-2.,ankle[1]], [ankle[0]+toe as f32*1.8-4.,ankle[1]+5.],0.8); }
    }
    // Pelvis has an open centre; individual vertebrae carry the rib cage.
    for sign in [-1.,1.] {
        let a=p(48.+sign*3.,101.);let b=p(48.+sign*11.,106.);let c=p(48.+sign*7.,117.);
        bone(&mut f,a,b,3.);bone(&mut f,b,c,2.4);bone(&mut f,c,p(48.,113.),1.8);
    }
    for y in (47..108).step_by(5) { let q=p(48.,y as f32);f.ellipse(q[0],q[1],2.8,1.8,Linen,0.78); }
    for rib in 0..6 {
        let y=60.+rib as f32*6.;let width=14.-rib as f32*1.1;
        for sign in [-1.,1.] {
            let mut last=p(48.,y-2.);
            for step in 1..=8 {
                let t=step as f32/8.;let angle=t*std::f32::consts::PI;
                let next=p(48.+sign*width*angle.sin(),y+t*5.);
                bone(&mut f,last,next,0.95);last=next;
            }
        }
    }
    if !back { bone(&mut f,p(48.,58.),p(48.,90.),1.4); }
    for sign in [-1.,1.] { bone(&mut f,p(48.,54.),p(48.+sign*16.,58.),2.0); }
    // Frayed remnants at the waist; the empty rib spaces stay transparent.
    f.poly(&[p(36.,104.),p(59.,103.),p(58.,125.),p(54.,118.),p(50.,129.),p(47.,120.),p(43.,126.),p(38.,118.)],Cloth,0.44);
    bone(&mut f,p(39.,107.),p(55.,108.),0.8);
    let (hand, tip) = if (8..12).contains(&pose) {
        (p(76.-15.*wind,97.-63.*wind),p(81.-31.*wind,61.-58.*wind))
    } else if (12..15).contains(&pose) {
        (p(61.-35.*strike,34.+83.*strike),p(50.-44.*strike,3.+145.*strike))
    } else if (15..18).contains(&pose) {
        (p(26.+50.*recover,117.-20.*recover),p(6.+75.*recover,148.-87.*recover))
    } else { (p(76.+stride*2.,97.),p(81.+stride*2.,61.)) };
    let shoulder=p(64.,58.);
    let elbow=[(shoulder[0]+hand[0])*0.5+5.,(shoulder[1]+hand[1])*0.5+9.];
    bone(&mut f,shoulder,elbow,2.6);
    bone(&mut f,[elbow[0]-1.,elbow[1]],[hand[0]-1.,hand[1]],1.7);
    bone(&mut f,[elbow[0]+2.,elbow[1]],[hand[0]+2.,hand[1]],1.0);
    let left_elbow=p(27.-stride*3.,83.);let left_hand=p(31.-stride*2.,108.);
    bone(&mut f,p(32.,58.),left_elbow,2.5);bone(&mut f,left_elbow,left_hand,1.8);
    for finger in 0..4 { let x=left_hand[0]+finger as f32*1.8-3.;bone(&mut f,[x,left_hand[1]],[x-1.,left_hand[1]+7.],0.8); }
    // Rusted arming sword follows the hand through anticipation and follow-through.
    let d=[tip[0]-hand[0],tip[1]-hand[1]];let length=d[0].hypot(d[1]).max(1.);let n=[-d[1]/length,d[0]/length];
    f.line(hand,tip,2.1,Metal,0.52);f.line([hand[0]+n[0],hand[1]+n[1]],tip,0.65,Metal,0.98);
    f.line([hand[0]-n[0]*6.,hand[1]-n[1]*6.],[hand[0]+n[0]*6.,hand[1]+n[1]*6.],1.2,Leather,0.78);
    for finger in 0..3 {f.ellipse(hand[0],hand[1]+finger as f32*2.,3.0,0.9,Linen,0.9);}
    // Skull: brow, cheek bones, recessed sockets, nasal cavity and separate jaw.
    let skull=p(48.,29.);let sx=if side {7.} else {10.};
    f.ellipse(skull[0],skull[1],sx,12.,Linen,0.83);
    f.ellipse(skull[0],skull[1]+10.,sx*0.67,5.,Linen,0.65);
    if !back {
        for x in if side {vec![skull[0]+2.]} else {vec![skull[0]-4.5,skull[0]+4.5]} {
            f.ellipse(x,skull[1]+1.,3.0,3.6,Hair,0.12);
            f.dot(x as i32,skull[1] as i32+1,Accent,0.95);
            f.line([x-2.8,skull[1]-3.],[x+2.8,skull[1]-2.],0.9,Linen,0.94);
        }
        f.poly(&[[skull[0],skull[1]+4.],[skull[0]-1.8,skull[1]+8.],[skull[0]+1.8,skull[1]+8.]],Hair,0.1);
        f.rect(skull[0]-4.,skull[1]+10.,8.,2.,Hair,0.12);
        for tooth in 0..5 {f.rect(skull[0]-4.+tooth as f32*1.7,skull[1]+9.,1.,2.,Linen,0.98);}
    }
    f.line([skull[0]-2.,skull[1]-10.],[skull[0]+1.,skull[1]-6.],0.55,Hair,0.25);
    f.line([skull[0]+1.,skull[1]-6.],[skull[0],skull[1]-3.],0.55,Hair,0.25);
    f
}

fn creature_frame(kind:u32,dir:u32,pose:u32)->Frame {
    if kind==0{return combat_frame(dir,pose);}
    use Slot::*;
    let mut f=Frame::sized(COMBAT_W,COMBAT_H);
    let side=dir==1||dir==3;let back=dir==2;
    let phase=pose as f32*std::f32::consts::TAU/8.;
    let stride=if pose<8{phase.sin()}else{0.};
    let lift=if (8..12).contains(&pose){(pose-8)as f32/3.}else if (12..15).contains(&pose){1.-(pose-12)as f32/2.}else{0.};
    let slash=if (12..15).contains(&pose){(pose-12)as f32/2.}else if (15..18).contains(&pose){1.-(pose-15)as f32/2.}else{0.};
    let fall=if pose>=20{(pose-20)as f32/3.}else{0.};let hurt=if pose==18{-5.}else if pose==19{-9.}else{0.};
    let p=|x:f32,y:f32|{let x=48.+(x-48.)*if side{0.62}else{1.}+hurt*(1.-y/190.)+fall*(180.-y)*0.1;[if dir==3{96.-x}else{x},y+(180.-y)*fall*0.9+if pose<8{phase.cos()*1.5}else{0.}]};
    if kind==1 {
        // Bent legs, broad feet, layered leather tunic and a heavy hooked nose.
        for (sign,v) in [(-1.,stride),(1.,-stride)] {
            f.line(p(48.+sign*12.,119.),p(48.+sign*19.+v*4.,151.),6.,Skin,0.58);
            f.line(p(48.+sign*19.+v*4.,151.),p(48.+sign*17.+v*7.,176.-v.max(0.)*5.),4.,Skin,0.81);
            let foot=p(48.+sign*20.+v*7.,179.-v.max(0.)*5.);f.ellipse(foot[0],foot[1],10.,5.,Leather,0.58);
        }
        f.poly(&[p(27.,62.),p(67.,62.),p(72.,114.),p(65.,134.),p(51.,130.),p(39.,135.),p(25.,120.)],Cloth,0.59);
        f.poly(&[p(30.,65.),p(44.,67.),p(47.,122.),p(30.,126.)],Leather,0.7);
        f.line(p(48.,65.),p(55.,119.),1.5,Linen,0.47);
        for y in (74..111).step_by(9){let q=p(50.,y as f32);f.rect(q[0],q[1],2.,2.,Metal,0.8);}
        f.line(p(26.,114.),p(70.,115.),3.,Leather,0.3);let buckle=p(49.,113.);f.rect(buckle[0],buckle[1],7.,5.,Metal,0.75);
        let hand=p(77.-slash*47.-lift*12.,106.-lift*65.+slash*14.);
        let elbow=p(78.-lift*3.,80.-lift*20.);
        f.line(p(66.,67.),elbow,6.,Skin,0.52);f.line(elbow,hand,4.8,Skin,0.79);
        let tip=p(82.-slash*70.-lift*24.,63.-lift*59.+slash*85.);
        f.line(hand,tip,3.,Metal,0.51);f.line([hand[0]-1.,hand[1]],tip,1.,Metal,0.95);
        f.line([hand[0]-7.,hand[1]],[hand[0]+7.,hand[1]],1.5,Leather,0.74);
        f.ellipse(hand[0],hand[1]+4.,4.8,6.,Skin,0.7);
        let lh=p(21.+stride*3.,109.);f.line(p(28.,68.),p(18.,88.),6.,Skin,0.6);f.line(p(18.,88.),lh,4.5,Skin,0.84);f.ellipse(lh[0],lh[1],5.,7.,Skin,0.65);
        for sign in [-1.,1.] {f.poly(&[p(48.+sign*11.,29.),p(48.+sign*33.,20.),p(48.+sign*25.,45.),p(48.+sign*12.,48.)],Skin,0.65);f.line(p(48.+sign*28.,25.),p(48.+sign*17.,40.),1.3,Cloth,0.7);}
        let head=p(48.,42.);f.ellipse(head[0],head[1],if side{12.}else{18.},23.,Skin,0.77);
        f.poly(&[p(31.,23.),p(36.,12.),p(44.,23.),p(50.,10.),p(57.,24.),p(65.,24.)],Hair,0.38);
        if !back {
            for sign in [-1.,1.]{let e=p(48.+sign*8.,36.);f.ellipse(e[0],e[1],4.,2.5,Hair,0.15);f.dot(e[0]as i32,e[1]as i32,Accent,0.95);f.line(p(48.+sign*4.,32.),p(48.+sign*12.,30.),2.,Skin,0.36);}
            f.poly(&[p(46.,35.),p(40.,51.),p(49.,54.),p(53.,49.)],Skin,0.99);
            f.line(p(37.,57.),p(59.,57.),2.,Hair,0.2);
            for x in [39.,56.]{f.poly(&[p(x,60.),p(x+2.,51.),p(x+4.,60.)],Linen,0.96);}
        }
    } else {
        // Torn spectral shroud, nested hood, reaching claw bones and wispy hem.
        let drift=if pose<8{phase.sin()*2.}else{0.};
        f.poly(&[p(32.,42.),p(23.,74.),p(29.,117.),p(15.+drift,166.),p(30.,155.),p(27.,181.),p(41.,170.),p(50.+drift,186.),p(58.,162.),p(72.,179.),p(68.,155.),p(81.,166.),p(66.,106.),p(70.,73.),p(64.,41.)],Cloth,0.52);
        for (x,shade) in [(31.,0.72),(43.,0.84),(56.,0.63),(63.,0.76)] {f.poly(&[p(x,72.),p(x+4.,77.),p(x+7.+drift,162.),p(x-3.+drift,171.)],Cloth,shade);}
        for sign in [-1.,1.] {
            let hand=p(48.+sign*(34.-slash*19.),105.-lift*51.+slash*15.);
            let elbow=p(48.+sign*28.,75.-lift*18.);
            f.line(p(48.+sign*18.,66.),elbow,8.,Cloth,0.55);f.line(elbow,hand,5.,Cloth,0.8);
            f.ellipse(hand[0],hand[1],4.,5.,Linen,0.86);
            for finger in 0..4{let x=hand[0]+finger as f32*2.-3.;f.line([x,hand[1]],[x+sign*3.,hand[1]+9.+(finger%2)as f32*3.],0.8,Linen,0.96);}
        }
        let head=p(48.,37.);f.ellipse(head[0],head[1],20.,29.,Cloth,0.78);
        if !back {f.ellipse(head[0],head[1]+3.,13.,20.,Hair,0.14);f.ellipse(head[0],head[1]+5.,9.,12.,Linen,0.51);
            for sign in [-1.,1.]{let e=p(48.+sign*5.,38.);f.ellipse(e[0],e[1],2.,1.8,Accent,0.99);}
            let mouth=p(48.,50.);f.ellipse(mouth[0],mouth[1],2.2,4.5,Hair,0.08);
        }
        // Sparse edges at the hem suggest dissolution without sorted alpha blending.
        for y in 135..COMBAT_H {for x in 0..COMBAT_W {let i=(y*COMBAT_W+x)*4;if (x*17+y*23+pose as usize*3)%37<((y-135)/5){f.pixels[i..i+4].fill(0);}}}
    }
    f
}

/// Original skeleton animation plate, exposed for the local art study.
pub fn combat_preview(dir:u32,pose:u32)->Vec<u8>{creature_preview(0,dir,pose)}
pub fn creature_preview(kind:u32,dir:u32,pose:u32)->Vec<u8>{
    let mut pixels=creature_frame(kind,dir%4,pose%COMBAT_FRAMES).pixels;
    for p in pixels.chunks_mut(4) {
        if p[3]==0 {continue;}
        let shade=p[0] as f32/255.*0.98+0.12;
        let mut pigment=match p[1]/24 {
            2=>[0.045,0.038,0.030], 3=>[0.30,0.17,0.14],
            5=>[0.30,0.18,0.095],6=>[0.53,0.57,0.58],
            7=>[0.84,0.81,0.68],8=>[0.40,0.85,0.88],_=>[0.7,0.65,0.5]
        };
        if kind==1 {pigment=match p[1]/24 {1=>[0.30,0.44,0.16],3=>[0.30,0.16,0.19],8=>[0.9,0.64,0.17],_=>pigment};}
        if kind==2 {pigment=match p[1]/24 {3=>[0.36,0.65,0.71],7=>[0.62,0.84,0.84],8=>[0.7,0.96,1.0],_=>pigment};}
        for i in 0..3 {p[i]=(pigment[i]*shade*255.).min(255.) as u8;}
    }
    pixels
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
    for kind in 0..3 {
    for dir in 0..4 {
        for pose in 0..COMBAT_FRAMES {
            let f = creature_frame(kind, dir, pose);
            for y in 0..COMBAT_H {
                for x in 0..COMBAT_W {
                    let dst = (((64+kind*COMBAT_FRAMES+pose) as usize*HEIGHT as usize + (dir/2) as usize*COMBAT_H+y)*WIDTH as usize+(dir%2) as usize*COMBAT_W+x)*2;
                    let src=(y*COMBAT_W+x)*4;
                    out[dst..dst+2].copy_from_slice(&f.pixels[src..src+2]);
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
    fn all_foes_pack_distinct_frames_without_overwriting_each_other() {
        let atlas=generate();
        for kind in 0..3 {for dir in 0..4 {for pose in [0,2,11,14,18,23] {
            let f=creature_frame(kind,dir,pose);let mut visible=0;
            for y in 0..COMBAT_H {for x in 0..COMBAT_W {
                let src=(y*COMBAT_W+x)*4;
                let dst=(((64+kind*COMBAT_FRAMES+pose) as usize*HEIGHT as usize+(dir/2) as usize*COMBAT_H+y)*WIDTH as usize+(dir%2) as usize*COMBAT_W+x)*2;
                assert_eq!(&atlas[dst..dst+2],&f.pixels[src..src+2]);
                if f.pixels[src+3]>0{visible+=1;}
            }}
            assert!(visible>300);
        }}}
        assert_ne!(creature_frame(0,0,0).pixels,creature_frame(1,0,0).pixels);
        assert_ne!(creature_frame(1,0,0).pixels,creature_frame(2,0,0).pixels);
    }
    #[test]
    fn skeleton_animation_fits_atlas_and_matches_shader_packing() {
        let atlas = generate();
        assert_eq!(atlas.len(), WIDTH as usize * HEIGHT as usize * LAYERS as usize * 2);
        for dir in 0..4u32 {
            let mut distinct = std::collections::HashSet::new();
            for pose in 0..COMBAT_FRAMES {
                let frame = combat_frame(dir, pose);
                assert_eq!(frame.pixels.len(), COMBAT_W * COMBAT_H * 4);
                distinct.insert(frame.pixels.clone());
                let packed = 65536u32 + dir*16 + (pose%4)*64 + (pose/4)*262144;
                assert_eq!((packed as f32) as u32, packed);
                assert_eq!((packed/64)%4 + ((packed/262144)%8)*4, pose);
                let mut coverage = 0;
                for y in 0..COMBAT_H {
                    for x in 0..COMBAT_W {
                        let src = (y*COMBAT_W+x)*4;
                        let dst = (((64+pose as usize)*HEIGHT as usize + (dir as usize/2)*COMBAT_H+y)*WIDTH as usize+(dir as usize%2)*COMBAT_W+x)*2;
                        assert_eq!(&atlas[dst..dst+2], &frame.pixels[src..src+2]);
                        if frame.pixels[src+3]>0 {coverage+=1;}
                    }
                }
                assert!(coverage>300, "empty skeleton {dir}/{pose}");
            }
            // Phase boundary frames intentionally share their end/start poses.
            assert!(distinct.len() >= 14, "animation lost pose variation");
            for (a,b) in [(0,2),(8,11),(12,14),(15,17),(18,19),(20,23)] {
                assert_ne!(combat_frame(dir,a).pixels, combat_frame(dir,b).pixels);
            }
        }
        let height = |pose| {
            let p=combat_frame(0,pose).pixels;
            let rows: Vec<_>=(0..COMBAT_H).filter(|&y| (0..COMBAT_W).any(|x| p[(y*COMBAT_W+x)*4+3]>0)).collect();
            rows.last().unwrap()-rows[0]
        };
        assert!(height(23)<height(0)/3, "death should collapse to the ground");
    }

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
