//! Geometry recipes only. Placement, identity and eligibility live in natural.rs.
use super::*;

struct Builder<'a> {
    w: &'a World,
    seed: u32,
    x: f32,
    z: f32,
    yaw: f32,
    color: [f32; 3],
    solids: Vec<Solid>,
}
impl Builder<'_> {
    fn world(&self, p: [f32; 2]) -> [f32; 2] {
        let q = rotated(p[0], p[1], self.yaw);
        [self.x + q[0], self.z + q[1]]
    }
    fn local_ground(&self, p: [f32; 2]) -> f32 {
        let q = self.world(p);
        ground(self.w, q[0], q[1])
    }
    /// Six sides retain a strong irregular lithic silhouette at both detail levels.
    fn footprint(
        &self,
        center: [f32; 2],
        size: [f32; 2],
        angle: f32,
        variant: u32,
        sides: usize,
    ) -> Vec<[f32; 2]> {
        (0..sides)
            .map(|i| {
                let a = i as f32 * TAU / sides as f32;
                let p = rotated(a.cos() * size[0], a.sin() * size[1], angle);
                let wobble = 0.90 + random(self.seed, variant + i as u32) * 0.16;
                self.world([center[0] + p[0] * wobble, center[1] + p[1] * wobble])
            })
            .collect()
    }
    fn column(
        &mut self,
        center: [f32; 2],
        size: [f32; 2],
        height: f32,
        angle: f32,
        variant: u32,
        detail: bool,
    ) {
        let footprint = self.footprint(center, size, angle, variant, 6);
        let bottom: Vec<_> = footprint
            .iter()
            .map(|p| ground(self.w, p[0], p[1]) - EMBED)
            .collect();
        let base = bottom.iter().copied().fold(f32::NEG_INFINITY, f32::max) + EMBED;
        let top = footprint
            .iter()
            .enumerate()
            .map(|(i, _)| {
                base + height * (0.90 + random(self.seed, variant + 40 + i as u32) * 0.13)
            })
            .collect();
        self.solids.push(Solid {
            footprint,
            bottom,
            top,
            grounded: true,
            detail,
            color: tint(self.color, 0.92 + random(self.seed, variant + 70) * 0.16),
        });
    }
    /// A bevel-sided eroded slab, with unequal depth and fractured upper edge.
    /// It is still the same shared convex solid used by body collision.
    fn beam(&mut self, x0: f32, x1: f32, z: f32, depth: f32, y0: f32, y1: f32, thickness: f32) {
        let salt = 2400 + self.solids.len() as u32 * 19;
        let middle = (x0 + x1) * 0.5;
        let d0 = depth * (0.70 + random(self.seed, salt) * 0.30);
        let d1 = depth * (0.75 + random(self.seed, salt + 1) * 0.30);
        let dm = depth * (1.03 + random(self.seed, salt + 2) * 0.18);
        let local = vec![
            [x0, z - d0],
            [middle, z - dm],
            [x1, z - d1],
            [x1, z + d1],
            [middle, z + dm],
            [x0, z + d0],
        ];
        let footprint = local.iter().map(|p| self.world(*p)).collect();
        let bottom: Vec<f32> = local
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let t = ((p[0] - x0) / (x1 - x0)).clamp(0., 1.);
                y0 + (y1 - y0) * t + (random(self.seed, salt + 3 + i as u32) - 0.5) * 0.26 * depth
            })
            .collect();
        let top = bottom
            .iter()
            .enumerate()
            .map(|(i, y)| y + thickness * (0.70 + random(self.seed, salt + 11 + i as u32) * 0.80))
            .collect();
        self.solids.push(Solid {
            footprint,
            bottom,
            top,
            grounded: false,
            detail: false,
            color: self.color,
        });
    }
    /// Attached scree buttress: its triangulated top rises to a broken off-center
    /// crest and falls back into the ground. No square pedestal or hidden cone.
    fn talus(&mut self, center: [f32; 2], size: [f32; 2], height: f32, angle: f32, variant: u32) {
        let footprint = self.footprint(center, size, angle, variant, 6);
        let bottom: Vec<_> = footprint
            .iter()
            .map(|p| ground(self.w, p[0], p[1]) - EMBED)
            .collect();
        let crest = bottom.iter().copied().fold(f32::NEG_INFINITY, f32::max) + EMBED;
        let profile = [0.12, 0.46, 1.0, 0.75, 0.18, 0.05];
        let top = bottom
            .iter()
            .enumerate()
            .map(|(i, y)| {
                (*y + 0.15).max(
                    crest
                        + height
                            * profile[i]
                            * (0.85 + random(self.seed, variant + 30 + i as u32) * 0.22),
                )
            })
            .collect();
        self.solids.push(Solid {
            footprint,
            bottom,
            top,
            grounded: true,
            detail: false,
            color: tint(self.color, 0.91),
        });
    }
    fn cap(
        &mut self,
        center: [f32; 2],
        size: [f32; 2],
        y: f32,
        thickness: f32,
        angle: f32,
        variant: u32,
    ) {
        let footprint = self.footprint(center, size, angle, variant, 6);
        self.solids.push(Solid {
            bottom: vec![y; 6],
            top: (0..6)
                .map(|i| y + thickness * (1.0 + random(self.seed, variant + 81 + i) * 0.40))
                .collect(),
            footprint,
            grounded: false,
            detail: false,
            color: tint(self.color, 0.94),
        });
    }
}
impl Builder<'_> {
    /// Offset, narrowing upper courses make a leaning silhouette while retaining
    /// exact individual solids. Broken variants end in a sloping fracture face.
    fn spire(
        &mut self,
        c: [f32; 2],
        size: [f32; 2],
        height: f32,
        angle: f32,
        variant: u32,
        broken: bool,
    ) {
        let body = height
            * if broken {
                0.55 + random(self.seed, variant + 201) * 0.24
            } else {
                0.67
            };
        self.column(c, size, body, angle, variant, false);
        let lower = self.solids.last_mut().unwrap();
        let base = lower
            .bottom
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
            + EMBED;
        if broken {
            for (i, y) in lower.top.iter_mut().enumerate() {
                *y = base
                    + body
                        * (0.73
                            + (i as f32 * 0.77).sin() * 0.22
                            + random(self.seed, variant + 210 + i as u32) * 0.21);
            }
        } else {
            let contact = lower.top.iter().copied().fold(f32::INFINITY, f32::min) - 0.38;
            let tilt = rotated(
                size[0] * (0.09 + random(self.seed, variant + 220) * 0.10),
                size[1] * (random(self.seed, variant + 221) - 0.5) * 0.15,
                angle,
            );
            self.cap(
                [c[0] + tilt[0], c[1] + tilt[1]],
                [size[0] * 0.66, size[1] * 0.65],
                contact,
                height * 0.32,
                angle,
                variant + 230,
            );
        }
    }
    fn arch_span(
        &mut self,
        cx: f32,
        half: f32,
        depth: f32,
        scale: f32,
        baseline: f32,
        rise: f32,
        spring: f32,
        window: bool,
        collapsed: bool,
        skip_support: i32,
        variant: u32,
    ) {
        let leg = (2.6 + random(self.seed, variant + 1) * 0.7) * scale;
        for side in [-1., 1.] {
            if side as i32 == skip_support {
                continue;
            }
            let salt = variant + (side + 1.) as u32 * 53;
            let c = [cx + side * half, side * 0.22 * depth];
            let h = (baseline + spring + 4.6 * scale - self.local_ground(c)).max(5.);
            self.column(c, [leg, depth * 1.14], h, side * 0.17, salt + 10, false);
            self.talus(
                [c[0] + side * 1.8 * scale, c[1]],
                [5.8 * scale, depth * 1.85],
                h * 0.83,
                side * 0.18,
                salt + 40,
            );
            self.talus(
                [c[0] + side * 4.7 * scale, c[1] - depth * 0.65],
                [5.7 * scale, depth * 1.52],
                h * 0.39,
                side * 0.35,
                salt + 70,
            );
        }
        let slabs = 5 + (random(self.seed, variant + 130) * 4.) as usize;
        let weights: Vec<f32> = (0..slabs)
            .map(|i| 0.6 + random(self.seed, variant + 140 + i as u32) * 0.9)
            .collect();
        let total: f32 = weights.iter().sum();
        let mut left = -1.;
        let profile = |u: f32| baseline + spring + rise * (1. - u * u).max(0.).sqrt() + u * scale;
        for (i, w) in weights.iter().enumerate() {
            let right = if i + 1 == slabs {
                1.
            } else {
                left + 2. * w / total
            };
            if !collapsed || !(i == slabs / 2 || i + 1 == slabs / 2) {
                self.beam(
                    cx + left * half * 0.95 - 0.30 * scale,
                    cx + right * half * 0.95 + 0.30 * scale,
                    (random(self.seed, variant + 160 + i as u32) - 0.5) * 0.30 * depth,
                    depth * (0.90 + random(self.seed, variant + 180 + i as u32) * 0.28),
                    profile(left),
                    profile(right),
                    (2.3 + random(self.seed, variant + 200 + i as u32) * 1.9) * scale,
                );
            }
            left = right;
        }
        if window {
            let parts = if random(self.seed, variant + 240) < 0.45 {
                1
            } else {
                2
            };
            for i in 0..parts {
                let x0 = -half + 2. * half * i as f32 / parts as f32;
                let x1 = -half + 2. * half * (i + 1) as f32 / parts as f32;
                self.beam(
                    cx + x0 - 0.2 * scale,
                    cx + x1 + 0.2 * scale,
                    0.,
                    depth,
                    baseline + (5.1 + 1.0 * i as f32) * scale,
                    baseline + (5.9 - 0.25 * i as f32) * scale,
                    3.0 * scale,
                );
            }
        }
        if collapsed {
            // Fallen pieces collect beside the cleft; neither entrance becomes a dam.
            for i in 0..2 {
                self.talus(
                    [cx + half + 3. * scale, (i as f32 - 0.5) * 6. * scale],
                    [4.5 * scale, 3.5 * scale],
                    (2.2 + i as f32) * scale,
                    0.3 + i as f32,
                    variant + 260 + i * 23,
                );
            }
        }
    }
    /// Extend the parent rock fabric along geological strike. These connected low
    /// shelves and broken fins bridge the main mass into its actual hillside.
    fn landform_skirt(&mut self, rarity: NaturalRarity, scale: f32, kind: NaturalKind) {
        let mut left = (f32::INFINITY, 0.);
        let mut right = (f32::NEG_INFINITY, 0.);
        for solid in self.solids.iter().filter(|s| !s.detail) {
            for p in &solid.footprint {
                let local = rotated(p[0] - self.x, p[1] - self.z, -self.yaw);
                if local[0] < left.0 {
                    left = (local[0], local[1]);
                }
                if local[0] > right.0 {
                    right = (local[0], local[1]);
                }
            }
        }
        let layers = match rarity {
            NaturalRarity::Common => 2,
            NaturalRarity::Rare => 3,
            NaturalRarity::Monumental => 4,
        };
        let reach = scale.min(1.35);
        for (side, edge) in [(-1., left), (1., right)] {
            for i in 0..layers {
                let salt = 3700 + (side + 1.) as u32 * 80 + i * 19;
                let decay = 1. - i as f32 * 0.16;
                let center = [
                    edge.0 + side * (i as f32 * 5.5 - 2.) * reach,
                    edge.1 + (random(self.seed, salt) - 0.5) * 5. * reach,
                ];
                let width = (9. + random(self.seed, salt + 1) * 4.) * reach * decay;
                let depth = (5. + random(self.seed, salt + 2) * 3.) * reach * decay;
                let h = (3.1 + random(self.seed, salt + 3) * 3.8) * scale * decay;
                self.talus(
                    center,
                    [width, depth],
                    h,
                    if side > 0. { 0.12 } else { PI - 0.12 },
                    salt + 4,
                );
                if i == 0
                    && rarity != NaturalRarity::Common
                    && matches!(
                        kind,
                        NaturalKind::GraniteTor
                            | NaturalKind::BrokenEscarpment
                            | NaturalKind::CoastalStacks
                    )
                {
                    self.column(
                        [center[0], center[1] + depth * 0.1],
                        [width * 0.55, depth * 0.32],
                        h * 1.25,
                        0.08,
                        salt + 14,
                        false,
                    );
                }
            }
        }
    }
}

pub(super) fn recipe(
    world: &World,
    seed: u32,
    kind: NaturalKind,
    x: f32,
    z: f32,
    yaw: f32,
    color: [f32; 3],
) -> NaturalLandmark {
    let rarity = NaturalRarity::from_id(seed);
    let scale = rarity.scale(seed);
    let mut b = Builder {
        w: world,
        seed,
        x,
        z,
        yaw,
        color,
        solids: Vec::new(),
    };
    match kind {
        NaturalKind::Arch | NaturalKind::StoneWindow => {
            let half = (9. + random(seed, 101) * 4.) * scale;
            let depth = (2.7 + random(seed, 102) * 1.6) * scale;
            let rise = (8. + random(seed, 103) * 6.) * scale;
            let spring = (if kind == NaturalKind::StoneWindow {
                12.7
            } else {
                6.5
            }) * scale;
            let extra = match rarity {
                NaturalRarity::Common => 0,
                NaturalRarity::Rare => usize::from(random(seed, 3210) < 0.62),
                NaturalRarity::Monumental => 1 + usize::from(random(seed, 3210) < 0.35),
            };
            let mut spans: Vec<(f32, f32, bool, i32)> = vec![(0., half, false, 0)];
            for i in 0..extra {
                let side = if i == 0 {
                    if random(seed, 3211) < 0.5 {
                        -1.
                    } else {
                        1.
                    }
                } else {
                    -spans[1].0.signum()
                };
                let secondary = half * (0.57 + random(seed, 3212 + i as u32) * 0.28);
                spans.push((
                    side * (half + secondary),
                    secondary,
                    random(seed, 3215 + i as u32) < 0.38,
                    -side as i32,
                ));
            }
            let baseline = spans
                .iter()
                .flat_map(|&(cx, h, _, _)| [[cx - h, 0.], [cx + h, 0.], [cx, 0.]])
                .map(|p| b.local_ground(p))
                .fold(f32::NEG_INFINITY, f32::max);
            for (i, &(cx, h, collapsed, skip)) in spans.iter().enumerate() {
                b.arch_span(
                    cx,
                    h,
                    depth * (if i == 0 { 1. } else { 0.85 }),
                    scale,
                    baseline,
                    rise * (h / half),
                    spring,
                    kind == NaturalKind::StoneWindow
                        && (i == 0 || random(seed, 3220 + i as u32) < 0.5),
                    collapsed,
                    skip,
                    1000 + i as u32 * 400,
                );
            }
            // Even a common arch can retain a short detached shoulder of an older
            // collapsed span; the main opening is always an intact usable passage.
            if extra == 0 && random(seed, 3229) < 0.30 {
                let side = if random(seed, 3230) < 0.5 { -1. } else { 1. };
                b.spire(
                    [side * (half + 9. * scale), depth * 0.4],
                    [3.0 * scale, depth],
                    9. * scale,
                    0.2,
                    3240,
                    true,
                );
            }
        }
        NaturalKind::Pinnacles => {
            let count = rarity.count(seed, 3301, [(3, 6), (5, 9), (8, 12)]);
            let pattern = random(seed, 3302);
            for i in 0..count {
                let a = i as f32 * 2.39996 + random(seed, 201 + i) * 0.42;
                let spread = (6. + (i as f32).sqrt() * 6.) * scale;
                let c = if pattern < 0.48 {
                    [
                        (i as f32 - (count - 1) as f32 * 0.5) * 5.6 * scale,
                        (random(seed, 221 + i) - 0.5) * 12. * scale,
                    ]
                } else {
                    [a.cos() * spread, a.sin() * spread * 0.66]
                };
                let height = (15. + random(seed, 241 + i) * 25.) * scale;
                let size = (2.0 + random(seed, 261 + i) * 2.3) * scale;
                b.spire(
                    c,
                    [size * 0.86, size * 0.65],
                    height,
                    a,
                    4000 + i * 300,
                    random(seed, 281 + i) < 0.28,
                );
                if i % 2 == 0 {
                    b.talus(
                        [c[0] + size * 0.2, c[1]],
                        [size * 1.65, size * 1.35],
                        height * 0.32,
                        a + 0.35,
                        4200 + i * 300,
                    );
                }
            }
        }
        NaturalKind::GraniteTor => {
            let count = rarity.count(seed, 3310, [(3, 5), (5, 7), (6, 9)]);
            let curved = random(seed, 3311) > 0.45;
            for i in 0..count {
                let along = (i as f32 - (count - 1) as f32 * 0.5) * 8.3 * scale;
                let c = [
                    along,
                    if curved {
                        (along / 17. / scale).sin() * 7. * scale
                    } else {
                        (random(seed, 301 + i) - 0.5) * 9. * scale
                    },
                ];
                let h = (7. + random(seed, 321 + i) * 12.) * scale;
                let size = [
                    (4.6 + random(seed, 341 + i) * 1.4) * scale,
                    (3.7 + random(seed, 361 + i)) * scale,
                ];
                b.column(
                    c,
                    size,
                    h,
                    0.2 * (random(seed, 381 + i) - 0.5),
                    5000 + i * 100,
                    false,
                );
                if random(seed, 391 + i) < 0.72 {
                    let contact = b
                        .solids
                        .last()
                        .unwrap()
                        .top
                        .iter()
                        .copied()
                        .fold(f32::INFINITY, f32::min)
                        - 0.7;
                    b.cap(
                        [c[0] + scale * 0.25, c[1]],
                        [size[0] * 1.04, size[1] * 0.98],
                        contact,
                        (2.4 + random(seed, 401 + i) * 3.5) * scale,
                        0.17,
                        5060 + i * 100,
                    );
                }
            }
        }
        NaturalKind::BoulderRing => {
            let count = rarity.count(seed, 3320, [(6, 8), (8, 11), (11, 14)]);
            let open = (0.20 + random(seed, 3321) * 0.14) * PI;
            for i in 0..count {
                let a = TAU * i as f32 / count as f32 + (random(seed, 601 + i) - 0.5) * 0.13;
                // Both N/S approaches remain open, even when the broken ring is asymmetric.
                if a.cos().abs() < open.sin() {
                    continue;
                }
                let r = (15. + random(seed, 621 + i) * 8.) * scale;
                let c = [
                    a.cos() * r,
                    a.sin() * r * (0.72 + random(seed, 641 + i) * 0.34),
                ];
                b.spire(
                    c,
                    [(3.4 + random(seed, 661 + i) * 1.5) * scale, 3.0 * scale],
                    (5. + random(seed, 681 + i) * 8.) * scale,
                    a,
                    6000 + i * 100,
                    random(seed, 691 + i) < 0.66,
                );
            }
        }
        NaturalKind::BrokenEscarpment => {
            for side in [-1., 1.] {
                let ss = (side + 1.) as u32 * 81;
                let count = rarity.count(seed, 3330 + ss, [(2, 3), (3, 4), (4, 5)]);
                for i in 0..count {
                    let c = [
                        side * (8.6 + i as f32 * 8.0) * scale,
                        (random(seed, 701 + i + ss) - 0.5) * 8. * scale,
                    ];
                    let h = (10. + i as f32 * 2. + random(seed, 721 + i + ss) * 8.) * scale;
                    let size = [
                        (4.7 + random(seed, 741 + i + ss)) * scale,
                        (2.5 + random(seed, 761 + i + ss) * 1.6) * scale,
                    ];
                    b.column(c, size, h, side * 0.08, 7000 + i * 200 + ss, false);
                    if random(seed, 781 + i + ss) < 0.63 {
                        let contact = b
                            .solids
                            .last()
                            .unwrap()
                            .top
                            .iter()
                            .copied()
                            .fold(f32::INFINITY, f32::min)
                            - 0.6;
                        b.cap(
                            c,
                            [size[0] * 1.07, size[1] * 1.13],
                            contact,
                            2.0 * scale,
                            0.1,
                            7060 + i * 200 + ss,
                        );
                    }
                    if i % 2 == 0 {
                        b.talus(
                            [c[0] + side * scale, c[1] - size[1]],
                            [size[0] * 1.16, size[1] * 1.8],
                            h * 0.38,
                            side * 0.22,
                            7100 + i * 200 + ss,
                        );
                    }
                }
            }
        }
        NaturalKind::BasaltPipes => {
            let count = rarity.count(seed, 3340, [(9, 16), (16, 24), (24, 34)]);
            let lobes = 1 + (random(seed, 3341) * (rarity.index() + 2) as f32) as u32;
            let sweep = (0.8 + random(seed, 3342) * 1.5) * PI;
            for i in 0..count {
                let lobe = i % lobes;
                let k = i / lobes;
                let a = k as f32 * 2.39996 + random(seed, 901 + i) * 0.45;
                let r = (1.5 + (k as f32).sqrt() * 2.25) * scale;
                let bend =
                    (lobe as f32 - (lobes - 1) as f32 * 0.5) * sweep / (lobes as f32).max(1.);
                let c = [
                    bend.sin() * 10. * scale + a.cos() * r,
                    bend.cos() * 5. * scale + a.sin() * r * 0.83,
                ];
                let h = (9. + random(seed, 921 + i) * 15. + (1. - k as f32 / count as f32) * 5.)
                    * scale;
                let width = (1.50 + random(seed, 941 + i) * 0.57) * scale;
                b.column(
                    c,
                    [width, width * 0.91],
                    h,
                    PI / 6. + random(seed, 961 + i) * 0.18,
                    8000 + i * 41,
                    false,
                );
                // Different broken roof planes retain true basalt columns while
                // removing the old identical row/column silhouette.
                if random(seed, 981 + i) < 0.32 {
                    let p = b.solids.last_mut().unwrap();
                    let base = p.bottom.iter().copied().fold(f32::NEG_INFINITY, f32::max) + EMBED;
                    for (j, y) in p.top.iter_mut().enumerate() {
                        *y = base + h * (0.49 + j as f32 * 0.045);
                    }
                }
            }
        }
        NaturalKind::SplitMonolith => {
            let h = (22. + random(seed, 1201) * 14.) * scale;
            for side in [-1., 1.] {
                let c = [
                    side * (6.4 + random(seed, 1202 + (side + 1.) as u32) * 0.8) * scale,
                    side * 1.3 * scale,
                ];
                b.spire(
                    c,
                    [3.6 * scale, 5.5 * scale],
                    h * (if side < 0. { 1. } else { 0.82 }),
                    side * 0.06,
                    10000 + (side + 1.) as u32 * 200,
                    false,
                );
                if random(seed, 1207 + (side + 1.) as u32) < 0.66 {
                    b.talus(
                        [c[0] + side * 4. * scale, c[1]],
                        [5.6 * scale, 4.8 * scale],
                        h * 0.34,
                        side * 0.2,
                        10400 + (side + 1.) as u32 * 80,
                    );
                }
            }
        }
        NaturalKind::CoastalStacks => {
            let count = rarity.count(seed, 3350, [(2, 4), (3, 5), (5, 7)]);
            let curve = (random(seed, 3351) - 0.5) * 1.9;
            for i in 0..count {
                let t = i as f32 - (count - 1) as f32 * 0.5;
                let c = [t * 10.8 * scale, (t * 0.57 + curve).sin() * 7. * scale];
                let h = (14. + random(seed, 1401 + i) * 19.) * scale;
                let size = [
                    (3.4 + random(seed, 1421 + i) * 1.5) * scale,
                    (2.8 + random(seed, 1441 + i)) * scale,
                ];
                b.spire(
                    c,
                    size,
                    h,
                    0.15 * i as f32,
                    11000 + i * 300,
                    random(seed, 1461 + i) < 0.24,
                );
            }
        }
        NaturalKind::Hoodoos => {
            let count = rarity.count(seed, 3360, [(3, 6), (6, 9), (8, 12)]);
            let wandering = random(seed, 3361) < 0.45;
            for i in 0..count {
                let a = i as f32 * 2.39996 + random(seed, 1601 + i) * 0.3;
                let r = (5. + (i as f32).sqrt() * 5.0) * scale;
                let c = if wandering {
                    [
                        (i as f32 - (count - 1) as f32 * 0.5) * 4.4 * scale,
                        (i as f32 * 0.9).sin() * 7. * scale,
                    ]
                } else {
                    [a.cos() * r, a.sin() * r * 0.77]
                };
                let width = (1.7 + random(seed, 1621 + i)) * scale;
                let h = (8. + random(seed, 1641 + i) * 13.) * scale;
                b.column(c, [width, width * 0.83], h, a, 12000 + i * 90, false);
                if random(seed, 1661 + i) > 0.18 {
                    let contact = b
                        .solids
                        .last()
                        .unwrap()
                        .top
                        .iter()
                        .copied()
                        .fold(f32::INFINITY, f32::min)
                        - 0.5;
                    b.cap(
                        [c[0] + width * 0.14, c[1]],
                        [width * (1.25 + random(seed, 1681 + i) * 0.55), width * 1.34],
                        contact,
                        2.2 * scale,
                        a + 0.13,
                        12050 + i * 90,
                    );
                }
            }
        }
    }
    b.landform_skirt(rarity, scale, kind);
    let debris = rarity.count(seed, 3380, [(3, 6), (5, 8), (7, 10)]);
    // Scattered chips stay close to actual supports and terminate the parent rock
    // fabric, rather than describing the same arbitrary ring around every family.
    let anchors: Vec<[f32; 2]> = b
        .solids
        .iter()
        .filter(|s| s.grounded)
        .map(|s| {
            let p = s
                .footprint
                .iter()
                .fold([0.; 2], |a, p| [a[0] + p[0], a[1] + p[1]]);
            rotated(
                p[0] / s.footprint.len() as f32 - x,
                p[1] / s.footprint.len() as f32 - z,
                -yaw,
            )
        })
        .collect();
    for i in 0..debris {
        let anchor =
            anchors[(random(seed, 2001 + i) * anchors.len() as f32) as usize % anchors.len()];
        let a = random(seed, 2021 + i) * TAU;
        let c = [
            anchor[0] + a.cos() * (2. + random(seed, 2041 + i) * 4.) * scale,
            anchor[1] + a.sin() * (2. + random(seed, 2061 + i) * 4.) * scale,
        ];
        // Central passages remain intentionally free of loose collider debris.
        if c[0].abs() < 3.5 * scale
            && matches!(
                kind,
                NaturalKind::Arch
                    | NaturalKind::StoneWindow
                    | NaturalKind::BoulderRing
                    | NaturalKind::BrokenEscarpment
                    | NaturalKind::SplitMonolith
            )
        {
            continue;
        }
        b.column(
            c,
            [(0.8 + random(seed, 2081 + i)) * scale, 0.9 * scale],
            (0.5 + random(seed, 2101 + i)) * scale,
            a,
            13000 + i * 23,
            true,
        );
    }
    let radius = b
        .solids
        .iter()
        .flat_map(|s| s.footprint.iter())
        .map(|p| (p[0] - x).hypot(p[1] - z))
        .fold(0., f32::max)
        + 0.5;
    let top = b
        .solids
        .iter()
        .flat_map(|s| s.top.iter().copied())
        .fold(f32::NEG_INFINITY, f32::max);
    NaturalLandmark {
        id: seed,
        name: format!(
            "{} {}",
            [
                "Elder", "Cinder", "Storm", "Pale", "Moon", "Hollow", "Whisper", "Sable", "Copper",
                "Frost", "Amber", "Mist"
            ][(seed as usize >> 8) % 12],
            match kind {
                NaturalKind::Arch => "Arch",
                NaturalKind::StoneWindow => "Window",
                NaturalKind::Pinnacles => "Needles",
                NaturalKind::GraniteTor => "Crown",
                NaturalKind::BoulderRing => "Amphitheatre",
                NaturalKind::BrokenEscarpment => "Gates",
                NaturalKind::BasaltPipes => "Organ",
                NaturalKind::SplitMonolith => "Cleft",
                NaturalKind::CoastalStacks => "Sentinels",
                NaturalKind::Hoodoos => "Spires",
            }
        ),
        kind,
        x,
        z,
        ground: ground(world, x, z),
        radius,
        top,
        yaw,
        solids: b.solids,
    }
}
