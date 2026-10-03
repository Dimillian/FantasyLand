// Skill descriptions only. Blades and Blocking progress comes from the Rust character.
// Other skills remain untrained until their gameplay systems exist.
// Skill entries: [name, icon, description, level, current XP, next-level XP].
export const skillGroups = [
  {
    name: 'Combat',
    skills: [
      ['Blades', 'blade', 'Swords, daggers, and precision strikes.', 1, 0, 75],
      ['Axes', 'axe', 'Axes and cleaving attacks.', 1, 0, 75],
      ['Blunt weapons', 'mace', 'Maces, hammers, and crushing attacks.', 1, 0, 75],
      ['Polearms', 'spear', 'Spears, staves, and reach.', 1, 0, 75],
      ['Archery', 'bow', 'Bows, crossbows, and aimed shots.', 1, 0, 75],
      ['Unarmed', 'fist', 'Fists, grapples, and unarmed defense.', 1, 0, 75],
      ['Blocking', 'shield', 'Shields, parries, and timed blocks.', 1, 0, 75],
      ['Armor', 'armor', 'Protection and movement in armor.', 1, 0, 75],
    ],
  },
  {
    name: 'Magic',
    skills: [
      ['Pyromancy', 'fire', 'Flame, burning, and heat.', 1, 0, 75],
      ['Cryomancy', 'frost', 'Frost, slowing, and ice.', 1, 0, 75],
      ['Stormcraft', 'storm', 'Lightning, wind, and shock.', 1, 0, 75],
      ['Restoration', 'restoration', 'Healing, cleansing, and renewal.', 1, 0, 75],
      ['Illusion', 'illusion', 'Concealment and altered perception.', 1, 0, 75],
      ['Conjuration', 'conjuration', 'Summoned creatures and bound weapons.', 1, 0, 75],
      ['Alteration', 'alteration', 'Wards, transmutation, and physical change.', 1, 0, 75],
      ['Necromancy', 'necromancy', 'Spirits, decay, and the undead.', 1, 0, 75],
    ],
  },
  {
    name: 'Life',
    skills: [
      ['Woodworking', 'woodworking', 'Bows, shields, furniture, and timberwork.', 1, 0, 75],
      ['Woodcutting', 'woodcutting', 'Felling trees and gathering timber.', 1, 0, 75],
      ['Mining', 'mining', 'Ore, gems, and stone.', 1, 0, 75],
      ['Smithing', 'smithing', 'Forging and repairing metalwork.', 1, 0, 75],
      ['Foraging', 'foraging', 'Herbs, mushrooms, and wild ingredients.', 1, 0, 75],
      ['Alchemy', 'alchemy', 'Potions, poisons, and reagents.', 1, 0, 75],
      ['Cooking', 'cooking', 'Meals, provisions, and preserving food.', 1, 0, 75],
      ['Fishing', 'fishing', 'Fish from rivers, lakes, and the sea.', 1, 0, 75],
      ['Tailoring', 'tailoring', 'Cloth, robes, and woven equipment.', 1, 0, 75],
      ['Leatherworking', 'leatherworking', 'Hides, light armor, and packs.', 1, 0, 75],
    ],
  },
];
