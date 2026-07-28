//! Closed, downstream-owned Loading Bay weapon project authoring.
//!
//! This contract intentionally edits only the durable item definition selected
//! through a provider-derived weapon entity. Inventory binding is identity
//! context, not a second mutation surface. Live ammunition, cooldowns, attacks,
//! runtime state, and save data never enter this module.

use std::collections::BTreeMap;

use content_store::ContentHash;
use serde::{Deserialize, Serialize};

use crate::{
    admit_stored_project_with_document, ProjectLocation, ProjectStore, ProjectStoreError,
    StoredInventory, StoredItemDefinition, StoredItemKind, StoredProject, StoredProjectError,
    StoredWeaponAttackMode, MAX_ITEM_DEFINITION_ID_BYTES,
};

pub const LOADING_BAY_WEAPON_COMPONENT_TYPE_ID: &str = "rusty-engine-demo.loading-bay.weapon";
pub const LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID: &str =
    "rusty-engine-demo.loading-bay.weapon-authoring";
pub const LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION: u32 = 1;
pub const MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES: usize = 16 * 1024;
pub const MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_REQUEST_ID_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LoadingBayWeaponAuthoringRequest {
    ReadLoadingBayWeapon {
        contract_version: u32,
        request_id: String,
        expected_project_hash: String,
        owner_entity_id: u64,
    },
    ReplaceLoadingBayWeapon {
        contract_version: u32,
        request_id: String,
        expected_project_hash: String,
        owner_entity_id: u64,
        expected_component_revision: String,
        candidate: LoadingBayWeaponAuthoringCandidate,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LoadingBayWeaponAuthoringResponse {
    LoadingBayWeaponRead {
        contract_version: u32,
        request_id: String,
        weapon: LoadingBayWeaponAuthoringWeapon,
    },
    LoadingBayWeaponReplaced {
        contract_version: u32,
        request_id: String,
        receipt: LoadingBayWeaponAuthoringReceipt,
        weapon: LoadingBayWeaponAuthoringWeapon,
    },
    LoadingBayWeaponRejected {
        contract_version: u32,
        request_id: String,
        rejection: LoadingBayWeaponAuthoringRejection,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LoadingBayWeaponAuthoringCandidate {
    pub attack_mode: LoadingBayWeaponAuthoringAttackMode,
    pub damage: u32,
    pub max_distance: f32,
    pub cooldown_ticks: u64,
    pub ammunition_item_id: String,
    pub ammunition_cost: u32,
    pub muzzle_offset: [f32; 3],
    pub presentation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LoadingBayWeaponAuthoringAttackMode {
    Hitscan,
    Spread {
        pellet_count: u8,
        spread_degrees: f32,
    },
    Automatic,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayWeaponAuthoringWeapon {
    pub component_type_id: &'static str,
    pub contract_id: &'static str,
    pub contract_version: u32,
    pub owner_entity_id: u64,
    pub component_revision: String,
    pub item_definition_id: String,
    pub binding: LoadingBayWeaponAuthoringBinding,
    pub definition: LoadingBayWeaponAuthoringCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayWeaponAuthoringBinding {
    pub inventory_owner_entity_id: u64,
    pub slot_index: usize,
    pub starting_quantity: u32,
    pub initially_equipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayWeaponAuthoringReceipt {
    pub owner_entity_id: u64,
    pub item_definition_id: String,
    pub project_hash_before: String,
    pub project_hash_after: String,
    pub component_revision_before: String,
    pub component_revision_after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayWeaponAuthoringRejection {
    pub code: LoadingBayWeaponAuthoringRejectionCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadingBayWeaponAuthoringRejectionCode {
    UnsupportedContractVersion,
    InvalidRequestId,
    InvalidProjectHash,
    StaleProject,
    ProjectRejected,
    WeaponNotFound,
    StaleComponent,
    CandidateRejected,
    ProjectStoreFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadingBayWeaponAuthoringCodecError {
    RequestTooLarge { actual: usize, limit: usize },
    MalformedRequest { path: String, message: String },
    ResponseTooLarge { actual: usize, limit: usize },
    ResponseEncode(String),
}

impl std::fmt::Display for LoadingBayWeaponAuthoringCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequestTooLarge { actual, limit } => {
                write!(formatter, "request is {actual} bytes, exceeding {limit}")
            }
            Self::MalformedRequest { path, message } => {
                write!(formatter, "malformed request at {path}: {message}")
            }
            Self::ResponseTooLarge { actual, limit } => {
                write!(formatter, "response is {actual} bytes, exceeding {limit}")
            }
            Self::ResponseEncode(message) => write!(formatter, "response encode failed: {message}"),
        }
    }
}

impl std::error::Error for LoadingBayWeaponAuthoringCodecError {}

pub fn decode_loading_bay_weapon_authoring_request(
    input: &str,
) -> Result<LoadingBayWeaponAuthoringRequest, LoadingBayWeaponAuthoringCodecError> {
    if input.len() > MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES {
        return Err(LoadingBayWeaponAuthoringCodecError::RequestTooLarge {
            actual: input.len(),
            limit: MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
        });
    }
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let request = serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        LoadingBayWeaponAuthoringCodecError::MalformedRequest {
            path: json_path(&error.path().to_string()),
            message: error.inner().to_string(),
        }
    })?;
    deserializer.end().map_err(
        |error| LoadingBayWeaponAuthoringCodecError::MalformedRequest {
            path: "$".to_string(),
            message: error.to_string(),
        },
    )?;
    Ok(request)
}

pub fn encode_loading_bay_weapon_authoring_response(
    response: &LoadingBayWeaponAuthoringResponse,
) -> Result<String, LoadingBayWeaponAuthoringCodecError> {
    let encoded = serde_json::to_string(response)
        .map_err(|error| LoadingBayWeaponAuthoringCodecError::ResponseEncode(error.to_string()))?;
    if encoded.len() > MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES {
        return Err(LoadingBayWeaponAuthoringCodecError::ResponseTooLarge {
            actual: encoded.len(),
            limit: MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES,
        });
    }
    Ok(encoded)
}

#[derive(Debug, Clone, Default)]
pub struct LoadingBayWeaponAuthoringService {
    store: ProjectStore,
}

impl LoadingBayWeaponAuthoringService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(
        &self,
        location: &ProjectLocation,
        request: LoadingBayWeaponAuthoringRequest,
    ) -> LoadingBayWeaponAuthoringResponse {
        let (_, request_id) = request_identity(&request);
        let result = self.handle_checked(location, request);
        match result {
            Ok(response) => response,
            Err(rejection) => LoadingBayWeaponAuthoringResponse::LoadingBayWeaponRejected {
                contract_version: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
                request_id,
                rejection,
            },
        }
    }

    fn handle_checked(
        &self,
        location: &ProjectLocation,
        request: LoadingBayWeaponAuthoringRequest,
    ) -> Result<LoadingBayWeaponAuthoringResponse, LoadingBayWeaponAuthoringRejection> {
        let (requested_contract_version, request_id) = request_identity(&request);
        require_common(requested_contract_version, &request_id)?;
        let contract_version = LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION;
        location.revalidate().map_err(|error| {
            rejection(
                LoadingBayWeaponAuthoringRejectionCode::ProjectStoreFailure,
                error.to_string(),
                None,
            )
        })?;

        match request {
            LoadingBayWeaponAuthoringRequest::ReadLoadingBayWeapon {
                expected_project_hash,
                owner_entity_id,
                ..
            } => {
                let source = self.load_expected(location, &expected_project_hash)?;
                let weapon = resolve_weapon(&source.decoded.project, owner_entity_id)?;
                Ok(LoadingBayWeaponAuthoringResponse::LoadingBayWeaponRead {
                    contract_version,
                    request_id,
                    weapon,
                })
            }
            LoadingBayWeaponAuthoringRequest::ReplaceLoadingBayWeapon {
                expected_project_hash,
                owner_entity_id,
                expected_component_revision,
                candidate,
                ..
            } => {
                let expected_hash = parse_hash(&expected_project_hash)?;
                let source = self.load_expected(location, &expected_project_hash)?;
                let before = resolve_weapon(&source.decoded.project, owner_entity_id)?;
                let expected_component = parse_component_revision(&expected_component_revision)?;
                let actual_component = parse_component_revision(&before.component_revision)?;
                if actual_component != expected_component {
                    return Err(rejection(
                        LoadingBayWeaponAuthoringRejectionCode::StaleComponent,
                        format!(
                            "expected component revision {expected_component_revision}, found {}",
                            before.component_revision
                        ),
                        None,
                    ));
                }

                let mut candidate_project = source.decoded.project;
                replace_definition(
                    &mut candidate_project,
                    &before.item_definition_id,
                    candidate,
                )?;
                let (admitted_candidate, _) =
                    admit_stored_project_with_document(candidate_project).map_err(project_error)?;
                let after = resolve_weapon(admitted_candidate.document(), owner_entity_id)?;
                let installed_hash = self
                    .store
                    .replace_if_unchanged(
                        location.project_file(),
                        &admitted_candidate,
                        expected_hash,
                    )
                    .map_err(store_error)?;
                Ok(
                    LoadingBayWeaponAuthoringResponse::LoadingBayWeaponReplaced {
                        contract_version,
                        request_id,
                        receipt: LoadingBayWeaponAuthoringReceipt {
                            owner_entity_id,
                            item_definition_id: before.item_definition_id,
                            project_hash_before: expected_hash.to_hex(),
                            project_hash_after: installed_hash.to_hex(),
                            component_revision_before: before.component_revision,
                            component_revision_after: after.component_revision.clone(),
                        },
                        weapon: after,
                    },
                )
            }
        }
    }

    fn load_expected(
        &self,
        location: &ProjectLocation,
        expected_project_hash: &str,
    ) -> Result<crate::LoadedProjectSource, LoadingBayWeaponAuthoringRejection> {
        let expected = parse_hash(expected_project_hash)?;
        let source = self
            .store
            .load_source(location.project_file())
            .map_err(store_error)?;
        if source.source_hash() != expected {
            return Err(rejection(
                LoadingBayWeaponAuthoringRejectionCode::StaleProject,
                format!(
                    "expected project hash {expected}, found {}",
                    source.source_hash()
                ),
                None,
            ));
        }
        admit_stored_project_with_document(source.decoded.project.clone())
            .map_err(source_project_error)?;
        Ok(source)
    }
}

impl LoadingBayWeaponAuthoringRequest {
    pub(crate) fn request_id(&self) -> &str {
        match self {
            Self::ReadLoadingBayWeapon { request_id, .. }
            | Self::ReplaceLoadingBayWeapon { request_id, .. } => request_id,
        }
    }
}

impl LoadingBayWeaponAuthoringResponse {
    pub(crate) fn rejected(
        request_id: impl Into<String>,
        code: LoadingBayWeaponAuthoringRejectionCode,
        message: impl Into<String>,
        path: Option<String>,
    ) -> Self {
        Self::LoadingBayWeaponRejected {
            contract_version: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
            request_id: request_id.into(),
            rejection: rejection(code, message, path),
        }
    }
}

fn request_identity(request: &LoadingBayWeaponAuthoringRequest) -> (u32, String) {
    match request {
        LoadingBayWeaponAuthoringRequest::ReadLoadingBayWeapon {
            contract_version,
            request_id,
            ..
        }
        | LoadingBayWeaponAuthoringRequest::ReplaceLoadingBayWeapon {
            contract_version,
            request_id,
            ..
        } => (*contract_version, request_id.clone()),
    }
}

fn require_common(
    contract_version: u32,
    request_id: &str,
) -> Result<(), LoadingBayWeaponAuthoringRejection> {
    if contract_version != LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION {
        return Err(rejection(
            LoadingBayWeaponAuthoringRejectionCode::UnsupportedContractVersion,
            format!(
                "contract version {contract_version} is unsupported; expected {}",
                LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION
            ),
            None,
        ));
    }
    if request_id.is_empty() || request_id.len() > MAX_REQUEST_ID_BYTES {
        return Err(rejection(
            LoadingBayWeaponAuthoringRejectionCode::InvalidRequestId,
            format!("requestId must contain 1..={MAX_REQUEST_ID_BYTES} UTF-8 bytes"),
            Some("$.requestId".to_string()),
        ));
    }
    Ok(())
}

fn parse_hash(value: &str) -> Result<ContentHash, LoadingBayWeaponAuthoringRejection> {
    ContentHash::parse(value).map_err(|error| {
        rejection(
            LoadingBayWeaponAuthoringRejectionCode::InvalidProjectHash,
            error.to_string(),
            Some("$.expectedProjectHash".to_string()),
        )
    })
}

fn parse_component_revision(
    value: &str,
) -> Result<ContentHash, LoadingBayWeaponAuthoringRejection> {
    ContentHash::parse(value).map_err(|error| {
        rejection(
            LoadingBayWeaponAuthoringRejectionCode::StaleComponent,
            format!("invalid component revision: {error}"),
            Some("$.expectedComponentRevision".to_string()),
        )
    })
}

fn replace_definition(
    project: &mut StoredProject,
    item_definition_id: &str,
    candidate: LoadingBayWeaponAuthoringCandidate,
) -> Result<(), LoadingBayWeaponAuthoringRejection> {
    let definition = project
        .item_definitions
        .iter_mut()
        .find(|definition| definition.id == item_definition_id)
        .ok_or_else(|| weapon_not_found(0))?;
    definition.max_quantity = 1;
    definition.kind = candidate.into_stored_kind();
    Ok(())
}

impl LoadingBayWeaponAuthoringCandidate {
    fn into_stored_kind(self) -> StoredItemKind {
        let (attack_mode, pellet_count, spread_degrees) = match self.attack_mode {
            LoadingBayWeaponAuthoringAttackMode::Hitscan => {
                (StoredWeaponAttackMode::Hitscan, None, None)
            }
            LoadingBayWeaponAuthoringAttackMode::Spread {
                pellet_count,
                spread_degrees,
            } => (
                StoredWeaponAttackMode::Spread,
                Some(pellet_count),
                Some(spread_degrees),
            ),
            LoadingBayWeaponAuthoringAttackMode::Automatic => {
                (StoredWeaponAttackMode::Automatic, None, None)
            }
        };
        StoredItemKind::Weapon {
            ammunition: self.ammunition_item_id,
            attack_mode: Some(attack_mode),
            pellet_count,
            spread_degrees,
            damage: Some(self.damage),
            max_distance: Some(self.max_distance),
            cooldown_ticks: Some(self.cooldown_ticks),
            ammunition_cost: Some(self.ammunition_cost),
            muzzle_offset: Some(self.muzzle_offset),
            presentation: Some(self.presentation),
        }
    }
}

struct ResolvedBinding<'a> {
    owner_entity_id: u64,
    inventory_owner_entity_id: u64,
    slot_index: usize,
    item_definition_id: &'a str,
    inventory: &'a StoredInventory,
}

fn resolve_weapon(
    project: &StoredProject,
    owner_entity_id: u64,
) -> Result<LoadingBayWeaponAuthoringWeapon, LoadingBayWeaponAuthoringRejection> {
    let binding = resolve_binding(project, owner_entity_id)?;
    let definition = project
        .item_definitions
        .iter()
        .find(|definition| definition.id == binding.item_definition_id)
        .ok_or_else(|| weapon_not_found(owner_entity_id))?;
    let candidate =
        candidate_from_definition(definition).ok_or_else(|| weapon_not_found(owner_entity_id))?;
    let starting_quantity = binding
        .inventory
        .starting_stacks
        .iter()
        .find(|stack| stack.item == binding.item_definition_id)
        .map_or(0, |stack| stack.quantity);
    Ok(LoadingBayWeaponAuthoringWeapon {
        component_type_id: LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
        contract_id: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
        contract_version: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
        owner_entity_id: binding.owner_entity_id,
        component_revision: component_revision(definition)?,
        item_definition_id: binding.item_definition_id.to_string(),
        binding: LoadingBayWeaponAuthoringBinding {
            inventory_owner_entity_id: binding.inventory_owner_entity_id,
            slot_index: binding.slot_index,
            starting_quantity,
            initially_equipped: binding.inventory.initially_equipped_weapon.as_deref()
                == Some(binding.item_definition_id),
        },
        definition: candidate,
    })
}

fn resolve_binding(
    project: &StoredProject,
    requested_entity_id: u64,
) -> Result<ResolvedBinding<'_>, LoadingBayWeaponAuthoringRejection> {
    all_bindings(project)
        .map_err(|message| {
            rejection(
                LoadingBayWeaponAuthoringRejectionCode::ProjectRejected,
                message,
                None,
            )
        })?
        .into_iter()
        .find(|binding| binding.owner_entity_id == requested_entity_id)
        .ok_or_else(|| weapon_not_found(requested_entity_id))
}

pub(crate) fn loading_bay_weapon_owner_entity_ids(
    project: &StoredProject,
) -> Result<Vec<u64>, &'static str> {
    Ok(all_bindings(project)?
        .into_iter()
        .map(|binding| binding.owner_entity_id)
        .collect())
}

fn all_bindings(project: &StoredProject) -> Result<Vec<ResolvedBinding<'_>>, &'static str> {
    let scene = project
        .scenes
        .iter()
        .find(|scene| scene.id == project.entry_scene)
        .ok_or("entry scene is missing while resolving weapon inspector owners")?;
    let first = scene
        .entities
        .iter()
        .map(|entity| entity.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("weapon inspector owner identity exceeds the supported range")?;
    let inventories: BTreeMap<_, _> = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .inventory
                .as_ref()
                .map(|inventory| (entity.id, inventory))
        })
        .collect();
    let mut bindings = Vec::new();
    for (inventory_owner_entity_id, inventory) in inventories {
        for (slot_index, item_definition_id) in inventory.weapon_slots.iter().enumerate() {
            let offset = u64::try_from(bindings.len())
                .map_err(|_| "weapon inspector owner identity exceeds the supported range")?;
            let owner_entity_id = first
                .checked_add(offset)
                .ok_or("weapon inspector owner identity exceeds the supported range")?;
            bindings.push(ResolvedBinding {
                owner_entity_id,
                inventory_owner_entity_id,
                slot_index,
                item_definition_id,
                inventory,
            });
        }
    }
    Ok(bindings)
}

fn candidate_from_definition(
    definition: &StoredItemDefinition,
) -> Option<LoadingBayWeaponAuthoringCandidate> {
    let StoredItemKind::Weapon {
        ammunition,
        attack_mode,
        pellet_count,
        spread_degrees,
        damage,
        max_distance,
        cooldown_ticks,
        ammunition_cost,
        muzzle_offset,
        presentation,
    } = &definition.kind
    else {
        return None;
    };
    let attack_mode = match (*attack_mode)? {
        StoredWeaponAttackMode::Hitscan => LoadingBayWeaponAuthoringAttackMode::Hitscan,
        StoredWeaponAttackMode::Spread => LoadingBayWeaponAuthoringAttackMode::Spread {
            pellet_count: (*pellet_count)?,
            spread_degrees: (*spread_degrees)?,
        },
        StoredWeaponAttackMode::Automatic => LoadingBayWeaponAuthoringAttackMode::Automatic,
    };
    Some(LoadingBayWeaponAuthoringCandidate {
        attack_mode,
        damage: (*damage)?,
        max_distance: (*max_distance)?,
        cooldown_ticks: (*cooldown_ticks)?,
        ammunition_item_id: ammunition.clone(),
        ammunition_cost: (*ammunition_cost)?,
        muzzle_offset: (*muzzle_offset)?,
        presentation: presentation.clone()?,
    })
}

fn component_revision(
    definition: &StoredItemDefinition,
) -> Result<String, LoadingBayWeaponAuthoringRejection> {
    let bytes = serde_json::to_vec(definition).map_err(|error| {
        rejection(
            LoadingBayWeaponAuthoringRejectionCode::ProjectStoreFailure,
            format!("could not encode canonical weapon component: {error}"),
            None,
        )
    })?;
    Ok(ContentHash::of(&bytes).to_hex())
}

fn project_error(error: StoredProjectError) -> LoadingBayWeaponAuthoringRejection {
    let diagnostic = error.diagnostic();
    rejection(
        LoadingBayWeaponAuthoringRejectionCode::CandidateRejected,
        diagnostic.message.clone(),
        Some(diagnostic.path.clone()),
    )
}

fn source_project_error(error: StoredProjectError) -> LoadingBayWeaponAuthoringRejection {
    let diagnostic = error.diagnostic();
    rejection(
        LoadingBayWeaponAuthoringRejectionCode::ProjectRejected,
        diagnostic.message.clone(),
        Some(diagnostic.path.clone()),
    )
}

fn store_error(error: ProjectStoreError) -> LoadingBayWeaponAuthoringRejection {
    let code = match error {
        ProjectStoreError::StaleSource { .. } => {
            LoadingBayWeaponAuthoringRejectionCode::StaleProject
        }
        _ => LoadingBayWeaponAuthoringRejectionCode::ProjectStoreFailure,
    };
    rejection(code, error.to_string(), None)
}

fn weapon_not_found(owner_entity_id: u64) -> LoadingBayWeaponAuthoringRejection {
    rejection(
        LoadingBayWeaponAuthoringRejectionCode::WeaponNotFound,
        format!(
            "entry-scene entity {owner_entity_id} is not a provider-derived Loading Bay weapon"
        ),
        Some("$.ownerEntityId".to_string()),
    )
}

fn rejection(
    code: LoadingBayWeaponAuthoringRejectionCode,
    message: impl Into<String>,
    path: Option<String>,
) -> LoadingBayWeaponAuthoringRejection {
    LoadingBayWeaponAuthoringRejection {
        code,
        message: message.into(),
        path,
    }
}

fn json_path(path: &str) -> String {
    if path.is_empty() {
        "$".to_string()
    } else {
        format!("$.{path}")
    }
}

const _: () =
    assert!(MAX_ITEM_DEFINITION_ID_BYTES <= MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES);
