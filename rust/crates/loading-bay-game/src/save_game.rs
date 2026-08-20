//! Versioned, bounded runtime-save storage for the Loading Bay game.
//!
//! Save documents contain one validated [`GameSnapshot`] and an exact identity
//! for the authored project that produced it. They never contain authored
//! project bytes, input-session state, or presentation state.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusty_engine::core_ids::EntityId;
use serde::{Deserialize, Serialize};

use crate::{
    encode_project_document, GameRuntime, GameSnapshot, GameSnapshotError, StoredProject,
    VitalityState, GAME_SNAPSHOT_SCHEMA_VERSION,
};

pub const SAVE_GAME_SCHEMA_VERSION: u32 = 1;
// E1M1's admitted voxel snapshot is roughly 7 MiB.  Keep a bounded ceiling
// above the supported product rather than rejecting legitimate saves.
pub const MAX_SAVE_GAME_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SAVE_SLOTS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveSlotId {
    Checkpoint,
    Slot1,
    Slot2,
    Slot3,
}

impl SaveSlotId {
    pub const ALL: [Self; MAX_SAVE_SLOTS] =
        [Self::Checkpoint, Self::Slot1, Self::Slot2, Self::Slot3];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Checkpoint => "Checkpoint",
            Self::Slot1 => "Manual save 1",
            Self::Slot2 => "Manual save 2",
            Self::Slot3 => "Manual save 3",
        }
    }

    const fn file_stem(self) -> &'static str {
        match self {
            Self::Checkpoint => "checkpoint",
            Self::Slot1 => "slot-1",
            Self::Slot2 => "slot-2",
            Self::Slot3 => "slot-3",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveProjectIdentity {
    pub project_id: String,
    pub entry_scene: String,
    pub player_entity: u64,
    pub project_schema_version: u32,
    pub content_revision: String,
}

impl SaveProjectIdentity {
    pub fn from_project(project: &StoredProject, player: EntityId) -> Result<Self, SaveGameError> {
        let canonical = encode_project_document(project)
            .map_err(|error| SaveGameError::Encode(error.to_string()))?;
        Ok(Self {
            project_id: project.project_id.clone(),
            entry_scene: project.entry_scene.clone(),
            player_entity: player.raw(),
            project_schema_version: project.schema_version,
            content_revision: content_revision(canonical.as_bytes()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveGameMetadata {
    pub revision: u64,
    pub saved_at_unix_milliseconds: u64,
    pub display_name: String,
    pub tick: u64,
    pub snapshot_schema_version: u32,
    pub player_state: SavePlayerState,
    pub level_complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SavePlayerState {
    Alive,
    Dead,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveSlotCompatibility {
    Empty,
    Available,
    Corrupt,
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSlotSummary {
    pub slot: SaveSlotId,
    pub compatibility: SaveSlotCompatibility,
    pub storage_revision: Option<String>,
    pub metadata: Option<SaveGameMetadata>,
    pub project: Option<SaveProjectIdentity>,
    pub diagnostic: Option<String>,
}

impl SaveSlotSummary {
    fn empty(slot: SaveSlotId) -> Self {
        Self {
            slot,
            compatibility: SaveSlotCompatibility::Empty,
            storage_revision: None,
            metadata: None,
            project: None,
            diagnostic: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveWriteRequest {
    pub slot: SaveSlotId,
    pub overwrite: bool,
    pub expected_storage_revision: Option<String>,
    pub saved_at_unix_milliseconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveLoadRequest {
    pub slot: SaveSlotId,
    pub expected_storage_revision: Option<String>,
}

#[derive(Debug)]
pub struct LoadedSaveGame {
    pub runtime: GameRuntime,
    pub summary: SaveSlotSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveGameError {
    Empty {
        slot: SaveSlotId,
    },
    OverwriteRequired {
        slot: SaveSlotId,
    },
    Stale {
        slot: SaveSlotId,
        expected: Option<String>,
        actual: Option<String>,
    },
    Corrupt {
        slot: SaveSlotId,
        message: String,
    },
    Incompatible {
        slot: SaveSlotId,
        message: String,
    },
    TooLarge {
        slot: SaveSlotId,
        actual: u64,
        limit: usize,
    },
    Io {
        operation: &'static str,
        message: String,
    },
    Encode(String),
}

impl std::fmt::Display for SaveGameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty { slot } => write!(formatter, "{} is empty", slot.display_name()),
            Self::OverwriteRequired { slot } => {
                write!(
                    formatter,
                    "{} requires explicit overwrite",
                    slot.display_name()
                )
            }
            Self::Stale {
                slot,
                expected,
                actual,
            } => write!(
                formatter,
                "{} changed since it was displayed (expected {expected:?}, actual {actual:?})",
                slot.display_name()
            ),
            Self::Corrupt { slot, message } => {
                write!(formatter, "{} is corrupt: {message}", slot.display_name())
            }
            Self::Incompatible { slot, message } => {
                write!(
                    formatter,
                    "{} is incompatible: {message}",
                    slot.display_name()
                )
            }
            Self::TooLarge {
                slot,
                actual,
                limit,
            } => write!(
                formatter,
                "{} is too large ({actual} bytes; limit {limit})",
                slot.display_name()
            ),
            Self::Io { operation, message } => {
                write!(formatter, "save storage {operation} failed: {message}")
            }
            Self::Encode(message) => write!(formatter, "save encoding failed: {message}"),
        }
    }
}

impl std::error::Error for SaveGameError {}

#[derive(Debug, Clone)]
pub struct SaveGameStore {
    root: PathBuf,
    max_bytes: usize,
}

impl SaveGameStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_bytes: MAX_SAVE_GAME_BYTES,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn inspect_all(&self, identity: &SaveProjectIdentity) -> Vec<SaveSlotSummary> {
        SaveSlotId::ALL
            .into_iter()
            .map(|slot| self.inspect(slot, identity))
            .collect()
    }

    pub fn inspect(&self, slot: SaveSlotId, identity: &SaveProjectIdentity) -> SaveSlotSummary {
        let path = self.slot_path(slot);
        let (bytes, storage_revision) = match self.read_slot_bytes(slot, &path) {
            Ok(Some(value)) => value,
            Ok(None) => return SaveSlotSummary::empty(slot),
            Err(error) => {
                return SaveSlotSummary {
                    slot,
                    compatibility: SaveSlotCompatibility::Corrupt,
                    storage_revision: storage_revision_for_path(&path, self.max_bytes),
                    metadata: None,
                    project: None,
                    diagnostic: Some(error.to_string()),
                };
            }
        };
        match self.decode_and_validate(slot, bytes, storage_revision.clone(), identity) {
            Ok((_, summary)) => summary,
            Err(error) => summary_from_error(slot, storage_revision, error),
        }
    }

    pub fn save(
        &self,
        identity: &SaveProjectIdentity,
        request: SaveWriteRequest,
        runtime: &GameRuntime,
    ) -> Result<SaveSlotSummary, SaveGameError> {
        let _slot_lock = self.lock_slot(request.slot)?;
        let path = self.slot_path(request.slot);
        let current = observe_storage_revision(&path, self.max_bytes)?;
        if current.is_some() && !request.overwrite {
            return Err(SaveGameError::OverwriteRequired { slot: request.slot });
        }
        if request.expected_storage_revision != current {
            return Err(SaveGameError::Stale {
                slot: request.slot,
                expected: request.expected_storage_revision,
                actual: current,
            });
        }

        let prior_revision = self
            .read_document(request.slot)
            .ok()
            .flatten()
            .map_or(0, |document| document.metadata.revision);
        let snapshot = runtime.snapshot();
        GameRuntime::from_snapshot(snapshot.clone()).map_err(|error| SaveGameError::Corrupt {
            slot: request.slot,
            message: format!("runtime snapshot did not re-admit before save: {error:?}"),
        })?;
        let player_state = runtime
            .session()
            .health(EntityId::new(identity.player_entity))
            .map_or(SavePlayerState::Unavailable, |health| match health.state {
                VitalityState::Alive => SavePlayerState::Alive,
                VitalityState::Dead => SavePlayerState::Dead,
            });
        let document = SaveGameDocument {
            schema_version: SAVE_GAME_SCHEMA_VERSION,
            slot: request.slot,
            project: identity.clone(),
            metadata: SaveGameMetadata {
                revision: prior_revision.saturating_add(1).max(1),
                saved_at_unix_milliseconds: request.saved_at_unix_milliseconds,
                display_name: request.slot.display_name().to_owned(),
                tick: snapshot.tick,
                snapshot_schema_version: snapshot.schema_version,
                player_state,
                level_complete: runtime.is_level_complete(),
            },
            snapshot,
        };
        let mut encoded = serde_json::to_vec_pretty(&document)
            .map_err(|error| SaveGameError::Encode(error.to_string()))?;
        encoded.push(b'\n');
        if encoded.len() > self.max_bytes {
            return Err(SaveGameError::TooLarge {
                slot: request.slot,
                actual: u64::try_from(encoded.len()).unwrap_or(u64::MAX),
                limit: self.max_bytes,
            });
        }
        self.write_atomic(
            request.slot,
            &path,
            &encoded,
            request.expected_storage_revision.as_ref(),
        )?;
        Ok(self.inspect(request.slot, identity))
    }

    pub fn load(
        &self,
        identity: &SaveProjectIdentity,
        request: SaveLoadRequest,
    ) -> Result<LoadedSaveGame, SaveGameError> {
        let path = self.slot_path(request.slot);
        let Some((bytes, storage_revision)) = self.read_slot_bytes(request.slot, &path)? else {
            return Err(SaveGameError::Empty { slot: request.slot });
        };
        if request
            .expected_storage_revision
            .as_ref()
            .is_some_and(|expected| expected != &storage_revision)
        {
            return Err(SaveGameError::Stale {
                slot: request.slot,
                expected: request.expected_storage_revision,
                actual: Some(storage_revision),
            });
        }
        let (runtime, summary) =
            self.decode_and_validate(request.slot, bytes, storage_revision, identity)?;
        Ok(LoadedSaveGame { runtime, summary })
    }

    fn slot_path(&self, slot: SaveSlotId) -> PathBuf {
        self.root.join(format!("{}.save.json", slot.file_stem()))
    }

    fn pending_path(&self, slot: SaveSlotId) -> PathBuf {
        self.root
            .join(format!(".{}.save.pending", slot.file_stem()))
    }

    fn lock_slot(&self, slot: SaveSlotId) -> Result<File, SaveGameError> {
        fs::create_dir_all(&self.root).map_err(|error| SaveGameError::Io {
            operation: "create directory",
            message: error.to_string(),
        })?;
        let lock_path = self.root.join(format!(".{}.save.lock", slot.file_stem()));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|error| SaveGameError::Io {
                operation: "open slot lock",
                message: error.to_string(),
            })?;
        file.lock().map_err(|error| SaveGameError::Io {
            operation: "lock slot",
            message: error.to_string(),
        })?;
        Ok(file)
    }

    fn read_document(&self, slot: SaveSlotId) -> Result<Option<SaveGameDocument>, SaveGameError> {
        let path = self.slot_path(slot);
        let Some((bytes, _)) = self.read_slot_bytes(slot, &path)? else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| SaveGameError::Corrupt {
                slot,
                message: error.to_string(),
            })
    }

    fn read_slot_bytes(
        &self,
        slot: SaveSlotId,
        path: &Path,
    ) -> Result<Option<(Vec<u8>, String)>, SaveGameError> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(SaveGameError::Io {
                    operation: "open",
                    message: error.to_string(),
                });
            }
        };
        let length = file
            .metadata()
            .map_err(|error| SaveGameError::Io {
                operation: "inspect",
                message: error.to_string(),
            })?
            .len();
        if length > self.max_bytes as u64 {
            return Err(SaveGameError::TooLarge {
                slot,
                actual: length,
                limit: self.max_bytes,
            });
        }
        let mut bytes = Vec::with_capacity(length as usize);
        file.read_to_end(&mut bytes)
            .map_err(|error| SaveGameError::Io {
                operation: "read",
                message: error.to_string(),
            })?;
        let storage_revision = content_revision(&bytes);
        Ok(Some((bytes, storage_revision)))
    }

    fn decode_and_validate(
        &self,
        slot: SaveSlotId,
        bytes: Vec<u8>,
        storage_revision: String,
        identity: &SaveProjectIdentity,
    ) -> Result<(GameRuntime, SaveSlotSummary), SaveGameError> {
        let document: SaveGameDocument =
            serde_json::from_slice(&bytes).map_err(|error| SaveGameError::Corrupt {
                slot,
                message: error.to_string(),
            })?;
        if document.schema_version != SAVE_GAME_SCHEMA_VERSION {
            return Err(SaveGameError::Incompatible {
                slot,
                message: format!(
                    "save schema {} is not supported by schema {}",
                    document.schema_version, SAVE_GAME_SCHEMA_VERSION
                ),
            });
        }
        if document.slot != slot {
            return Err(SaveGameError::Corrupt {
                slot,
                message: format!("document declares slot {:?}", document.slot),
            });
        }
        if document.project != *identity {
            return Err(SaveGameError::Incompatible {
                slot,
                message: incompatible_project_message(&document.project, identity),
            });
        }
        if document.metadata.snapshot_schema_version != document.snapshot.schema_version {
            return Err(SaveGameError::Corrupt {
                slot,
                message: "metadata snapshot schema does not match the payload".to_owned(),
            });
        }
        if document.metadata.tick != document.snapshot.tick {
            return Err(SaveGameError::Corrupt {
                slot,
                message: "metadata tick does not match the payload".to_owned(),
            });
        }
        let runtime = GameRuntime::from_snapshot(document.snapshot).map_err(|error| match error {
            GameSnapshotError::UnsupportedSchema { actual } => SaveGameError::Incompatible {
                slot,
                message: format!(
                    "snapshot schema {actual} is not supported by schema {GAME_SNAPSHOT_SCHEMA_VERSION}"
                ),
            },
            error => SaveGameError::Corrupt {
                slot,
                message: format!("snapshot admission failed: {error:?}"),
            },
        })?;
        let actual_player_state = runtime
            .session()
            .health(EntityId::new(identity.player_entity))
            .map_or(SavePlayerState::Unavailable, |health| match health.state {
                VitalityState::Alive => SavePlayerState::Alive,
                VitalityState::Dead => SavePlayerState::Dead,
            });
        if document.metadata.player_state != actual_player_state {
            return Err(SaveGameError::Corrupt {
                slot,
                message: "metadata player state does not match the payload".to_owned(),
            });
        }
        if document.metadata.level_complete != runtime.is_level_complete() {
            return Err(SaveGameError::Corrupt {
                slot,
                message: "metadata completion state does not match the payload".to_owned(),
            });
        }
        let summary = SaveSlotSummary {
            slot,
            compatibility: SaveSlotCompatibility::Available,
            storage_revision: Some(storage_revision),
            metadata: Some(document.metadata),
            project: Some(document.project),
            diagnostic: None,
        };
        Ok((runtime, summary))
    }

    fn write_atomic(
        &self,
        slot: SaveSlotId,
        path: &Path,
        bytes: &[u8],
        expected_storage_revision: Option<&String>,
    ) -> Result<(), SaveGameError> {
        fs::create_dir_all(&self.root).map_err(|error| SaveGameError::Io {
            operation: "create directory",
            message: error.to_string(),
        })?;
        let pending = self.pending_path(slot);
        match fs::remove_file(&pending) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(SaveGameError::Io {
                    operation: "remove stale pending file",
                    message: error.to_string(),
                });
            }
        }
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&pending)
            .map_err(|error| SaveGameError::Io {
                operation: "create pending file",
                message: error.to_string(),
            })?;
        if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&pending);
            return Err(SaveGameError::Io {
                operation: "write pending save",
                message: error.to_string(),
            });
        }
        let actual = match observe_storage_revision(path, self.max_bytes) {
            Ok(actual) => actual,
            Err(error) => {
                let _ = fs::remove_file(&pending);
                return Err(error);
            }
        };
        if actual.as_ref() != expected_storage_revision {
            let _ = fs::remove_file(&pending);
            return Err(SaveGameError::Stale {
                slot,
                expected: expected_storage_revision.cloned(),
                actual,
            });
        }
        if let Err(error) = fs::rename(&pending, path) {
            let _ = fs::remove_file(&pending);
            return Err(SaveGameError::Io {
                operation: "atomic replace",
                message: error.to_string(),
            });
        }
        if let Ok(directory) = File::open(&self.root) {
            let _ = directory.sync_all();
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SaveGameDocument {
    schema_version: u32,
    slot: SaveSlotId,
    project: SaveProjectIdentity,
    metadata: SaveGameMetadata,
    snapshot: GameSnapshot,
}

fn summary_from_error(
    slot: SaveSlotId,
    storage_revision: String,
    error: SaveGameError,
) -> SaveSlotSummary {
    let compatibility = match error {
        SaveGameError::Incompatible { .. } => SaveSlotCompatibility::Incompatible,
        _ => SaveSlotCompatibility::Corrupt,
    };
    SaveSlotSummary {
        slot,
        compatibility,
        storage_revision: Some(storage_revision),
        metadata: None,
        project: None,
        diagnostic: Some(error.to_string()),
    }
}

fn incompatible_project_message(
    actual: &SaveProjectIdentity,
    expected: &SaveProjectIdentity,
) -> String {
    if actual.project_id != expected.project_id {
        return format!(
            "project {} does not match {}",
            actual.project_id, expected.project_id
        );
    }
    if actual.entry_scene != expected.entry_scene {
        return format!(
            "entry scene {} does not match {}",
            actual.entry_scene, expected.entry_scene
        );
    }
    if actual.player_entity != expected.player_entity {
        return format!(
            "player entity {} does not match {}",
            actual.player_entity, expected.player_entity
        );
    }
    if actual.project_schema_version != expected.project_schema_version {
        return format!(
            "project schema {} does not match {}",
            actual.project_schema_version, expected.project_schema_version
        );
    }
    "authored content revision no longer matches".to_owned()
}

fn storage_revision_for_path(path: &Path, max_bytes: usize) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > max_bytes as u64 {
        return Some(format!("oversize:{}", metadata.len()));
    }
    let bytes = fs::read(path).ok()?;
    Some(content_revision(&bytes))
}

fn observe_storage_revision(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<String>, SaveGameError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(SaveGameError::Io {
                operation: "open current slot",
                message: error.to_string(),
            });
        }
    };
    let length = file
        .metadata()
        .map_err(|error| SaveGameError::Io {
            operation: "inspect current slot",
            message: error.to_string(),
        })?
        .len();
    if length > max_bytes as u64 {
        return Ok(Some(format!("oversize:{length}")));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| SaveGameError::Io {
            operation: "read current slot",
            message: error.to_string(),
        })?;
    Ok(Some(content_revision(&bytes)))
}

fn content_revision(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}:{}", bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode_project_document, encode_game_snapshot, ResolvedPlayerAction};

    const PROJECT: &str = include_str!("../../../../content/projects/doom-e1m1.project.json");

    fn fixture() -> (StoredProject, GameRuntime) {
        let project = decode_project_document(PROJECT).unwrap().project;
        let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
        (project, runtime)
    }

    fn temporary_root(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "loading-bay-save-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn exact_snapshot_round_trip_is_atomic_and_stale_guarded() {
        let root = temporary_root("round-trip");
        let store = SaveGameStore::new(&root);
        let (project, mut runtime) = fixture();
        let identity = SaveProjectIdentity::from_project(&project, EntityId::new(1)).unwrap();
        runtime
            .apply_player_action(
                EntityId::new(1),
                ResolvedPlayerAction::Move {
                    forward: 1.0,
                    right: 0.0,
                },
            )
            .unwrap();
        let expected = encode_game_snapshot(&runtime).unwrap();

        let saved = store
            .save(
                &identity,
                SaveWriteRequest {
                    slot: SaveSlotId::Slot1,
                    overwrite: false,
                    expected_storage_revision: None,
                    saved_at_unix_milliseconds: 42,
                },
                &runtime,
            )
            .unwrap();
        assert_eq!(saved.compatibility, SaveSlotCompatibility::Available);
        assert_eq!(
            saved.metadata.as_ref().unwrap().saved_at_unix_milliseconds,
            42
        );
        let loaded = store
            .load(
                &identity,
                SaveLoadRequest {
                    slot: SaveSlotId::Slot1,
                    expected_storage_revision: saved.storage_revision.clone(),
                },
            )
            .unwrap();
        assert_eq!(encode_game_snapshot(&loaded.runtime).unwrap(), expected);
        assert!(matches!(
            store.save(
                &identity,
                SaveWriteRequest {
                    slot: SaveSlotId::Slot1,
                    overwrite: true,
                    expected_storage_revision: Some("stale".to_owned()),
                    saved_at_unix_milliseconds: 43,
                },
                &runtime,
            ),
            Err(SaveGameError::Stale { .. })
        ));
        assert_eq!(
            encode_game_snapshot(
                &store
                    .load(
                        &identity,
                        SaveLoadRequest {
                            slot: SaveSlotId::Slot1,
                            expected_storage_revision: saved.storage_revision,
                        },
                    )
                    .unwrap()
                    .runtime,
            )
            .unwrap(),
            expected
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_and_incompatible_slots_fail_before_runtime_publication() {
        let root = temporary_root("reject");
        let store = SaveGameStore::new(&root);
        let (project, runtime) = fixture();
        let identity = SaveProjectIdentity::from_project(&project, EntityId::new(1)).unwrap();
        let saved = store
            .save(
                &identity,
                SaveWriteRequest {
                    slot: SaveSlotId::Checkpoint,
                    overwrite: false,
                    expected_storage_revision: None,
                    saved_at_unix_milliseconds: 7,
                },
                &runtime,
            )
            .unwrap();
        let path = store.slot_path(SaveSlotId::Checkpoint);
        let before = fs::read(&path).unwrap();
        fs::write(&path, b"{broken").unwrap();
        assert_eq!(
            store
                .inspect(SaveSlotId::Checkpoint, &identity)
                .compatibility,
            SaveSlotCompatibility::Corrupt
        );
        assert!(matches!(
            store.load(
                &identity,
                SaveLoadRequest {
                    slot: SaveSlotId::Checkpoint,
                    expected_storage_revision: None,
                }
            ),
            Err(SaveGameError::Corrupt { .. })
        ));

        let mut mismatched_metadata: serde_json::Value = serde_json::from_slice(&before).unwrap();
        mismatched_metadata["metadata"]["playerState"] = "dead".into();
        fs::write(
            &path,
            serde_json::to_vec_pretty(&mismatched_metadata).unwrap(),
        )
        .unwrap();
        assert_eq!(
            store
                .inspect(SaveSlotId::Checkpoint, &identity)
                .compatibility,
            SaveSlotCompatibility::Corrupt
        );
        assert!(matches!(
            store.load(
                &identity,
                SaveLoadRequest {
                    slot: SaveSlotId::Checkpoint,
                    expected_storage_revision: None,
                }
            ),
            Err(SaveGameError::Corrupt { .. })
        ));

        fs::write(&path, before).unwrap();
        let mut other = identity.clone();
        other.content_revision.push_str("-changed");
        assert_eq!(
            store.inspect(SaveSlotId::Checkpoint, &other).compatibility,
            SaveSlotCompatibility::Incompatible
        );
        assert!(matches!(
            store.load(
                &other,
                SaveLoadRequest {
                    slot: SaveSlotId::Checkpoint,
                    expected_storage_revision: saved.storage_revision,
                }
            ),
            Err(SaveGameError::Incompatible { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn competing_writers_cannot_overwrite_the_same_observed_revision() {
        let root = temporary_root("competing-writers");
        let store = SaveGameStore::new(&root);
        let (project, runtime) = fixture();
        let identity = SaveProjectIdentity::from_project(&project, EntityId::new(1)).unwrap();
        let initial = store
            .save(
                &identity,
                SaveWriteRequest {
                    slot: SaveSlotId::Slot2,
                    overwrite: false,
                    expected_storage_revision: None,
                    saved_at_unix_milliseconds: 1,
                },
                &runtime,
            )
            .unwrap();
        let expected = initial.storage_revision.unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut writers = Vec::new();
        for saved_at in [2, 3] {
            let store = store.clone();
            let identity = identity.clone();
            let expected = expected.clone();
            let barrier = barrier.clone();
            writers.push(std::thread::spawn(move || {
                let runtime = GameRuntime::from_stored_project(PROJECT).unwrap();
                barrier.wait();
                store.save(
                    &identity,
                    SaveWriteRequest {
                        slot: SaveSlotId::Slot2,
                        overwrite: true,
                        expected_storage_revision: Some(expected),
                        saved_at_unix_milliseconds: saved_at,
                    },
                    &runtime,
                )
            }));
        }
        barrier.wait();
        let outcomes = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Err(SaveGameError::Stale { .. })))
                .count(),
            1
        );
        assert_eq!(
            store
                .load(
                    &identity,
                    SaveLoadRequest {
                        slot: SaveSlotId::Slot2,
                        expected_storage_revision: None,
                    },
                )
                .unwrap()
                .summary
                .compatibility,
            SaveSlotCompatibility::Available
        );
        fs::remove_dir_all(root).unwrap();
    }
}
