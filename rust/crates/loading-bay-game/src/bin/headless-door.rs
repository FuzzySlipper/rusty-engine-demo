use loading_bay_game::{encode_game_snapshot, GameRuntime};
use rusty_engine::core_time::TickDelta;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (ids, mut runtime) = GameRuntime::security_door(Some(TickDelta::new(3)))?;
    let opened = runtime.interact(ids.actor, ids.switch)?;
    let closed = runtime.advance_by(3)?;
    println!(
        "opened_events={} closed_events={} final_tick={} final_revision={}",
        opened.events.len(),
        closed.events.len(),
        runtime.tick().raw(),
        runtime.session().entities().revision()
    );
    println!("{}", encode_game_snapshot(&runtime)?);
    Ok(())
}
