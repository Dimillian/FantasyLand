//! Seeded drops and item definitions, shared by inventory, weight and tooltips.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Stack {
    pub id: String,
    pub quantity: u32,
}
pub struct Item {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub icon: &'static str,
    pub tier: &'static str,
    pub weight: f32,
    pub description: &'static str,
}
pub const ITEMS:&[Item]=&[
 Item{id:"iron-sword",name:"Iron arming sword",group:"equipment",icon:"blade",tier:"common",weight:2.5,description:"A serviceable iron blade, its edge nicked by an earlier journey."},
 Item{id:"oak-shield",name:"Oak round shield",group:"equipment",icon:"shield",tier:"common",weight:3.,description:"Oak planks bound in iron. Turns aside frontal blows while your stamina holds."},
 Item{id:"old-bone",name:"Ancient bone",group:"parts",icon:"bone",tier:"common",weight:0.2,description:"A dense fragment of an unquiet skeleton. A monster part reserved for future crafting."},
 Item{id:"goblin-fang",name:"Goblin fang",group:"parts",icon:"fang",tier:"common",weight:0.05,description:"A hooked fang, still stained with bitter herbs. A monster part reserved for future crafting."},
 Item{id:"ectoplasm",name:"Ectoplasm",group:"parts",icon:"ectoplasm",tier:"uncommon",weight:0.1,description:"Pale residue left by a broken apparition. A monster part reserved for future crafting."},
 Item{id:"grave-essence",name:"Grave essence",group:"essences",icon:"essence",tier:"uncommon",weight:0.02,description:"A cold mote of lingering death. Reserved for the future enchantment system."},
 Item{id:"wild-essence",name:"Wild essence",group:"essences",icon:"essence",tier:"uncommon",weight:0.02,description:"A restless spark of untamed vitality. Reserved for the future enchantment system."},
 Item{id:"spectral-essence",name:"Spectral essence",group:"essences",icon:"essence",tier:"rare",weight:0.02,description:"A violet thread caught between worlds. Reserved for the future enchantment system."},
];
pub fn item(id: &str) -> Option<&'static Item> {
    ITEMS.iter().find(|i| i.id == id)
}
pub fn starter() -> Vec<Stack> {
    vec![
        Stack {
            id: "iron-sword".into(),
            quantity: 1,
        },
        Stack {
            id: "oak-shield".into(),
            quantity: 1,
        },
    ]
}
pub fn valid(stacks: &[Stack]) -> bool {
    stacks.len() <= 48
        && stacks
            .iter()
            .all(|s| item(&s.id).is_some() && (1..=1_000_000).contains(&s.quantity))
        && stacks
            .iter()
            .map(|s| &s.id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            == stacks.len()
}
pub fn weight(stacks: &[Stack]) -> f32 {
    stacks
        .iter()
        .filter_map(|s| item(&s.id).map(|i| i.weight * s.quantity as f32))
        .sum()
}
pub fn add(stacks: &mut Vec<Stack>, drops: &[Stack]) {
    for d in drops {
        if let Some(s) = stacks.iter_mut().find(|s| s.id == d.id) {
            s.quantity = (s.quantity + d.quantity).min(1_000_000);
        } else if stacks.len() < 48 {
            stacks.push(d.clone());
        }
    }
}
// Independent salts make individual table edits predictable, never frame/random-order dependent.
pub fn roll(seed: u32, encounter: &str, kind: u32) -> Vec<Stack> {
    let h = encounter.bytes().fold(seed ^ 0x4c4f4f54, |h, b| {
        (h ^ b as u32).wrapping_mul(16777619)
    });
    let r = |salt| crate::world::hash(h, salt, 17);
    let k = (kind as usize).min(2);
    let mut out = vec![Stack {
        id: ["old-bone", "goblin-fang", "ectoplasm"][k].into(),
        quantity: 1 + r(1) % 3,
    }];
    if r(2) % 100 < [45, 30, 80][k] {
        out.push(Stack {
            id: ["grave-essence", "wild-essence", "spectral-essence"][k].into(),
            quantity: 1,
        });
    }
    if kind < 2 && r(3) % 100 < 12 {
        out.push(Stack {
            id: if r(4) % 2 == 0 {
                "iron-sword"
            } else {
                "oak-shield"
            }
            .into(),
            quantity: 1,
        });
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drops_are_seeded_valid_and_stack() {
        for kind in 0..3 {
            for seed in 0..100 {
                let d = roll(seed, "monster:crypt:1", kind);
                assert_eq!(d, roll(seed, "monster:crypt:1", kind));
                assert!(valid(&d));
                let mut s = starter();
                add(&mut s, &d);
                let w = weight(&s);
                add(&mut s, &d);
                assert!(valid(&s));
                assert!(weight(&s) > w);
            }
        }
    }
    #[test]
    fn reject_unknown_duplicate_or_empty_stacks() {
        assert!(!valid(&[Stack {
            id: "x".into(),
            quantity: 1
        }]));
        let mut s = starter();
        s.push(s[0].clone());
        assert!(!valid(&s));
        s = starter();
        s[0].quantity = 0;
        assert!(!valid(&s));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpse {
    pub id: String,
    pub kind: u32,
    pub position: [f32; 3],
    pub items: Vec<Stack>,
}
impl Corpse {
    pub fn valid(&self) -> bool {
        self.id.starts_with("monster:")
            && self.id.len() <= 100
            && self.kind < 3
            && self
                .position
                .iter()
                .all(|v| v.is_finite() && v.abs() < 192000.)
            && valid(&self.items)
    }
    pub fn take(&mut self, inventory: &mut Vec<Stack>, id: &str) -> bool {
        let drops: Vec<_> = self
            .items
            .iter()
            .filter(|s| id == "all" || s.id == id)
            .cloned()
            .collect();
        if drops.is_empty() {
            return false;
        }
        let new = drops
            .iter()
            .filter(|d| !inventory.iter().any(|s| s.id == d.id))
            .count();
        if inventory.len() + new > 48
            || drops.iter().any(|d| {
                inventory
                    .iter()
                    .find(|s| s.id == d.id)
                    .is_some_and(|s| s.quantity > 1_000_000 - d.quantity)
            })
        {
            return false;
        }
        add(inventory, &drops);
        self.items.retain(|s| !drops.iter().any(|d| d.id == s.id));
        true
    }
    pub fn name(&self) -> &'static str {
        ["Barrow skeleton", "Thorn goblin", "Hollow ghost"][self.kind.min(2) as usize]
    }
}

#[cfg(test)]
mod corpse_tests {
    use super::*;
    #[test]
    fn loot_requires_take_and_survives_roundtrip_without_duplication() {
        let mut c = Corpse {
            id: "monster:crypt:0".into(),
            kind: 0,
            position: [0., 5., 0.],
            items: roll(1337, "monster:crypt:0", 0),
        };
        let mut bag = starter();
        assert_eq!(bag.len(), 2);
        let first = c.items[0].id.clone();
        assert!(c.take(&mut bag, &first));
        assert!(!c.take(&mut bag, &first));
        let mut restored: Corpse =
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert!(restored.valid());
        restored.take(&mut bag, "all");
        assert!(restored.items.is_empty());
        assert!(!restored.take(&mut bag, "all"));
        assert!(valid(&bag));
    }
}
