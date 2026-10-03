//! Save only mutable state; deterministic geography is reconstructed from identity.
//! Validate the entire snapshot before mutating the running game.
use crate::{world::World, worldgen::WorldDescriptor};
use serde::{Deserialize, Serialize};
pub const SAVE_VERSION: u32 = 3;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: u32,
    pub world: WorldDescriptor,
    pub player: PlayerState,
    pub clock: f64,
    pub character: crate::progression::Character,
    pub defeated: Vec<String>,
    pub corpses:Vec<crate::loot::Corpse>,
    pub doors: Vec<DoorState>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerState {
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub mana: f32,
    pub stamina: f32,
    pub walked: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoorState {
    pub id: u32,
    pub angle: f32,
    pub target: f32,
    pub hold: f32,
}
impl Snapshot {
    pub fn validate(&self, expected: WorldDescriptor) -> Result<(), &'static str> {
        self.world.validate()?;
        if self.schema != SAVE_VERSION || self.world != expected {
            return Err("Incompatible save or world identity.");
        }
        if !self.character.valid() || self.defeated.len()>10000 || self.defeated.iter().any(|id| id.len()>100 || !id.starts_with("monster:")) || self.defeated.iter().collect::<std::collections::HashSet<_>>().len()!=self.defeated.len() {return Err("Invalid character or encounter state.");}
        if self.corpses.len()>10000||self.corpses.iter().any(|c|!c.valid()||!self.defeated.contains(&c.id))||self.corpses.iter().map(|c|&c.id).collect::<std::collections::HashSet<_>>().len()!=self.corpses.len(){return Err("Invalid corpse loot state.");}
        let stats=self.character.derived();
        let p = &self.player;
        if ![
            p.x, p.z, p.yaw, p.pitch, p.health, p.mana, p.stamina, p.walked,
        ]
        .iter()
        .all(|v| v.is_finite())
            || p.x.abs() >= (crate::world::WORLD_SIZE * 0.5)
            || p.z.abs() >= (crate::world::WORLD_SIZE * 0.5)
            || p.pitch.abs() > 1.42
            || p.yaw.abs() > 1e6
            || p.walked > 1e12
            || !(0. ..=stats.max_health).contains(&p.health)
            || !(0. ..=stats.max_mana).contains(&p.mana)
            || !(0. ..=stats.max_stamina).contains(&p.stamina)
            || p.walked < 0.
            || !self.clock.is_finite()
            || !(0. ..=1e12).contains(&self.clock)
        {
            return Err("Invalid player state in save.");
        }
        if self.doors.len() > 10000 {
            return Err("Save contains too many door states.");
        }
        let mut ids = std::collections::HashSet::new();
        for d in &self.doors {
            if !ids.insert(d.id)
                || ![d.angle, d.target, d.hold].iter().all(|v| v.is_finite())
                || !(0. ..=1.).contains(&d.angle)
                || !(0. ..=1.).contains(&d.target)
                || !(0. ..=3600.).contains(&d.hold)
            {
                return Err("Invalid door state in save.");
            }
        }
        Ok(())
    }
    pub(crate) fn capture(world: &World, p: &crate::player::Player, clock: f64) -> Self {
        let mut doors: Vec<_> = world
            .doors
            .borrow()
            .iter()
            .map(|(&id, d)| DoorState {
                id,
                angle: d.angle,
                target: d.target,
                hold: d.hold,
            })
            .collect();
        doors.sort_by_key(|d| d.id);
        Self {
            schema: SAVE_VERSION,
            character:p.character.clone(),
            defeated:vec![],corpses:vec![],
            world: WorldDescriptor::current(world.seed),
            clock,
            doors,
            player: PlayerState {
                x: p.position.x,
                z: p.position.z,
                yaw: p.yaw,
                pitch: p.pitch,
                health: p.health,
                mana: p.mana,
                stamina: p.stamina,
                walked: p.walked,
            },
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_roundtrip_rejects_mismatches_corruption_and_duplicate_ids() {
        let mut s = Snapshot {
            schema: SAVE_VERSION,
            character:Default::default(),
            defeated:vec![],corpses:vec![],
            world: WorldDescriptor::current(0),
            clock: 2881.,
            doors: vec![],
            player: PlayerState {
                x: 10.,
                z: 20.,
                yaw: 1.,
                pitch: 0.,
                health: 60.,
                mana: 50.,
                stamina: 70.,
                walked: 42.,
            },
        };
        let json = serde_json::to_string(&s).unwrap();
        let read: Snapshot = serde_json::from_str(&json).unwrap();
        assert!(read.validate(s.world).is_ok());
        assert_eq!(read.player.health, 60.);
        assert!(s.validate(WorldDescriptor::current(1)).is_err());
        s.player.x = f32::NAN;
        assert!(s.validate(s.world).is_err());
        s.player.x = 10.;
        let d = DoorState {
            id: 1,
            angle: 0.5,
            target: 1.,
            hold: 2.,
        };
        s.doors = vec![d.clone(), d];
        assert!(s.validate(s.world).is_err());
    }
}
