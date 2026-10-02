//! Persisted world identity and immutable recipe configuration.
//! Any change to generated terrain, placement or identity MUST increment
//! GENERATOR_VERSION. Never load an unsupported version as the current world.
use serde::{Deserialize, Serialize};
pub const GENERATOR_VERSION: u32 = 1;
pub const RECIPE_REVISION: u32 = 1;
pub mod mountains;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldDescriptor {
    pub seed: u32,
    pub generator_version: u32,
    pub recipe_revision: u32,
}
impl WorldDescriptor {
    pub const fn current(seed: u32) -> Self {
        Self {
            seed,
            generator_version: GENERATOR_VERSION,
            recipe_revision: RECIPE_REVISION,
        }
    }
    pub fn validate(self) -> Result<(), &'static str> {
        if self != Self::current(self.seed) {
            return Err("This world requires a different generator/recipe version. Its save has not been changed.");
        }
        Ok(())
    }
}
/// Exact logical coordinates, never a random hash or a floating-point JS integer.
/// Scoped by the save's WorldDescriptor. Slot identifies parts within a location.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LocationId(pub String);
impl LocationId {
    pub fn cell(namespace: &str, x: i32, z: i32, slot: u32) -> Self {
        Self(format!("{namespace}:{x}:{z}:{slot}"))
    }
}
pub struct NaturalRecipe {
    pub cell_size: f32,
    pub max_radius: f32,
    pub candidate_probability: f32,
    pub settlement_clearance: f32,
}
pub const NATURAL: NaturalRecipe = NaturalRecipe {
    cell_size: 768.,
    max_radius: 112.,
    candidate_probability: 0.34,
    settlement_clearance: 30.,
};
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_preserves_full_seed_range_and_exact_signed_cells() {
        for seed in [0, 1, 1337, u32::MAX] {
            assert!(WorldDescriptor::current(seed).validate().is_ok());
        }
        let mut old = WorldDescriptor::current(42);
        old.generator_version += 1;
        assert!(old.validate().is_err());
        let mut keys = std::collections::HashSet::new();
        for x in -200..200 {
            for z in -200..200 {
                assert!(keys.insert(LocationId::cell("natural", x, z, 0)));
            }
        }
        assert_ne!(
            LocationId::cell("natural", 1, 2, 0),
            LocationId::cell("ruin", 1, 2, 0)
        );
    }
}
