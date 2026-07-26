use std::fs;
use std::path::PathBuf;

use core_ids::EntityId;
use loading_bay_game::{
    decode_game_snapshot, decode_project_document, encode_game_snapshot, GameRuntime,
    InventoryAction, InventoryCommand, ItemDefinitionId, ResolvedPlayerAction, SaveGameStore,
    SaveLoadRequest, SaveProjectIdentity, SaveSlotId, SaveWriteRequest, VoxelEdit,
    VoxelEditTransaction, VoxelSourceRevision,
};

const PROJECT: &str = include_str!("../../../../content/projects/loading-bay.project.json");
const PLAYER: EntityId = EntityId::new(1);

#[test]
fn multiple_mid_level_slots_restore_exact_state_and_eventual_behavior() {
    let root = temporary_root("mid-level");
    let store = SaveGameStore::new(&root);
    let project = decode_project_document(PROJECT).unwrap().project;
    let identity = SaveProjectIdentity::from_project(&project, PLAYER).unwrap();
    let mut runtime = GameRuntime::from_stored_project(PROJECT).unwrap();

    runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Look {
                yaw_delta: 0.75,
                pitch_delta: -0.25,
            },
        )
        .unwrap();
    runtime
        .apply_inventory_command(
            PLAYER,
            InventoryCommand {
                sequence: 1,
                action: InventoryAction::Consume {
                    item: ItemDefinitionId::parse("ammo/energy-cell").unwrap(),
                    quantity: 3,
                },
            },
        )
        .unwrap();
    runtime.advance_by(7).unwrap();
    let first_snapshot = encode_game_snapshot(&runtime).unwrap();
    let first = store
        .save(
            &identity,
            SaveWriteRequest {
                slot: SaveSlotId::Slot1,
                overwrite: false,
                expected_storage_revision: None,
                saved_at_unix_milliseconds: 100,
            },
            &runtime,
        )
        .unwrap();

    runtime
        .apply_voxel_edits(VoxelEditTransaction {
            expected_revision: VoxelSourceRevision::INITIAL,
            edits: &[VoxelEdit::Clear { address: [4, 1, 6] }],
        })
        .unwrap();
    runtime
        .apply_player_action(
            PLAYER,
            ResolvedPlayerAction::Move {
                forward: -1.0,
                right: 0.5,
            },
        )
        .unwrap();
    runtime.advance_by(11).unwrap();
    let second_snapshot = encode_game_snapshot(&runtime).unwrap();
    let second = store
        .save(
            &identity,
            SaveWriteRequest {
                slot: SaveSlotId::Slot2,
                overwrite: false,
                expected_storage_revision: None,
                saved_at_unix_milliseconds: 200,
            },
            &runtime,
        )
        .unwrap();

    let first_loaded = store
        .load(
            &identity,
            SaveLoadRequest {
                slot: SaveSlotId::Slot1,
                expected_storage_revision: first.storage_revision,
            },
        )
        .unwrap();
    assert_eq!(
        encode_game_snapshot(&first_loaded.runtime).unwrap(),
        first_snapshot
    );
    assert_eq!(
        first_loaded
            .runtime
            .collision_scene()
            .unwrap()
            .source_revision(),
        VoxelSourceRevision::INITIAL
    );

    let second_loaded = store
        .load(
            &identity,
            SaveLoadRequest {
                slot: SaveSlotId::Slot2,
                expected_storage_revision: second.storage_revision,
            },
        )
        .unwrap();
    assert_eq!(
        encode_game_snapshot(&second_loaded.runtime).unwrap(),
        second_snapshot
    );
    assert_eq!(
        second_loaded
            .runtime
            .collision_scene()
            .unwrap()
            .source_revision()
            .raw(),
        1
    );

    let mut expected = decode_game_snapshot(&second_snapshot).unwrap();
    let mut actual = second_loaded.runtime;
    let action = ResolvedPlayerAction::Move {
        forward: 1.0,
        right: -0.5,
    };
    assert_eq!(
        actual.apply_player_action(PLAYER, action).unwrap(),
        expected.apply_player_action(PLAYER, action).unwrap()
    );
    assert_eq!(
        encode_game_snapshot(&actual).unwrap(),
        encode_game_snapshot(&expected).unwrap()
    );

    fs::remove_dir_all(root).unwrap();
}

fn temporary_root(label: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "loading-bay-save-runtime-{label}-{}-{unique}",
        std::process::id()
    ))
}
