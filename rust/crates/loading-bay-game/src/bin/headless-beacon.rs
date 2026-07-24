use std::fs;
use std::path::{Path, PathBuf};

use core_ids::EntityId;
use loading_bay_game::{ExtractionBeaconFact, ExtractionBeaconState, GameRuntime};

const ACTOR: EntityId = EntityId::new(1);
const BEACON: EntityId = EntityId::new(7);

fn main() {
    let project = project_argument();
    let input = fs::read_to_string(&project)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", project.display()));
    let mut runtime = GameRuntime::from_stored_project(&input)
        .unwrap_or_else(|error| panic!("could not admit {}: {error}", project.display()));
    let receipt = runtime
        .activate_extraction_beacon(ACTOR, BEACON)
        .unwrap_or_else(|error| {
            panic!(
                "could not activate beacon in {}: {error}",
                project.display()
            )
        });
    assert!(matches!(
        receipt.fact,
        ExtractionBeaconFact::Activated {
            beacon: BEACON,
            actor: ACTOR,
            ..
        }
    ));
    assert!(matches!(
        runtime.session().extraction_beacon(BEACON).unwrap().state,
        ExtractionBeaconState::Active { actor: ACTOR, .. }
    ));
    println!(
        "extraction beacon activated project={} actor={} beacon={}",
        project.display(),
        ACTOR.raw(),
        BEACON.raw()
    );
}

fn project_argument() -> PathBuf {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        None => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../content/projects/loading-bay.project.json"),
        Some("--project") => {
            let path = PathBuf::from(arguments.next().expect("--project needs a path"));
            assert!(arguments.next().is_none(), "unexpected trailing argument");
            path
        }
        Some(argument) => panic!("unknown headless-beacon argument {argument}"),
    }
}
