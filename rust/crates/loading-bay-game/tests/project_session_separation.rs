use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use core_ids::EntityId;
use loading_bay_game::{
    admit_stored_project_with_document, decode_game_snapshot, decode_project_document,
    diagnostic_code, encode_game_snapshot, encode_project_document, DoorState, GameRuntime,
    ItemDefinitionId, ProjectSaveMode, ProjectStore, ProjectStoreError, ResolvedAttackAction,
    ResolvedPlayerAction,
};

const CURRENT_PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const LEGACY_PROJECT: &str =
    include_str!("../../../../content/generated/encounter-gate.project.json");
const PLAYER: EntityId = EntityId::new(1);
const EXIT: EntityId = EntityId::new(12);
const ENEMY: EntityId = EntityId::new(4);
const SWITCH: EntityId = EntityId::new(6);

#[test]
fn authored_save_stays_static_while_independent_snapshot_reopens_live_values() {
    let directory = TestDirectory::new();
    let project_path = directory.path().join("loading-bay.project.json");
    let mut decoded = decode_project_document(CURRENT_PROJECT).unwrap();
    decoded.project.scenes[0]
        .entities
        .iter_mut()
        .find(|entity| entity.id == 2)
        .unwrap()
        .encounter
        .as_mut()
        .unwrap()
        .activation_radius = None;
    decoded.project.scenes[0]
        .entities
        .iter_mut()
        .find(|entity| entity.id == 30)
        .unwrap()
        .collision
        .as_mut()
        .unwrap()
        .enabled = false;
    let (authored, admitted) = admit_stored_project_with_document(decoded.project).unwrap();
    let mut runtime = GameRuntime::from_admitted_project(admitted);

    aim_at(&mut runtime, ENEMY);
    runtime
        .attack(PLAYER, ResolvedAttackAction::Attack)
        .unwrap();
    runtime.advance_by(1).unwrap();
    runtime.interact(PLAYER, SWITCH).unwrap();

    assert_eq!(runtime.tick().raw(), 1);
    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 40);
    assert_eq!(inventory_quantity(&runtime, "ammo/energy-cell"), 17);
    assert_eq!(
        runtime
            .session()
            .weapon(PLAYER)
            .unwrap()
            .state
            .ready_at_tick
            .raw(),
        2
    );
    assert_eq!(runtime.session().door(EXIT).unwrap().state, DoorState::Open);
    assert_ne!(
        runtime
            .session()
            .player_controller(PLAYER)
            .unwrap()
            .state
            .yaw_degrees,
        0.0
    );

    let snapshot = encode_game_snapshot(&runtime).unwrap();
    ProjectStore::default()
        .save(&project_path, &authored, ProjectSaveMode::CreateNew)
        .unwrap();
    let saved_project = fs::read_to_string(&project_path).unwrap();
    let project_value: serde_json::Value = serde_json::from_str(&saved_project).unwrap();
    for live_key in [
        "tick",
        "current",
        "ammoRemaining",
        "readyAtTick",
        "yawDegrees",
        "pitchDegrees",
        "scheduled",
        "events",
        "journal",
    ] {
        assert!(
            !contains_object_key(&project_value, live_key),
            "authored save leaked {live_key}"
        );
    }

    let loaded = ProjectStore::default().load(&project_path).unwrap();
    let (_, initial) = admit_stored_project_with_document(loaded.project).unwrap();
    let initial = GameRuntime::from_admitted_project(initial);
    assert_eq!(initial.tick().raw(), 0);
    assert_eq!(initial.session().health(ENEMY).unwrap().current, 100);
    assert_eq!(inventory_quantity(&initial, "ammo/energy-cell"), 18);
    assert_eq!(
        initial.session().door(EXIT).unwrap().state,
        DoorState::Closed
    );

    let reopened = decode_game_snapshot(&snapshot).unwrap();
    assert_eq!(encode_game_snapshot(&reopened).unwrap(), snapshot);
    assert_eq!(reopened.tick().raw(), 1);
    assert_eq!(reopened.session().health(ENEMY).unwrap().current, 40);
    assert_eq!(inventory_quantity(&reopened, "ammo/energy-cell"), 17);
    assert_eq!(
        reopened.session().door(EXIT).unwrap().state,
        DoorState::Open
    );
}

#[test]
fn migrated_predecessor_runs_and_future_schema_preserves_the_good_project() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let legacy_path = directory.path().join("legacy.json");
    let future_path = directory.path().join("future.json");
    let store = ProjectStore::default();

    let current = decode_project_document(CURRENT_PROJECT).unwrap();
    let (current, _) = admit_stored_project_with_document(current.project).unwrap();
    store
        .save(&target, &current, ProjectSaveMode::CreateNew)
        .unwrap();

    fs::write(&legacy_path, LEGACY_PROJECT).unwrap();
    let migrated = store.load(&legacy_path).unwrap();
    assert_eq!(migrated.source_schema_version, 6);
    let (migrated, admitted) = admit_stored_project_with_document(migrated.project).unwrap();
    let mut runtime = GameRuntime::from_admitted_project(admitted);
    runtime.run_navigation_phase(1.0 / 60.0).unwrap();
    assert_eq!(runtime.session().health(ENEMY).unwrap().current, 100);
    store
        .save(&target, &migrated, ProjectSaveMode::ReplaceExisting)
        .unwrap();
    let known_good = fs::read_to_string(&target).unwrap();

    fs::write(&future_path, "{\"schemaVersion\":99}").unwrap();
    let error = store.load(&future_path).unwrap_err();
    let ProjectStoreError::Codec(error) = error else {
        panic!("unexpected future-version error: {error}");
    };
    assert_eq!(error.diagnostic().code, diagnostic_code::UNSUPPORTED_SCHEMA);
    assert_eq!(error.diagnostic().path, "schemaVersion");
    assert_eq!(fs::read_to_string(&target).unwrap(), known_good);
    assert_eq!(
        encode_project_document(&store.load(&target).unwrap().project).unwrap(),
        known_good
    );
}

fn aim_at(runtime: &mut GameRuntime, target: EntityId) {
    let player = runtime
        .session()
        .entity(PLAYER)
        .unwrap()
        .transform
        .unwrap()
        .translation;
    let target = runtime
        .session()
        .entity(target)
        .unwrap()
        .transform
        .unwrap()
        .translation;
    let offset_x = target.x - player.x;
    let offset_y = target.y - player.y;
    let offset_z = target.z - player.z;
    let desired_yaw = normalize_degrees((-offset_x).atan2(-offset_z).to_degrees());
    let desired_pitch = offset_y
        .atan2((offset_x * offset_x + offset_z * offset_z).sqrt())
        .to_degrees();

    for _ in 0..40 {
        let controller = runtime.session().player_controller(PLAYER).unwrap();
        let yaw_difference = normalize_degrees(desired_yaw - controller.state.yaw_degrees);
        let pitch_difference = desired_pitch - controller.state.pitch_degrees;
        if yaw_difference.abs() < 0.01 && pitch_difference.abs() < 0.01 {
            return;
        }
        runtime
            .apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Look {
                    yaw_delta: (yaw_difference / controller.config.look_degrees_per_unit)
                        .clamp(-1.0, 1.0),
                    pitch_delta: (pitch_difference / controller.config.look_degrees_per_unit)
                        .clamp(-1.0, 1.0),
                },
            )
            .unwrap();
    }
    panic!("could not aim at target");
}

fn normalize_degrees(value: f32) -> f32 {
    (value + 180.0).rem_euclid(360.0) - 180.0
}

fn inventory_quantity(runtime: &GameRuntime, item: &str) -> u32 {
    let item = ItemDefinitionId::parse(item).unwrap();
    runtime
        .session()
        .inventory(PLAYER)
        .unwrap()
        .stacks
        .into_iter()
        .find(|stack| stack.item == item)
        .map_or(0, |stack| stack.quantity)
}

fn contains_object_key(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .any(|value| contains_object_key(value, needle)),
        serde_json::Value::Object(values) => {
            values.contains_key(needle)
                || values
                    .values()
                    .any(|value| contains_object_key(value, needle))
        }
        _ => false,
    }
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rusty-engine-project-session-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
