use loading_bay_game::{encode_game_snapshot, GameRuntime};
use rusty_engine::core_ids::EntityId;

const PROJECT: &str = include_str!("../../../../../content/projects/loading-bay.project.json");

fn main() {
    let mut runtime =
        GameRuntime::from_stored_project(PROJECT).expect("admit stored encounter project");
    let first = runtime
        .defeat_enemy(EntityId::new(1), EntityId::new(4))
        .expect("defeat first enemy");
    let second = runtime
        .defeat_enemy(EntityId::new(1), EntityId::new(5))
        .expect("defeat second enemy");

    println!(
        "first_events={} clearing_events={} final_revision={}",
        first.events.len(),
        second.events.len(),
        runtime.session().entities().revision()
    );
    println!(
        "{}",
        encode_game_snapshot(&runtime).expect("encode final snapshot")
    );
}
