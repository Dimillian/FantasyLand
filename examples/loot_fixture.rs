//! Reproducible browser QA save: a cleared crypt with untouched, normally rolled corpse loot.
use fantasy_land::{
    encounters, loot,
    savegame::{PlayerState, Snapshot},
    world::World,
    worldgen::WorldDescriptor,
};
fn main() {
    let seed = 98765;
    let w = World::new(seed);
    let (p, _) = w.spawn_view();
    let sites = encounters::locations(&w, p[0], p[1], 10000.);
    let e = sites
        .iter()
        .find(|e| {
            e.kind == 0 && e.name.contains("Crypt") && loot::roll(seed, &e.id, e.kind).len() > 1
        })
        .expect("test seed has a crypt");
    let defeated = encounters::locations(&w, e.x, e.z, 250.)
        .into_iter()
        .map(|e| e.id)
        .collect();
    let corpse = loot::Corpse {
        id: e.id.clone(),
        kind: e.kind,
        position: [e.x, e.y, e.z],
        items: loot::roll(seed, &e.id, e.kind),
    };
    let snapshot = Snapshot {
        schema: 3,
        world: WorldDescriptor::current(seed),
        player: PlayerState {
            x: e.x,
            z: e.z + 2.,
            yaw: 0.,
            pitch: -0.35,
            health: 100.,
            mana: 100.,
            stamina: 100.,
            walked: 0.,
        },
        clock: 1200.,
        character: Default::default(),
        defeated,
        corpses: vec![corpse],
        doors: vec![],
    };
    snapshot.validate(snapshot.world).unwrap();
    let path = std::env::args()
        .nth(1)
        .unwrap_or("output/combat/loot-fixture.json".into());
    std::fs::write(
        &path,
        serde_json::to_string_pretty(
            &serde_json::json!({"schema":1,"snapshot":snapshot,"atlas":null,"waypoint":null}),
        )
        .unwrap(),
    )
    .unwrap();
    println!("{} at {:.1},{:.1}: {}", e.name, e.x, e.z, path);
}
