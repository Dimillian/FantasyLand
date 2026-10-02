// Illustrative UI values only. Replace this catalogue's progress with character data
// when gameplay progression exists; preview XP must never enter the world save.
// Skill entries: [name, icon, description, level, current XP, next-level XP].
export const skillGroups = [
  {
    name: 'Combat',
    skills: [
      ['Blades', 'blade', 'Swords, daggers, and precision strikes.', 7, 135, 240],
      ['Axes', 'axe', 'Axes and cleaving attacks.', 3, 46, 160],
      ['Blunt weapons', 'mace', 'Maces, hammers, and crushing attacks.', 2, 35, 140],
      ['Polearms', 'spear', 'Spears, staves, and reach.', 4, 95, 180],
      ['Archery', 'bow', 'Bows, crossbows, and aimed shots.', 5, 112, 200],
      ['Unarmed', 'fist', 'Fists, grapples, and unarmed defense.', 1, 12, 120],
      ['Blocking', 'shield', 'Shields, parries, and timed blocks.', 6, 155, 220],
      ['Armor', 'armor', 'Protection and movement in armor.', 4, 71, 180],
    ],
  },
  {
    name: 'Magic',
    skills: [
      ['Pyromancy', 'fire', 'Flame, burning, and heat.', 5, 125, 200],
      ['Cryomancy', 'frost', 'Frost, slowing, and ice.', 3, 82, 160],
      ['Stormcraft', 'storm', 'Lightning, wind, and shock.', 2, 55, 140],
      ['Restoration', 'restoration', 'Healing, cleansing, and renewal.', 6, 168, 220],
      ['Illusion', 'illusion', 'Concealment and altered perception.', 2, 24, 140],
      ['Conjuration', 'conjuration', 'Summoned creatures and bound weapons.', 1, 18, 120],
      ['Alteration', 'alteration', 'Wards, transmutation, and physical change.', 4, 114, 180],
      ['Necromancy', 'necromancy', 'Spirits, decay, and the undead.', 1, 0, 120],
    ],
  },
  {
    name: 'Life',
    skills: [
      ['Woodworking', 'woodworking', 'Bows, shields, furniture, and timberwork.', 4, 106, 180],
      ['Woodcutting', 'woodcutting', 'Felling trees and gathering timber.', 5, 142, 200],
      ['Mining', 'mining', 'Ore, gems, and stone.', 3, 98, 160],
      ['Smithing', 'smithing', 'Forging and repairing metalwork.', 2, 48, 140],
      ['Foraging', 'foraging', 'Herbs, mushrooms, and wild ingredients.', 8, 172, 260],
      ['Alchemy', 'alchemy', 'Potions, poisons, and reagents.', 3, 63, 160],
      ['Cooking', 'cooking', 'Meals, provisions, and preserving food.', 5, 76, 200],
      ['Fishing', 'fishing', 'Fish from rivers, lakes, and the sea.', 2, 91, 140],
      ['Tailoring', 'tailoring', 'Cloth, robes, and woven equipment.', 1, 21, 120],
      ['Leatherworking', 'leatherworking', 'Hides, light armor, and packs.', 2, 59, 140],
    ],
  },
];
