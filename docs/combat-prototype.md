# Sword and shield prototype

An optional sparring trial remains available alongside persistent world encounters. Press **F6** or use **Developer tools → Sparring trial** to place one opponent in a nearby dry, unobstructed lane. F6 leaves and restores the original player position, orientation, health and stamina; **T** restarts the trial. Saves/export capture the pre-trial player, so the experiment does not leave permanent combat damage. World geography and civilians are unchanged.

**R** draws/sheathes the weapon anywhere. **Left mouse / F** swings; **hold right mouse / V** guards. Click once to capture the mouse before mouse combat. Keyboard controls work without pointer lock. Escape and menus clear held combat inputs. Enemy simulation continues when the player sheathes. Weapon use does not require an active opponent.

## Rules and engine boundary

`src/combat.rs` owns reach, aim, line-of-sight checks, guard direction, stamina, phase timing, damage, results, events and a bounded local navigation search. A swing costs 16 stamina and strikes at 280 ms of a 580 ms animation. Hits deal 25 damage, or 34 during the opponent's recovery. The enemy telegraphs for 680 ms, strikes, then recovers for 850 ms. Guarding a frontal hit costs 20 stamina; an exhausted guard breaks. Wind-ups have poise to prevent continuous stun-lock. Body overlap is rejected. Attacks and path edges consult the same current world collision and walk-height queries as movement, including doors, structures, props, water and slope limits.

The local eight-neighbour path search is limited to 384 expansions, a 24-cell radius, and a 650 ms replanning cadence. It checks swept edges so diagonals do not cut solid corners. This is a prototype for one opponent, not a crowd navmesh. Visibility is refreshed every 150 ms; occluded opponents don't display a through-wall health bar.

The opponent adds one two-triangle billboard to the existing reusable dynamic mesh, with an original skeleton drawn at 96×192 pixels. Twenty-four RG8 layers each pack four directions, covering an eight-frame walk, wind-up, strike, recovery, recoil and collapse. This adds about 3.2 MiB over the original one-layer opponent without adding a render pass. Citizen sprite animation is untouched. On death the sprite collapses; there is no ragdoll. AI currently has one melee attack per creature and no hostility factions. The persistent RPG layer is described below.

## First-person procedural art

Equipment definitions, geometry, hand rigs, baking and display are separate modules (see the extension guide below). The rendered view shows the wooden **inside** of the shield, its reinforcing straps, rivets and handle; a closed gloved hand wraps the sword grip. A tapered arm extends below the view in every pose. The sword has a diamond cross section, fuller, guard and pommel. Iron, leather and wood use dark palettes and stable object-space wear. Sword, shield, axe and mace recipe silhouettes exist, but only sword-and-shield gameplay is wired in this slice.

Perspective poses bake in an OffscreenCanvas worker into pixel sprites at 420 px height, with a bounded 24 MiB cache. A depth buffer resolves fingers, grip, straps and arm occlusion. Steel scratches, wood fibres and glove grain are sampled in equipment coordinates so they travel with the swing. The four fingers curl around the vertical grip at separate heights; the thumb crosses the index finger. The relaxed shield sits largely below the frame, while guarding raises the inside face. Its recipe uses `showHand: false`: the shield keeps its wooden back, grip and straps, with no visible hand or forearm. Other equipment retains its hand rig. Only cached images are composited on ordinary frames; one bake runs at a time and the nearest cached pose covers initial warm-up. Browsers without worker canvas support use the synchronous cached fallback. The viewmodel responds approximately to time of day and cloud cover; it is not yet lit by per-pixel world lights or the full wgpu material pipeline. 32 swing samples, 12 guard samples and an eased draw/sheath offset give distinct preparation, strike, follow-through and recovery without detached arms. `combat-art.html` is a GPU-free pose contact sheet.

Visual study: [Oblivion shield interior reference](https://static.deltiasgaming.com/2025/04/shield-oblivion-remastered.jpg), [Daggerfall first-person screenshot reference](https://www.gamedeveloper.com/design/role-model-emphasizing-the-rp-in-rpg), and [Skyrim sword/shield exploration reference](https://ocean-of-games.com/game/472/the-elder-scrolls-v-skyrim). Reference assets are not shipped. All equipment, enemy plates and sounds are original procedural outputs.

`dist/combat-view.mjs` projects the enemy health bar and floating damage at world positions, adds contact marks and restrained red damage edges, and consumes discrete engine events. Impact briefly holds the swing for 50 ms, pushes the opponent back along collision-checked ground, flashes the full sprite and emits world-positioned contact fragments. The health bar clamps below the compass at close range and retains a trailing damage segment; damage numbers stay inside the viewport. Twelve generated swing, impact, guard and hurt samples use the existing bounded audio voice pool and a separate effects volume. The foley pass separates filtered blade air/sleeve friction, low body contacts and damped wood/metal guard resonances; it removes the original pitched sweeps and sustained sine tones. Offline PCM generation applies DC removal, smooth boundaries and per-family peak targets with quieter swings. `combat-art.html` includes sound audition controls.

Control hints and shortcut banners are confined to menus, not the playing view. Health bars, damage numbers, the compass and clock remain.

## Validation

Run `cargo test --lib combat::tests`, `node scripts/verify-combat-view.mjs`, `node scripts/verify-combat-audio.mjs`, `node scripts/verify-ui.cjs`, and `node scripts/verify-audio.mjs`. Tests exercise wall obstruction, facing, single-hit timing, guard break, enemy poise, pause/results, collision-safe path detours, free weapon use, sheathing, menu/input isolation, deterministic equipment and camera projection. Browser verification checks the actual WASM/WebGPU build and the weapon poses; performance is not a hardware-wide 60 FPS guarantee.

Additional pose study: [Skyrim grip and resting shield screenshot](https://www.notebookcheck.net/fileadmin/_migrated/pics/1366high_01.jpg) and [Daggerfall lower-right fist/sword screenshot](https://fsmedia.imgix.net/95/d9/5c/57/5827/4241/aa31/f473e96247f6/elder-scrolls-daggerfall.jpeg?auto=format%2Ccompress&dpr=2&rect=322%2C0%2C958%2C720&w=958). These are study references, not shipped art.

## Extending equipment art

- `equipment-items.mjs`: `registerEquipment(id, builder)` and `equipmentRecipe(seed, id, options)` produce serializable item geometry, palette, a grip anchor and rig choice. Unknown IDs fail explicitly. Sword, shield, axe, mace and a lantern demonstrate the same contract. The lantern is an art example, not a working dynamic light or inventory item.
- `equipment-geometry.mjs`: shared face, box and tapered tube builders. Item builders operate in local coordinates (+Y along the handle, +Z away from the eye), independently of camera or combat state. Custom faces support shapes beyond the primitive examples.
- `equipment-rig.mjs`: poses, articulated fingers, wrist/forearm connections and mirroring. Existing rigs cover one-handed items and shields; a two-handed or bow rig would be a new rig implementation. A custom grip supplies `position` and `wrist` anchors in item space.
- `equipment-sprites.mjs`: depth-tested texture rasterization and cropping only. The shield has a complete board behind the rim, a sealed thickness, and an inner metal lip; it does not rely on separately projected edges meeting by chance.
- `equipment-cache.mjs` / `equipment-worker.mjs`: accept the complete recipe rather than reconstructing a hardcoded sword/shield in the worker. Recipe content fingerprints, hand, pose, resolution and light form the cache key. The 24 MiB limit closes evicted bitmaps. A generation counter rejects stale results after equipment swaps. `dispose()` releases the worker and bitmaps.

`CombatView.setEquipment({mainHand: recipe, offHand: recipeOrNull})` changes the visual loadout. Gameplay damage and attack rules remain the sword/shield prototype; this API is the rendering boundary for future inventory integration. Recipe builders are invoked once when an item is created, not every animation frame.

Example (a new mesh can use any palette or primitive combination):

```js
registerEquipment('relic', ({r, box, limb}) => {
  limb([0,-.08,0], [0,.1,0], .024, .024, r.leather);
  box([0,.24,0], [.11,.25,.06], r.trim);
});
combatView.setEquipment({mainHand:equipmentRecipe(12,'relic'),offHand:null});
```

The reference contact sheet includes axe, mace and lantern art in both hand positions. Tests also register a new recipe, serialize it, send it through the same worker queue, change hands and discard an obsolete in-flight result. Shield perimeter coverage is checked at rest and four intermediate/raised guard poses.

## First persistent RPG slice

The current generator is version 3 and the save schema is 3. Earlier saves are incompatible and start fresh; there is no migration. `progression::Character` is the authoritative Rust state, with five attributes, two equipped-item flags and Blades/Blocking skills. The starting inventory owns an iron arming sword and an oak shield. Items use validated stacks with authoritative weights; one copy of each starting gear type can be equipped. Durability and arbitrary equipment variants remain future work.

Strength contributes `Strength / 3` to the sword's 21 base damage; Agility reduces its 18 base stamina cost by `Agility * .2` (minimum 8). Endurance sets health/stamina capacity (`60 + 4 * Endurance`), Intellect sets mana capacity, and Willpower sets stamina recovery (`12 + .2 * Willpower`). The shield grants 2 flat protection and blocks a frontal attack for 20 stamina. A stowed sword cannot attack, and a stowed shield cannot block. Equipment changes are rejected during an unresolved encounter or swing.

Successful sword hits award 12 Blades XP; successful defended strikes award 16 Blocking XP. Misses, swings into walls, failed guards and corpses grant none. Each rank requires `50 + rank * 25` XP, carries surplus into the next rank, and caps at 100. No rank rewards or skill damage bonuses are implemented. Sparring also trains these skills, while restoring its original position/vitals on exit. Character state and cleared encounters persist in local saves and exports. Notifications coalesce per skill, show an XP meter, and emphasize rank-ups; the Skills tab reads the same state. Other skills are explicitly untrained.

`encounters.rs` places two or three guards at dedicated hostile landmarks. `hostile.rs` generates marauder camps, cemeteries, haunted groves, three-chamber crypts and ruined keeps. Placement rejects wet/steep footprints and settlement surroundings; each landmark and guard has a seed-stable identity. Indoor sites are walk-in surface structures with a southern stair entrance, offset connecting doors, stone ceilings/skylights, sarcophagi and fire emitters. They are not underground caves or multi-level dungeons yet. Geometry, furniture collision and floor/entrance height come from one bounded cached recipe; vegetation is excluded from footprints.

Only one nearby guard simulates at a time. Sites therefore run sequential guards, not simultaneous packs: a defeated guard becomes a persistent corpse, then the next eligible guard activates after a short delay. The 128 m discovery cache, 65 m activation radius, 95 m unloading range and bounded navigation remain. Living guards reset on unload. Defeat returns to the starting road without an item/XP penalty.

Deaths create deterministic loot on a corpse without touching the player's inventory. Aim at the body within 3.4 m and press E: a keyboard/mouse loot dialog allows taking an individual stack or everything. Loot cannot be taken through walls or during unresolved combat. Partial looting, empty bodies, defeated IDs and owned stacks persist atomically in the current save. Bodies are rendered through the existing billboard batch, capped at 24 nearby bodies. `examples/loot_fixture.rs` produces a separate-seed, cleared-crypt save for repeatable browser QA through the normal Import save UI.

`loot.rs` owns definitions and tables: 1–3 bones/fangs/ectoplasm, a species-specific essence chance (45% / 30% / 80%), and a 12% equipment chance for physical enemies. Essences are reserved for future enchantment; materials have no usable/crafting behavior yet. Inventory filters All / Equipment / Monster parts / Essences, shows stack quantity and total carried weight, and keeps equipped items in the square grid. Taking a stack is transactional and duplicate takes do nothing. Weight is informational; there is no encumbrance penalty yet.

All three foes use 24 procedural 96×192 frames in four directions. The shared RG8 atlas has 136 layers (~19.1 MiB total including citizens), and the enemy remains one two-triangle billboard. Goblins have 72 health, skeletons 100, ghosts 85; their speed and attack damage differ. Ghosts use emissive eyes/cloth and a hovering silhouette rather than sorted transparency. `cargo run --example skeleton_art` exports a three-species contact sheet. Developer tools can select a sparring species or list/visit uncleared nearby monster haunts.
