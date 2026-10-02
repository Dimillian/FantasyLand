//! Content-facing placement contracts. Adding a family means defining its
//! eligibility here and its shape in recipes.rs; streaming needs no new branch.
use super::NaturalKind;
#[derive(Clone, Copy)]
pub struct Definition {
    pub kind: NaturalKind,
    pub max_slope: f32,
    pub max_support_relief: f32,
    pub walk_through: bool,
}
const fn family(
    kind: NaturalKind,
    max_slope: f32,
    max_support_relief: f32,
    walk_through: bool,
) -> Definition {
    Definition {
        kind,
        max_slope,
        max_support_relief,
        walk_through,
    }
}
pub const FAMILIES: [Definition; 10] = [
    family(NaturalKind::Arch, 0.38, 10., true),
    family(NaturalKind::StoneWindow, 0.38, 10., true),
    family(NaturalKind::Pinnacles, 0.78, 24., false),
    family(NaturalKind::GraniteTor, 0.78, 24., false),
    family(NaturalKind::BoulderRing, 0.38, 10., true),
    family(NaturalKind::BrokenEscarpment, 0.78, 24., true),
    family(NaturalKind::BasaltPipes, 0.78, 24., false),
    family(NaturalKind::SplitMonolith, 0.78, 24., true),
    family(NaturalKind::CoastalStacks, 0.58, 24., false),
    family(NaturalKind::Hoodoos, 0.78, 24., false),
];
pub fn definition(kind: NaturalKind) -> &'static Definition {
    &FAMILIES[kind as usize]
}
