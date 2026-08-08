use loading_bay_game::{
    decode_game_snapshot, decode_project_document, encode_game_snapshot, encode_project_document,
    ExtractionBeaconConfig, ExtractionBeaconFact, ExtractionBeaconState, GameEntityDefinition,
    GameRuntime, GameSession, RuntimeError,
};
use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::entity_state::EntityDefinition;

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const ACTOR: EntityId = EntityId::new(1);
const BEACON: EntityId = EntityId::new(7);

#[test]
fn malformed_beacon_configuration_fails_at_its_authored_path() {
    let malformed = PROJECT.replace("\"activationRadius\": 3", "\"activationRadius\": -1");

    let error = GameRuntime::from_stored_project(&malformed).unwrap_err();
    let RuntimeError::StoredProject(error) = error else {
        panic!("expected stored-project admission error");
    };
    assert_eq!(error.diagnostic().code, "project.invalidComponent");
    assert_eq!(
        error.diagnostic().path,
        "scenes[0].entities[6].extractionBeacon"
    );
}

#[test]
fn named_service_returns_a_typed_fact_and_duplicate_activation_is_atomic() {
    let mut runtime = beacon_runtime(Vec3::new(0.5, 0.0, 0.0));

    let receipt = runtime.activate_extraction_beacon(ACTOR, BEACON).unwrap();
    assert_eq!(
        receipt.fact,
        ExtractionBeaconFact::Activated {
            beacon: BEACON,
            actor: ACTOR,
            tick: runtime.tick(),
        }
    );
    assert!(matches!(
        runtime.session().extraction_beacon(BEACON).unwrap().state,
        ExtractionBeaconState::Active { actor: ACTOR, .. }
    ));

    let accepted = runtime.snapshot();
    assert!(matches!(
        runtime.activate_extraction_beacon(ACTOR, BEACON),
        Err(RuntimeError::ExtractionBeaconAlreadyActive { beacon: BEACON })
    ));
    assert_eq!(runtime.snapshot(), accepted);
}

#[test]
fn out_of_range_activation_rejects_without_mutation() {
    let mut runtime = beacon_runtime(Vec3::new(20.0, 0.0, 0.0));
    let before = runtime.snapshot();

    assert!(matches!(
        runtime.activate_extraction_beacon(ACTOR, BEACON),
        Err(RuntimeError::ExtractionBeaconOutOfRange {
            actor: ACTOR,
            beacon: BEACON,
            ..
        })
    ));
    assert_eq!(runtime.snapshot(), before);
}

#[test]
fn canonical_project_and_runtime_snapshot_preserve_beacon_meaning() {
    let decoded = decode_project_document(PROJECT).unwrap();
    let canonical = encode_project_document(&decoded.project).unwrap();
    let reopened_project = decode_project_document(&canonical).unwrap();
    assert_eq!(
        encode_project_document(&reopened_project.project).unwrap(),
        canonical
    );
    let beacon = reopened_project.project.scenes[0]
        .entities
        .iter()
        .find(|entity| entity.id == BEACON.raw())
        .unwrap()
        .extraction_beacon
        .unwrap();
    assert_eq!(beacon.activation_radius, 3.0);

    let mut runtime = beacon_runtime(Vec3::new(0.5, 0.0, 0.0));
    runtime.activate_extraction_beacon(ACTOR, BEACON).unwrap();
    let encoded = encode_game_snapshot(&runtime).unwrap();
    let reopened = decode_game_snapshot(&encoded).unwrap();
    assert_eq!(reopened.snapshot(), runtime.snapshot());
    assert!(matches!(
        reopened.session().extraction_beacon(BEACON).unwrap().state,
        ExtractionBeaconState::Active { actor: ACTOR, .. }
    ));
}

fn beacon_runtime(actor_translation: Vec3) -> GameRuntime {
    let session = GameSession::from_definitions([
        GameEntityDefinition::new(
            EntityDefinition::new(ACTOR, "actor").with_transform(actor_translation),
        ),
        GameEntityDefinition::new(
            EntityDefinition::new(BEACON, "extraction-beacon")
                .with_transform(Vec3::ZERO)
                .with_renderable("mesh/extraction-beacon", true),
        )
        .with_extraction_beacon(ExtractionBeaconConfig::new(2.5)),
    ])
    .unwrap();
    GameRuntime::new(session)
}
