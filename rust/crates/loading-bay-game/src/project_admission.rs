//! One direct admission path from stored project data to concrete game state.

use std::collections::BTreeMap;

use rusty_engine::core_assets::{AssetId, AssetKind};
use rusty_engine::core_ids::EntityId;
use rusty_engine::core_math::Vec3;
use rusty_engine::core_time::TickDelta;
use rusty_engine::engine_spatial::{
    validate_material_voxel, GeneratedRoomConfig, MaterialVoxel, VoxelAuthorityValidationError,
    VoxelCollisionScene,
};
use rusty_engine::entity_state::{EntityDefinition, EntityTransform, Quat, MAX_ABS_TRANSLATION};

use crate::combat::WeaponConfig;
use crate::content::AdmittedProject;
use crate::definition::{GameEntityDefinition, GameEntityDefinitionError};
use crate::door::DoorConfig;
use crate::enemy_combat::{
    EnemyAttackConfig, EnemyAttackKind, EnemyCombatConfig, EnemyPerceptionConfig,
};
use crate::enemy_drop::EnemyDropConfig;
use crate::extraction_beacon::ExtractionBeaconConfig;
use crate::floor_action::FloorActionConfig;
use crate::hazard::HazardConfig;
use crate::interaction::{SwitchConfig, SwitchEffect};
use crate::inventory::{
    ArmorGrantMode, ArmorTransition, InventoryConfig, InventoryStack, ItemDefinition,
    ItemDefinitionId, ItemKind, WeaponAttackMode, WeaponDefinition,
};
use crate::lift::LiftConfig;
use crate::navigation::NavigationConfig;
use crate::pickup::PickupConfig;
use crate::player::{PlayerControllerConfig, PlayerInputBindings};
use crate::progression::{
    DoorAccessConfig, LevelExitConfig, LoadingBayInterlockConfig, RequiredKeyPolicy,
    SecretRegionConfig,
};
use crate::project_codec::decode_project_document;
use crate::session::GameSession;
use crate::stored_project::{
    diagnostic_code, validate_stored_project, StoredAsset, StoredEntityDefinition,
    StoredItemDefinition, StoredItemKind, StoredMaterialVoxel, StoredMaterialVoxelEnvironment,
    StoredProject, StoredProjectError, StoredScene, StoredVoxelEnvironment,
};
use crate::vitality::HealthConfig;

/// Static project data that has passed the same complete semantic admission as
/// runtime construction. The persistence service accepts only this token and
/// therefore cannot save a merely shape-valid project.
#[derive(Debug, Clone, PartialEq)]
pub struct AdmittedStoredProject {
    document: StoredProject,
}

impl AdmittedStoredProject {
    pub fn document(&self) -> &StoredProject {
        &self.document
    }

    pub fn into_document(self) -> StoredProject {
        self.document
    }
}

pub fn decode_and_admit_stored_project(input: &str) -> Result<AdmittedProject, StoredProjectError> {
    admit_stored_project(decode_project_document(input)?.project)
}

pub fn admit_stored_project(
    document: StoredProject,
) -> Result<AdmittedProject, StoredProjectError> {
    admit_stored_project_with_document(document).map(|(_, admitted)| admitted)
}

/// Admit one document once and retain both its static persistence token and the
/// resulting concrete session. Storage sees only the first value.
pub fn admit_stored_project_with_document(
    document: StoredProject,
) -> Result<(AdmittedStoredProject, AdmittedProject), StoredProjectError> {
    validate_stored_project(&document)?;
    if document.project_id == "doom-e1m1" {
        // Enforce Doom-specific closure before any session is built.
        // Find the voxel volume palette and validate against the 54-material manifest.
        if let Some(voxel) = document
            .assets
            .iter()
            .find_map(|asset| asset.voxel_volume.as_ref())
        {
            crate::doom_e1m1_materials::validate_doom_palette_closure(
                &document,
                &voxel.material_palette,
            )?;
        }
        // Verify on-disk PNG bytes match the manifest digests; a same-length mutation must be rejected.
        crate::doom_e1m1_materials::verify_doom_texture_files().map_err(|msg| {
            StoredProjectError::new(
                crate::stored_project::diagnostic_code::INVALID_MATERIAL,
                "assets",
                msg,
            )
        })?;
    }
    let entry_scene_index = document
        .scenes
        .iter()
        .position(|scene| scene.id == document.entry_scene)
        .expect("validated entry scene");
    let catalog = ProjectAssetCatalog::new(&document);
    let item_definitions = document
        .item_definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| authored_item_definition(definition, index))
        .collect::<Result<Vec<_>, _>>()?;

    let mut entry_scene = None;
    for (scene_index, scene) in document.scenes.iter().enumerate() {
        let admitted = admit_scene(scene, scene_index, &catalog, &item_definitions)?;
        if scene_index == entry_scene_index {
            entry_scene = Some(admitted);
        }
    }
    let entry_scene = entry_scene.expect("validated entry scene was admitted");

    Ok((
        AdmittedStoredProject { document },
        AdmittedProject {
            session: entry_scene.session,
            collision_scene: entry_scene.collision_scene,
        },
    ))
}

struct AdmittedScene {
    session: GameSession,
    collision_scene: Option<VoxelCollisionScene>,
}

fn admit_scene(
    scene: &StoredScene,
    scene_index: usize,
    catalog: &ProjectAssetCatalog<'_>,
    item_definitions: &[ItemDefinition],
) -> Result<AdmittedScene, StoredProjectError> {
    catalog.validate_scene(scene, scene_index)?;

    let entity_indexes = index_entities(scene, scene_index)?;
    require_spatial_source(scene, scene_index)?;
    let collision_scene = build_collision_scene(scene, scene_index, catalog)?;
    let definitions = scene
        .entities
        .iter()
        .enumerate()
        .map(|(entity_index, entity)| authored_definition(entity, scene_index, entity_index))
        .collect::<Result<Vec<_>, _>>()?;
    let session = GameSession::from_item_and_entity_definitions(
        item_definitions.iter().cloned(),
        definitions,
    )
    .map_err(|error| definition_error(error, scene_index, &entity_indexes))?;

    Ok(AdmittedScene {
        session,
        collision_scene,
    })
}

fn authored_item_definition(
    authored: &StoredItemDefinition,
    index: usize,
) -> Result<ItemDefinition, StoredProjectError> {
    let path = format!("itemDefinitions[{index}]");
    let id = parse_item_id(&authored.id, &format!("{path}.id"))?;
    let kind = match &authored.kind {
        StoredItemKind::Weapon {
            ammunition,
            repeat_while_held,
            damage_rolls,
            attack_mode,
            pellet_count,
            spread_degrees,
            damage,
            max_distance,
            cooldown_ticks,
            ammunition_cost,
            muzzle_offset,
            presentation,
            projectile_mass,
            projectile_radius,
            projectile_impulse,
            projectile_gravity_scale,
            projectile_lifetime_ticks,
            projectile_restitution,
        } => ItemKind::Weapon(WeaponDefinition {
            attack_mode: match attack_mode.expect("validated current weapon attack mode") {
                crate::StoredWeaponAttackMode::Hitscan => WeaponAttackMode::Hitscan,
                crate::StoredWeaponAttackMode::Spread => WeaponAttackMode::Spread {
                    pellet_count: pellet_count.expect("validated current weapon pellet count"),
                    spread_degrees: spread_degrees.expect("validated current weapon spread angle"),
                },
                crate::StoredWeaponAttackMode::Automatic => WeaponAttackMode::Automatic,
                crate::StoredWeaponAttackMode::Projectile => WeaponAttackMode::Projectile,
            },
            repeat_while_held: *repeat_while_held,
            damage_rolls: *damage_rolls,
            damage: damage.expect("validated current weapon damage"),
            max_distance: max_distance.expect("validated current weapon range"),
            cooldown_ticks: cooldown_ticks.expect("validated current weapon cadence"),
            ammunition: parse_item_id(ammunition, &format!("{path}.kind.ammunition"))?,
            ammunition_cost: ammunition_cost.expect("validated current weapon ammunition cost"),
            muzzle_offset: array_vec3(
                muzzle_offset.expect("validated current weapon muzzle offset"),
            ),
            presentation: presentation
                .clone()
                .expect("validated current weapon presentation"),
            projectile: match attack_mode.expect("validated current weapon attack mode") {
                crate::StoredWeaponAttackMode::Projectile => Some(crate::ProjectileDefinition {
                    mass: projectile_mass.expect("validated projectile mass"),
                    radius: projectile_radius.expect("validated projectile radius"),
                    impulse: projectile_impulse.expect("validated projectile impulse"),
                    gravity_scale: projectile_gravity_scale
                        .expect("validated projectile gravity scale"),
                    lifetime_ticks: projectile_lifetime_ticks
                        .expect("validated projectile lifetime"),
                    restitution: projectile_restitution.expect("validated projectile restitution"),
                }),
                _ => None,
            },
        }),
        StoredItemKind::Ammunition => ItemKind::Ammunition,
        StoredItemKind::AccessKey => ItemKind::AccessKey,
        StoredItemKind::HealthSupply {
            restore_health,
            maximum_health,
            automatic_use,
        } => ItemKind::HealthSupply {
            restore_health: *restore_health,
            maximum_health: *maximum_health,
            automatic_use: *automatic_use,
        },
        StoredItemKind::Armor {
            protection,
            maximum_armor,
            absorption_percent,
            grant_mode,
            transition,
        } => ItemKind::Armor {
            protection: *protection,
            maximum_armor: *maximum_armor,
            absorption_percent: *absorption_percent,
            grant_mode: match grant_mode {
                crate::StoredArmorGrantMode::Add => ArmorGrantMode::Add,
                crate::StoredArmorGrantMode::SetMinimum => ArmorGrantMode::SetMinimum,
            },
            transition: match transition {
                crate::StoredArmorTransition::RejectDifferent => ArmorTransition::RejectDifferent,
                crate::StoredArmorTransition::Preserve => ArmorTransition::Preserve,
                crate::StoredArmorTransition::Replace => ArmorTransition::Replace,
            },
        },
    };
    Ok(ItemDefinition::new(id, kind, authored.max_quantity))
}

/// Materialize the runtime's accepted voxel authority into one explicit static
/// project candidate, then run the complete M5 admission again. Live source
/// revision, receipts, and edit history are deliberately not authored fields.
pub fn materialize_stored_project_voxels(
    source: &AdmittedStoredProject,
    scene: &VoxelCollisionScene,
) -> Result<AdmittedStoredProject, StoredProjectError> {
    let mut document = source.document.clone();
    let scene_index = document
        .scenes
        .iter()
        .position(|candidate| candidate.id == document.entry_scene)
        .expect("admitted project retains its entry scene");
    let gameplay_proxy = document.scenes[scene_index]
        .voxel_environment
        .as_ref()
        .is_some_and(|environment| match environment {
            StoredVoxelEnvironment::Solid(environment) => environment.gameplay_proxy,
            StoredVoxelEnvironment::Material(environment) => environment.gameplay_proxy,
            StoredVoxelEnvironment::GeneratedRoom(environment) => environment.gameplay_proxy,
        });
    document.scenes[scene_index].voxel_environment = Some(StoredVoxelEnvironment::Material(
        StoredMaterialVoxelEnvironment {
            voxel_size: scene.voxel_size(),
            chunk_size: scene.chunk_size(),
            material_voxels: scene
                .material_voxels()
                .iter()
                .map(|voxel| StoredMaterialVoxel {
                    address: voxel.address,
                    material_slot: voxel.material_slot,
                })
                .collect(),
            voxel_assets: Vec::new(),
            gameplay_proxy,
        },
    ));
    admit_stored_project_with_document(document).map(|(stored, _)| stored)
}

struct ProjectAssetCatalog<'a> {
    assets: BTreeMap<String, &'a StoredAsset>,
}

impl<'a> ProjectAssetCatalog<'a> {
    fn new(document: &'a StoredProject) -> Self {
        let assets = document
            .assets
            .iter()
            .map(|asset| (asset.id.clone(), asset))
            .collect();
        Self { assets }
    }

    fn validate_scene(
        &self,
        scene: &StoredScene,
        scene_index: usize,
    ) -> Result<(), StoredProjectError> {
        for (entity_index, entity) in scene.entities.iter().enumerate() {
            let Some(renderable) = &entity.renderable else {
                continue;
            };
            let path = format!("scenes[{scene_index}].entities[{entity_index}].renderable.asset");
            let id = AssetId::parse(&renderable.asset).map_err(|error| {
                StoredProjectError::new(diagnostic_code::INVALID_ASSET_ID, &path, error.to_string())
            })?;
            if !matches!(id.kind(), AssetKind::StaticMesh | AssetKind::AnimatedMesh) {
                return Err(StoredProjectError::new(
                    diagnostic_code::WRONG_ASSET_KIND,
                    path,
                    format!(
                        "renderable requires static or animated mesh identity, found `{}`",
                        id.kind()
                    ),
                ));
            }
            let Some(asset) = self.assets.get(id.as_str()) else {
                return Err(StoredProjectError::new(
                    diagnostic_code::MISSING_ASSET,
                    path,
                    format!("asset `{id}` is not declared in `assets`"),
                ));
            };
            let kind = AssetId::parse(&asset.id)
                .expect("validated catalog identity")
                .kind();
            if !matches!(kind, AssetKind::StaticMesh | AssetKind::AnimatedMesh) {
                return Err(StoredProjectError::new(
                    diagnostic_code::WRONG_ASSET_KIND,
                    path,
                    format!("catalog entry `{id}` is `{kind}`, expected static or animated mesh"),
                ));
            }
        }
        Ok(())
    }

    fn voxel_volume(
        &self,
        asset_id: &str,
        path: &str,
    ) -> Result<&'a rusty_engine::voxel_asset::VoxelAsset, StoredProjectError> {
        let id = AssetId::parse(asset_id).map_err(|error| {
            StoredProjectError::new(diagnostic_code::INVALID_ASSET_ID, path, error.to_string())
        })?;
        if id.kind() != AssetKind::VoxelVolume {
            return Err(StoredProjectError::new(
                diagnostic_code::WRONG_ASSET_KIND,
                path,
                format!(
                    "voxel environment requires `voxel-volume`, found `{}`",
                    id.kind()
                ),
            ));
        }
        let Some(asset) = self.assets.get(id.as_str()) else {
            return Err(StoredProjectError::new(
                diagnostic_code::MISSING_ASSET,
                path,
                format!("asset `{id}` is not declared in `assets`"),
            ));
        };
        asset.voxel_volume.as_ref().ok_or_else(|| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VOXEL_ASSET,
                path,
                format!("catalog entry `{id}` has no embedded voxelVolume artifact"),
            )
        })
    }
}

fn index_entities(
    scene: &StoredScene,
    scene_index: usize,
) -> Result<BTreeMap<EntityId, usize>, StoredProjectError> {
    let mut indexes = BTreeMap::new();
    for (entity_index, entity) in scene.entities.iter().enumerate() {
        let id = EntityId::new(entity.id);
        if let Some(first) = indexes.insert(id, entity_index) {
            return Err(StoredProjectError::new(
                diagnostic_code::DUPLICATE_ENTITY,
                format!("scenes[{scene_index}].entities[{entity_index}].id"),
                format!(
                    "entity {} was already declared at scenes[{scene_index}].entities[{first}].id",
                    entity.id
                ),
            ));
        }
    }
    Ok(indexes)
}

fn require_spatial_source(
    scene: &StoredScene,
    scene_index: usize,
) -> Result<(), StoredProjectError> {
    if scene.voxel_environment.is_some() {
        return Ok(());
    }
    if let Some((entity_index, _)) = scene
        .entities
        .iter()
        .enumerate()
        .find(|(_, entity)| entity.kinematic.is_some() || entity.navigation.is_some())
    {
        return Err(StoredProjectError::new(
            diagnostic_code::INVALID_SPATIAL,
            format!("scenes[{scene_index}].entities[{entity_index}].kinematic"),
            "kinematic/navigation components require a voxel environment",
        ));
    }
    Ok(())
}

fn build_collision_scene(
    scene: &StoredScene,
    scene_index: usize,
    catalog: &ProjectAssetCatalog<'_>,
) -> Result<Option<VoxelCollisionScene>, StoredProjectError> {
    let Some(environment) = &scene.voxel_environment else {
        return Ok(None);
    };
    let result = match environment {
        StoredVoxelEnvironment::Solid(environment) => VoxelCollisionScene::from_solid_voxels(
            environment.voxel_size,
            environment.chunk_size,
            environment.solid_voxels.iter().copied(),
        ),
        StoredVoxelEnvironment::Material(environment) => {
            let voxels = expand_material_voxels(environment, scene_index, catalog)?;
            VoxelCollisionScene::from_material_voxels(
                environment.voxel_size,
                environment.chunk_size,
                voxels,
            )
        }
        StoredVoxelEnvironment::GeneratedRoom(environment) => {
            VoxelCollisionScene::from_generated_room(GeneratedRoomConfig {
                seed: environment.seed,
                voxel_size: environment.voxel_size,
                chunk_size: environment.chunk_size,
                width: environment.width,
                height: environment.height,
                length: environment.length,
            })
        }
    };
    result.map(Some).map_err(|error| {
        StoredProjectError::new(
            diagnostic_code::INVALID_SPATIAL,
            format!("scenes[{scene_index}].voxelEnvironment"),
            error.to_string(),
        )
    })
}

fn expand_material_voxels(
    environment: &StoredMaterialVoxelEnvironment,
    scene_index: usize,
    catalog: &ProjectAssetCatalog<'_>,
) -> Result<Vec<MaterialVoxel>, StoredProjectError> {
    let mut voxels = Vec::with_capacity(environment.material_voxels.len());
    for (voxel_index, voxel) in environment.material_voxels.iter().enumerate() {
        let voxel = MaterialVoxel {
            address: voxel.address,
            material_slot: voxel.material_slot,
        };
        if let Err(error) = validate_material_voxel(voxel) {
            let (path, message) = match error {
                VoxelAuthorityValidationError::CoordinateOutOfBounds {
                    axis, limit, ..
                } => (
                    format!(
                        "scenes[{scene_index}].voxelEnvironment.materialVoxels[{voxel_index}].address[{axis}]"
                    ),
                    format!("voxel coordinate must stay within +/-{limit}"),
                ),
                VoxelAuthorityValidationError::InvalidMaterialSlot {
                    material_slot,
                    maximum,
                } => (
                    format!(
                        "scenes[{scene_index}].voxelEnvironment.materialVoxels[{voxel_index}].materialSlot"
                    ),
                    format!(
                        "material slot {material_slot} must be between 1 and {maximum}; zero is empty"
                    ),
                ),
            };
            return Err(StoredProjectError::new(
                diagnostic_code::INVALID_SPATIAL,
                path,
                message,
            ));
        }
        voxels.push(voxel);
    }
    for (reference_index, asset_id) in environment.voxel_assets.iter().enumerate() {
        let path = format!("scenes[{scene_index}].voxelEnvironment.voxelAssets[{reference_index}]");
        let asset = catalog.voxel_volume(asset_id, &path)?;
        if asset.grid.cell_size.to_bits() != environment.voxel_size.to_bits()
            || asset.grid.chunk_size != environment.chunk_size
        {
            return Err(StoredProjectError::new(
                diagnostic_code::INVALID_VOXEL_ASSET,
                path,
                format!(
                    "asset grid cellSize/chunkSize ({}/{}) must match environment ({}/{})",
                    asset.grid.cell_size,
                    asset.grid.chunk_size,
                    environment.voxel_size,
                    environment.chunk_size
                ),
            ));
        }
        for run in &asset.representation.sparse_runs {
            for offset in 0..run.length {
                voxels.push(MaterialVoxel {
                    address: [
                        asset.grid.origin[0]
                            .checked_add(run.start[0])
                            .and_then(|value| value.checked_add(i64::from(offset)))
                            .expect("validated voxel asset x address"),
                        asset.grid.origin[1]
                            .checked_add(run.start[1])
                            .expect("validated voxel asset y address"),
                        asset.grid.origin[2]
                            .checked_add(run.start[2])
                            .expect("validated voxel asset z address"),
                    ],
                    material_slot: run.material_slot,
                });
            }
        }
    }
    Ok(voxels)
}

fn authored_definition(
    authored: &StoredEntityDefinition,
    scene_index: usize,
    entity_index: usize,
) -> Result<GameEntityDefinition, StoredProjectError> {
    let entity = EntityId::new(authored.id);
    let path =
        |component: &str| format!("scenes[{scene_index}].entities[{entity_index}].{component}");
    let initial_translation = authored.translation.map(array_vec3);
    let mut entity_definition = EntityDefinition::new(entity, authored.name.clone());
    if initial_translation.is_some()
        || authored.parent.is_some()
        || authored.rotation != [0.0, 0.0, 0.0, 1.0]
        || authored.scale != [1.0; 3]
        || authored.light.is_some()
        || authored.bounds.is_some()
        || authored.pickup.is_some()
    {
        entity_definition = entity_definition.with_full_transform(EntityTransform {
            translation: initial_translation.unwrap_or(Vec3::ZERO),
            rotation: Quat::new(
                authored.rotation[0],
                authored.rotation[1],
                authored.rotation[2],
                authored.rotation[3],
            ),
            scale: array_vec3(authored.scale),
        });
    }
    if let Some(parent) = authored.parent {
        entity_definition = entity_definition.with_transform_parent(EntityId::new(parent));
    }
    if let Some(bounds) = authored.bounds {
        entity_definition =
            entity_definition.with_bounds(array_vec3(bounds.min), array_vec3(bounds.max));
    }
    if let Some(collision) = authored.collision {
        entity_definition =
            entity_definition.with_collision(collision.enabled, collision.static_collider);
    }
    if let Some(renderable) = &authored.renderable {
        entity_definition =
            entity_definition.with_renderable(renderable.asset.clone(), renderable.visible);
        if let Some(transform) = renderable.local_transform {
            entity_definition =
                entity_definition.with_renderable_local_transform(EntityTransform {
                    translation: array_vec3(transform.translation),
                    rotation: Quat::new(
                        transform.rotation[0],
                        transform.rotation[1],
                        transform.rotation[2],
                        transform.rotation[3],
                    ),
                    scale: array_vec3(transform.scale),
                });
        }
    }
    if let Some(kinematic) = authored.kinematic {
        entity_definition = entity_definition.with_kinematic(
            array_vec3(kinematic.half_extents),
            array_vec3(kinematic.velocity),
        );
    }

    let mut definition = GameEntityDefinition::new(entity_definition);
    if let Some(door) = &authored.door {
        let Some(closed_translation) = initial_translation else {
            return Err(StoredProjectError::new(
                diagnostic_code::INVALID_COMPONENT,
                path("door"),
                "door requires an initial translation",
            ));
        };
        let open_translation = array_vec3(door.open_translation);
        if !translation_is_valid(open_translation) {
            return Err(StoredProjectError::new(
                diagnostic_code::INVALID_COMPONENT,
                path("door.openTranslation"),
                "door open translation is invalid",
            ));
        }
        let auto_close_after = match door.auto_close_after_ticks {
            Some(0) => {
                return Err(StoredProjectError::new(
                    diagnostic_code::INVALID_COMPONENT,
                    path("door.autoCloseAfterTicks"),
                    "auto-close duration must be greater than zero",
                ));
            }
            Some(ticks) => Some(TickDelta::new(ticks)),
            None => None,
        };
        if door.motion_duration_ticks == 0 {
            return Err(StoredProjectError::new(
                diagnostic_code::INVALID_COMPONENT,
                path("door.motionDurationTicks"),
                "door motion duration must be greater than zero",
            ));
        }
        definition = definition.as_door(
            DoorConfig::new(closed_translation, open_translation, auto_close_after)
                .with_motion_duration(TickDelta::new(door.motion_duration_ticks)),
        );
        if let Some(access) = &door.access {
            definition = definition.with_door_access(DoorAccessConfig {
                required_key: parse_item_id(
                    &access.required_key,
                    &path("door.access.requiredKey"),
                )?,
                key_policy: match access.key_policy {
                    crate::StoredRequiredKeyPolicy::Retain => RequiredKeyPolicy::Retain,
                    crate::StoredRequiredKeyPolicy::Consume => RequiredKeyPolicy::Consume,
                },
                activation_radius: access.activation_radius,
                denied_presentation: access.denied_presentation.clone(),
            });
        }
    }
    if let Some(switch) = &authored.switch {
        let config = SwitchConfig {
            activation_radius: switch.activation_radius,
            prompt: switch.prompt.clone(),
            unavailable_presentation: switch.unavailable_presentation.clone(),
            repeatable: switch.repeatable,
            effects: switch
                .effects
                .iter()
                .map(|effect| match effect {
                    crate::StoredSwitchEffect::OpenDoor { door } => {
                        SwitchEffect::OpenDoor(EntityId::new(*door))
                    }
                    crate::StoredSwitchEffect::CloseDoor { door } => {
                        SwitchEffect::CloseDoor(EntityId::new(*door))
                    }
                })
                .collect(),
        };
        definition = definition
            .as_switch()
            .with_switch_config(config)
            .controls(switch.controls.iter().copied().map(EntityId::new));
        if let Some(interlock) = switch.loading_bay_interlock {
            definition = definition.with_loading_bay_interlock(LoadingBayInterlockConfig {
                close_door: EntityId::new(interlock.close_door),
                open_door: EntityId::new(interlock.open_door),
            });
        }
    }
    if let Some(floor_action) = &authored.floor_action {
        definition = definition.with_floor_action(FloorActionConfig::new(
            EntityId::new(floor_action.target_platform),
            array_vec3(floor_action.upper_translation),
            array_vec3(floor_action.lowered_translation),
            TickDelta::new(floor_action.motion_duration_ticks),
            floor_action.prompt.clone(),
            floor_action.presentation.clone(),
            floor_action.source.clone(),
        ));
    }
    if let Some(lift) = &authored.lift {
        definition = definition.with_lift(
            LiftConfig::new(
                EntityId::new(lift.target_platform),
                array_vec3(lift.raised_translation),
                array_vec3(lift.lowered_translation),
                TickDelta::new(lift.motion_duration_ticks),
                TickDelta::new(lift.lowered_wait_ticks),
            )
            .with_metadata(
                lift.prompt.clone(),
                lift.presentation.clone(),
                lift.source.clone(),
            ),
        );
    }
    if authored.enemy {
        definition = definition.as_enemy();
    }
    if let Some(combat) = &authored.enemy_combat {
        definition = definition.with_enemy_combat(EnemyCombatConfig {
            perception: EnemyPerceptionConfig {
                sight_range: combat.sight_range,
                hearing_range: combat.hearing_range,
            },
            attack: EnemyAttackConfig {
                kind: match combat.attack.kind {
                    crate::StoredEnemyAttackKind::Melee => EnemyAttackKind::Melee,
                    crate::StoredEnemyAttackKind::RangedHitscan => EnemyAttackKind::RangedHitscan,
                },
                damage: combat.attack.damage,
                range: combat.attack.range,
                cooldown_ticks: combat.attack.cooldown_ticks,
                origin_offset: array_vec3(combat.attack.origin_offset),
                presentation: combat.attack.presentation.clone(),
            },
        });
    }
    if let Some(drop) = authored.defeat_drop {
        definition = definition.with_enemy_drop(EnemyDropConfig {
            pickup: EntityId::new(drop.pickup),
        });
    }
    if let Some(health) = authored.health {
        definition = definition.with_health(HealthConfig {
            max: health.max,
            starting: health.starting_health.unwrap_or(health.max),
            hitbox_half_extents: array_vec3(health.hitbox_half_extents),
            max_armor: health.max_armor,
            armor_absorption_percent: health.armor_absorption_percent,
        });
    }
    if let Some(hazard) = authored.hazard {
        definition = definition.as_hazard(HazardConfig {
            damage: hazard.damage,
            cooldown_ticks: hazard.cooldown_ticks,
        });
    }
    if let Some(encounter) = &authored.encounter {
        definition = definition.as_encounter(
            encounter.members.iter().copied().map(EntityId::new),
            EntityId::new(encounter.exit),
        );
        definition = definition.with_encounter_activation_radius(encounter.activation_radius);
    }
    if let Some(beacon) = authored.extraction_beacon {
        definition = definition
            .with_extraction_beacon(ExtractionBeaconConfig::new(beacon.activation_radius));
    }
    if let Some(navigation) = authored.navigation {
        definition = definition.with_navigation(NavigationConfig {
            goal: array_vec3(navigation.goal),
            speed_units_per_second: navigation.speed_units_per_second,
            max_visited: navigation.max_visited,
        });
    }
    if let Some(controller) = &authored.player_controller {
        definition = definition.with_player_controller(PlayerControllerConfig {
            move_speed_units_per_second: controller.move_speed_units_per_second,
            move_step_seconds: controller.move_step_seconds,
            look_degrees_per_unit: controller.look_degrees_per_unit,
            initial_yaw_degrees: controller.initial_yaw_degrees,
            initial_pitch_degrees: controller.initial_pitch_degrees,
            traversal: crate::PlayerTraversalConfig {
                max_step_height: controller.traversal.max_step_height,
                gravity_units_per_second_squared: controller
                    .traversal
                    .gravity_units_per_second_squared,
                jump_impulse_units_per_second: controller.traversal.jump_impulse_units_per_second,
                ground_probe_distance: controller.traversal.ground_probe_distance,
                eye_height: controller.traversal.eye_height,
                manual_jump_enabled: controller.traversal.manual_jump_enabled,
                max_air_jumps: controller.traversal.max_air_jumps,
            },
            bindings: {
                let mut bindings = PlayerInputBindings::new(
                    controller.bindings.move_forward.clone(),
                    controller.bindings.move_backward.clone(),
                    controller.bindings.move_left.clone(),
                    controller.bindings.move_right.clone(),
                    controller.bindings.mouse_look.clone(),
                    controller.bindings.primary_fire.clone(),
                    controller.bindings.select_weapon.clone(),
                );
                bindings.jump = controller.bindings.jump.clone();
                bindings
            },
        });
    }
    if let Some(inventory) = &authored.inventory {
        definition = definition.with_inventory(InventoryConfig::new(
            inventory.capacity_slots,
            inventory
                .starting_stacks
                .iter()
                .enumerate()
                .map(|(stack_index, stack)| {
                    Ok(InventoryStack::new(
                        parse_item_id(
                            &stack.item,
                            &format!("{}.startingStacks[{stack_index}].item", path("inventory")),
                        )?,
                        stack.quantity,
                    ))
                })
                .collect::<Result<Vec<_>, StoredProjectError>>()?,
            inventory
                .initially_equipped_weapon
                .as_deref()
                .map(|item| {
                    parse_item_id(
                        item,
                        &format!("{}.initiallyEquippedWeapon", path("inventory")),
                    )
                })
                .transpose()?,
            inventory
                .weapon_slots
                .iter()
                .enumerate()
                .map(|(slot_index, item)| {
                    parse_item_id(
                        item,
                        &format!("{}.weaponSlots[{slot_index}]", path("inventory")),
                    )
                })
                .collect::<Result<Vec<_>, StoredProjectError>>()?,
        ));
    }
    if let Some(pickup) = &authored.pickup {
        definition = definition.as_pickup(
            PickupConfig::new(
                parse_item_id(&pickup.item, &format!("{}.item", path("pickup")))?,
                pickup.quantity,
            )
            .with_starter_ammunition(
                pickup
                    .starter_ammunition
                    .as_ref()
                    .map(|starter| {
                        Ok(InventoryStack::new(
                            parse_item_id(
                                &starter.item,
                                &format!("{}.starterAmmunition.item", path("pickup")),
                            )?,
                            starter.quantity,
                        ))
                    })
                    .transpose()?,
            ),
        );
    }
    if let Some(secret) = &authored.secret_region {
        definition = definition.as_secret_region(SecretRegionConfig {
            presentation: secret.presentation.clone(),
        });
    }
    if let Some(exit) = &authored.level_exit {
        definition = definition.as_level_exit(LevelExitConfig {
            activation_radius: exit.activation_radius,
            presentation: exit.presentation.clone(),
        });
    }
    if let Some(weapon) = authored.weapon {
        definition = definition.with_weapon(WeaponConfig {
            damage: weapon.damage,
            max_distance: weapon.max_distance,
            cooldown_ticks: weapon.cooldown_ticks,
            ammo_capacity: weapon.ammo_capacity,
            muzzle_offset: array_vec3(weapon.muzzle_offset),
        });
    }
    Ok(definition)
}

fn definition_error(
    error: GameEntityDefinitionError,
    scene_index: usize,
    indexes: &BTreeMap<EntityId, usize>,
) -> StoredProjectError {
    use GameEntityDefinitionError as Error;

    let (code, path) = match &error {
        Error::Mechanics { .. } => (
            diagnostic_code::INVALID_COMPONENT,
            format!("scenes[{scene_index}].entities"),
        ),
        Error::Inventory(source) => inventory_error_path(source, scene_index, indexes),
        Error::EntityState(source) => match source {
            rusty_engine::entity_state::EntityDefinitionError::DuplicateEntity { entity } => (
                diagnostic_code::DUPLICATE_ENTITY,
                entity_path(scene_index, indexes, *entity, "id"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::EmptyName { entity } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "name"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::DuplicateLabel {
                entity, ..
            } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "labels"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::InvalidSource { entity } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "source"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::InvalidTransform { entity } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "translation"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::InvalidRenderableTransform {
                entity,
            } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "renderable.localTransform"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::InvalidBounds { entity } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "bounds"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::EmptyAsset { entity } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "renderable.asset"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::KinematicMissingTransform {
                entity,
            }
            | rusty_engine::entity_state::EntityDefinitionError::InvalidKinematicHalfExtents {
                entity,
            }
            | rusty_engine::entity_state::EntityDefinitionError::InvalidKinematicVelocity {
                entity,
            } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "kinematic"),
            ),
            rusty_engine::entity_state::EntityDefinitionError::InvalidRelationship {
                entity,
                ..
            } => (
                diagnostic_code::INVALID_RELATIONSHIP,
                entity_path(scene_index, indexes, *entity, "relationships"),
            ),
        },
        Error::DuplicateControlTarget { switch, .. }
        | Error::UnknownControlTarget { switch, .. }
        | Error::ControlTargetIsNotDoor { switch, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *switch, "switch.controls"),
        ),
        Error::ControlsWithoutSwitch { entity } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *entity, "switch"),
        ),
        Error::SwitchConfigWithoutSwitch { entity } | Error::InvalidSwitchConfig { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "switch"),
        ),
        Error::DuplicateSwitchEffect { switch, .. }
        | Error::UnknownSwitchEffectTarget { switch, .. }
        | Error::SwitchEffectTargetIsNotDoor { switch, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *switch, "switch.effects"),
        ),
        Error::FloorActionMissingTransform { entity }
        | Error::FloorActionMissingBounds { entity }
        | Error::InvalidFloorActionConfig { entity }
        | Error::FloorActionConflictsWithGameplayOwner { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "floorAction"),
        ),
        Error::UnknownFloorActionTarget { action, .. }
        | Error::FloorActionTargetMissingTransform { action, .. }
        | Error::FloorActionTargetMissingCollision { action, .. }
        | Error::FloorActionTargetMustBeMovable { action, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *action, "floorAction.targetPlatform"),
        ),
        Error::LiftMissingTransform { entity }
        | Error::LiftMissingBounds { entity }
        | Error::InvalidLiftConfig { entity }
        | Error::LiftConflictsWithGameplayOwner { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "lift"),
        ),
        Error::UnknownLiftTarget { lift, .. }
        | Error::LiftTargetMissingTransform { lift, .. }
        | Error::LiftTargetMissingCollision { lift, .. }
        | Error::LiftTargetMustBeMovable { lift, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *lift, "lift.targetPlatform"),
        ),
        Error::DuplicateMovingPlatformTarget { second_owner, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *second_owner, "targetPlatform"),
        ),
        Error::DoorMissingTransform { entity }
        | Error::DoorMissingCollision { entity }
        | Error::DoorMustBeMovable { entity }
        | Error::DoorMissingRenderable { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "door"),
        ),
        Error::DoorAccessWithoutDoor { entity }
        | Error::InvalidDoorAccessConfig { entity }
        | Error::DoorAccessKeyMissingDefinition { entity }
        | Error::DoorAccessKeyNotAccessKey { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "door.access"),
        ),
        Error::LoadingBayInterlockWithoutSwitch { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "switch.loadingBayInterlock"),
        ),
        Error::InvalidLoadingBayInterlock { switch, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *switch, "switch.loadingBayInterlock"),
        ),
        Error::EnemyMissingCollision { entity } | Error::EnemyMissingRenderable { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "enemy"),
        ),
        Error::EnemyCombatWithoutEnemy { entity }
        | Error::EnemyCombatMissingTransform { entity }
        | Error::EnemyCombatMissingHealth { entity }
        | Error::EnemyCombatMissingNavigation { entity }
        | Error::InvalidEnemyCombatConfig { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "enemyCombat"),
        ),
        Error::HealthMissingTransform { entity }
        | Error::HealthMissingCollision { entity }
        | Error::InvalidHealthConfig { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "health"),
        ),
        Error::HazardMissingTransform { entity }
        | Error::HazardMissingBounds { entity }
        | Error::HazardMissingRenderable { entity }
        | Error::InvalidHazardConfig { entity }
        | Error::HazardConflictsWithGameplayOwner { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "hazard"),
        ),
        Error::NavigationWithoutEnemy { entity }
        | Error::NavigationMissingTransform { entity }
        | Error::NavigationMissingCollision { entity }
        | Error::NavigationMissingKinematic { entity }
        | Error::InvalidNavigationGoal { entity }
        | Error::InvalidNavigationSpeed { entity }
        | Error::InvalidNavigationQueryBudget { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "navigation"),
        ),
        Error::PlayerControllerMissingTransform { entity }
        | Error::PlayerControllerMissingCollision { entity }
        | Error::PlayerControllerMissingKinematic { entity }
        | Error::PlayerControllerMissingRenderable { entity }
        | Error::InvalidPlayerControllerConfig { entity }
        | Error::WeaponBindingSlotMismatch { entity, .. } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "playerController"),
        ),
        Error::PickupMissingTransform { entity }
        | Error::PickupMissingBounds { entity }
        | Error::PickupMissingRenderable { entity }
        | Error::PickupMissingItemDefinition { entity }
        | Error::InvalidPickupQuantity { entity }
        | Error::PickupConflictsWithGameplayOwner { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "pickup"),
        ),
        Error::TooManyPickups { .. } => (
            diagnostic_code::INVALID_COMPONENT,
            format!("scenes[{scene_index}].entities"),
        ),
        Error::WeaponWithoutPlayerController { entity }
        | Error::InvalidWeaponConfig { entity }
        | Error::LegacyEntityWeapon { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "weapon"),
        ),
        Error::InvalidPickupStarterAmmunition { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "pickup.starterAmmunition"),
        ),
        Error::EmptyEncounter { encounter }
        | Error::EncounterActivationMissingTransform { encounter }
        | Error::InvalidEncounterActivationRadius { encounter }
        | Error::DuplicateEncounterMember { encounter, .. }
        | Error::UnknownEncounterMember { encounter, .. }
        | Error::EncounterMemberIsNotEnemy { encounter, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *encounter, "encounter.members"),
        ),
        Error::UnknownEncounterExit { encounter, .. }
        | Error::EncounterExitIsNotDoor { encounter, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *encounter, "encounter.exit"),
        ),
        Error::EnemyInMultipleEncounters { second, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *second, "encounter.members"),
        ),
        Error::EnemyDropWithoutEnemy { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "defeatDrop"),
        ),
        Error::UnknownEnemyDropPickup { enemy, .. }
        | Error::EnemyDropTargetIsNotPickup { enemy, .. }
        | Error::EnemyDropPickupVisibleAtStart { enemy, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *enemy, "defeatDrop.pickup"),
        ),
        Error::PickupUsedByMultipleEnemyDrops { second, .. } => (
            diagnostic_code::INVALID_RELATIONSHIP,
            entity_path(scene_index, indexes, *second, "defeatDrop.pickup"),
        ),
        Error::ExtractionBeaconMissingTransform { entity }
        | Error::ExtractionBeaconMissingRenderable { entity }
        | Error::InvalidExtractionBeaconConfig { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "extractionBeacon"),
        ),
        Error::SecretRegionMissingTransform { entity }
        | Error::SecretRegionMissingBounds { entity }
        | Error::InvalidSecretRegionConfig { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "secretRegion"),
        ),
        Error::LevelExitMissingTransform { entity }
        | Error::LevelExitMissingRenderable { entity }
        | Error::InvalidLevelExitConfig { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "levelExit"),
        ),
    };
    StoredProjectError::new(code, path, error.to_string())
}

fn inventory_error_path(
    error: &crate::inventory::InventoryAdmissionError,
    scene_index: usize,
    indexes: &BTreeMap<EntityId, usize>,
) -> (&'static str, String) {
    let owner = match error {
        crate::inventory::InventoryAdmissionError::InventoryWithoutPlayerController { owner }
        | crate::inventory::InventoryAdmissionError::InvalidCapacity { owner, .. }
        | crate::inventory::InventoryAdmissionError::TooManyStartingStacks { owner, .. }
        | crate::inventory::InventoryAdmissionError::DuplicateStartingStack { owner, .. }
        | crate::inventory::InventoryAdmissionError::MissingStartingDefinition { owner, .. }
        | crate::inventory::InventoryAdmissionError::InvalidStartingQuantity { owner, .. }
        | crate::inventory::InventoryAdmissionError::InvalidInitialSelection { owner, .. } => {
            Some(*owner)
        }
        _ => None,
    };
    (
        diagnostic_code::INVALID_COMPONENT,
        owner.map_or_else(
            || "itemDefinitions".to_string(),
            |owner| entity_path(scene_index, indexes, owner, "inventory"),
        ),
    )
}

fn parse_item_id(value: &str, path: &str) -> Result<ItemDefinitionId, StoredProjectError> {
    ItemDefinitionId::parse(value.to_string()).map_err(|error| {
        StoredProjectError::new(diagnostic_code::INVALID_VALUE, path, error.to_string())
    })
}

fn entity_path(
    scene_index: usize,
    indexes: &BTreeMap<EntityId, usize>,
    entity: EntityId,
    suffix: &str,
) -> String {
    indexes.get(&entity).map_or_else(
        || format!("scenes[{scene_index}].entities"),
        |index| format!("scenes[{scene_index}].entities[{index}].{suffix}"),
    )
}

fn array_vec3(value: [f32; 3]) -> Vec3 {
    Vec3::new(value[0], value[1], value[2])
}

fn translation_is_valid(value: Vec3) -> bool {
    value.x.is_finite()
        && value.y.is_finite()
        && value.z.is_finite()
        && value.x.abs() <= MAX_ABS_TRANSLATION
        && value.y.abs() <= MAX_ABS_TRANSLATION
        && value.z.abs() <= MAX_ABS_TRANSLATION
}
