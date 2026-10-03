//! Current character state and authoritative starter-item rules.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Skill {
    pub rank: u32,
    pub xp: u32,
}
impl Default for Skill {
    fn default() -> Self {
        Self { rank: 1, xp: 0 }
    }
}
impl Skill {
    pub fn next(&self) -> u32 {
        50 + self.rank * 25
    }
    pub fn award(&mut self, amount: u32) -> bool {
        if self.rank >= 100 {
            return false;
        }
        self.xp += amount;
        let before = self.rank;
        while self.rank < 100 && self.xp >= self.next() {
            self.xp -= self.next();
            self.rank += 1;
        }
        if self.rank == 100 {
            self.xp = 0;
        }
        self.rank > before
    }
    fn valid(&self) -> bool {
        (1..=100).contains(&self.rank) && self.xp < self.next() && (self.rank < 100 || self.xp == 0)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Character {
    pub strength: u32,
    pub endurance: u32,
    pub agility: u32,
    pub intellect: u32,
    pub willpower: u32,
    pub sword_equipped: bool,
    pub shield_equipped: bool,
    pub blades: Skill,
    pub blocking: Skill,
    pub inventory: Vec<crate::loot::Stack>,
}
impl Default for Character {
    fn default() -> Self {
        Self {
            strength: 12,
            endurance: 10,
            agility: 10,
            intellect: 10,
            willpower: 10,
            sword_equipped: true,
            shield_equipped: true,
            blades: Skill::default(),
            blocking: Skill::default(),
            inventory: crate::loot::starter(),
        }
    }
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Derived {
    pub max_health: f32,
    pub max_stamina: f32,
    pub max_mana: f32,
    pub damage: f32,
    pub attack_cost: f32,
    pub block_cost: f32,
    pub protection: f32,
    pub stamina_regen: f32,
}
impl Character {
    pub fn derived(&self) -> Derived {
        Derived {
            max_health: 60. + self.endurance as f32 * 4.,
            max_stamina: 60. + self.endurance as f32 * 4.,
            max_mana: 60. + self.intellect as f32 * 4.,
            damage: if self.sword_equipped {
                21. + self.strength as f32 / 3.
            } else {
                0.
            },
            attack_cost: (18. - self.agility as f32 * 0.2).max(8.),
            block_cost: 20.,
            protection: if self.shield_equipped { 2. } else { 0. },
            stamina_regen: 12. + self.willpower as f32 * 0.2,
        }
    }
    pub fn valid(&self) -> bool {
        [
            self.strength,
            self.endurance,
            self.agility,
            self.intellect,
            self.willpower,
        ]
        .iter()
        .all(|v| (1..=50).contains(v))
            && self.blades.valid()
            && self.blocking.valid()
            && crate::loot::valid(&self.inventory)
            && (!self.sword_equipped || self.inventory.iter().any(|s| s.id == "iron-sword"))
            && (!self.shield_equipped || self.inventory.iter().any(|s| s.id == "oak-shield"))
    }
    pub fn equip(&mut self, id: &str, on: bool) -> bool {
        if !self.inventory.iter().any(|s| s.id == id) {
            return false;
        }
        match id {
            "iron-sword" => self.sword_equipped = on,
            "oak-shield" => self.shield_equipped = on,
            _ => return false,
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skills_roll_over_without_unlocking_damage() {
        let mut c = Character::default();
        let damage = c.derived().damage;
        assert!(!c.blades.award(70));
        assert!(c.blades.award(12));
        assert_eq!(c.blades, Skill { rank: 2, xp: 7 });
        assert_eq!(c.derived().damage, damage);
        assert!(c.valid());
    }
    #[test]
    fn equipment_controls_damage_defense_and_invalid_ids() {
        let mut c = Character::default();
        assert_eq!(c.derived().damage, 25.);
        assert!(c.equip("iron-sword", false));
        assert_eq!(c.derived().damage, 0.);
        assert!(c.equip("oak-shield", false));
        assert_eq!(c.derived().protection, 0.);
        assert!(!c.equip("invented-item", true));
        c.blades.rank = 0;
        assert!(!c.valid());
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStat {
    pub label: &'static str,
    pub value: String,
    pub suffix: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: &'static str,
    pub name: &'static str,
    pub tier: &'static str,
    pub category: &'static str,
    pub icon: &'static str,
    pub equipped: bool,
    pub slot: &'static str,
    pub description: &'static str,
    pub group: &'static str,
    pub quantity: u32,
    pub weight: f32,
    pub main_stats: Vec<ItemStat>,
    pub details: Vec<ItemStat>,
}
impl Character {
    pub fn items(&self) -> Vec<ItemView> {
        let d = self.derived();
        let stat = |label, value: String, suffix| ItemStat {
            label,
            value,
            suffix,
        };
        let mut views = vec![
            ItemView {
                group: "equipment",
                quantity: 1,
                weight: 2.5,
                id: "iron-sword",
                name: "Iron arming sword",
                tier: "common",
                category: "One-handed blade",
                icon: "blade",
                equipped: self.sword_equipped,
                slot: "Main hand",
                description: "A serviceable iron blade, its edge nicked by an earlier journey.",
                main_stats: vec![stat(
                    "Damage",
                    format!("{:.0}", 21. + self.strength as f32 / 3.),
                    "",
                )],
                details: vec![
                    stat("Base damage", "21".into(), ""),
                    stat(
                        "Strength bonus",
                        format!("+{:.0}", self.strength as f32 / 3.),
                        "",
                    ),
                    stat("Attack cost", format!("{:.0}", d.attack_cost), "stamina"),
                    stat("Reach", "2.65".into(), "m"),
                    stat("Weight", "2.5".into(), "kg"),
                ],
            },
            ItemView {
                group: "equipment",
                quantity: 1,
                weight: 3.,
                id: "oak-shield",
                name: "Oak round shield",
                tier: "common",
                category: "Shield",
                icon: "shield",
                equipped: self.shield_equipped,
                slot: "Off hand",
                description:
                    "Oak planks bound in iron. Turns aside frontal blows while your stamina holds.",
                main_stats: vec![
                    stat("Frontal block", "100".into(), "%"),
                    stat("Protection", "2".into(), ""),
                ],
                details: vec![
                    stat("Block cost", format!("{:.0}", d.block_cost), "stamina"),
                    stat("Coverage", "Frontal attacks".into(), ""),
                    stat("Protection", "−2 incoming damage".into(), ""),
                    stat("Weight", "3.0".into(), "kg"),
                ],
            },
        ];
        views.retain(|v| self.inventory.iter().any(|s| s.id == v.id));
        for s in &self.inventory {
            let Some(i) = crate::loot::item(&s.id) else {
                continue;
            };
            if let Some(v) = views.iter_mut().find(|v| v.id == s.id) {
                v.quantity = s.quantity;
                v.weight = i.weight * s.quantity as f32;
                continue;
            }
            views.push(ItemView {
                id: i.id,
                name: i.name,
                tier: i.tier,
                category: if i.group == "essences" {
                    "Enchantment essence"
                } else {
                    "Monster part"
                },
                icon: i.icon,
                equipped: false,
                slot: "Material",
                description: i.description,
                group: i.group,
                quantity: s.quantity,
                weight: i.weight * s.quantity as f32,
                main_stats: vec![stat("Quantity", s.quantity.to_string(), "")],
                details: vec![
                    stat("Unit weight", format!("{:.2}", i.weight), "kg"),
                    stat(
                        "Stack weight",
                        format!("{:.2}", i.weight * s.quantity as f32),
                        "kg",
                    ),
                ],
            });
        }
        views
    }
}
