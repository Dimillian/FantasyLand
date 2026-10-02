//! Regional mountain envelopes; geometry evaluates these without allocation.
//! Broad compact massifs and separate groups leave substantial lowland corridors.
#[derive(Clone, Copy)]
pub struct MountainRecipe {
    pub center: [f32; 2],
    pub angle: f32,
    pub length: f32,
    pub width: f32,
    pub height: f32,
    pub island: Option<usize>,
}
const fn range(
    x: f32,
    z: f32,
    angle: f32,
    length: f32,
    width: f32,
    height: f32,
    island: Option<usize>,
) -> MountainRecipe {
    MountainRecipe {
        center: [x, z],
        angle,
        length,
        width,
        height,
        island,
    }
}
pub const RANGES: [MountainRecipe; 11] = [
    range(-49000., -25000., 0.68, 42000., 16000., 3900., None),
    range(44000., 33000., -0.55, 35000., 17500., 4100., None),
    range(-52000., 62000., -0.85, 29000., 13500., 3550., None),
    range(-10000., -53000., -0.48, 18000., 11500., 2900., None),
    range(67000., -7000., 0.62, 21000., 12500., 3100., None),
    range(12000., 79000., 0.24, 18500., 11000., 2950., None),
    range(0., 0., 1.35, 15000., 6200., 2300., Some(0)),
    range(0., 0., 0.18, 15500., 6800., 2400., Some(1)),
    range(0., 0., 1.18, 15500., 6400., 2650., Some(2)),
    range(0., 0., -0.50, 14500., 6600., 2350., Some(3)),
    range(0., 0., 0.15, 17500., 6900., 2500., Some(4)),
];
