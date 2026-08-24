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
use rusty_engine::entity_state::{EntityDefinition, EntityTransform};

use crate::definition::{GameEntityDefinition, GameEntityDefinitionError};
use crate::door::DoorConfig;
use crate::encounter_program::compile_encounter_programs;
use crate::enemy_combat::{
    EnemyAttackConfig, EnemyAttackKind, EnemyCombatConfig, EnemyPerceptionConfig,
};
use crate::enemy_drop::EnemyDropConfig;
use crate::enemy_program::{compile_enemy_attack_programs, compile_enemy_defeat_programs};
use crate::explosive_prop::ExplosivePropConfig;
use crate::explosive_prop_program::compile_explosive_prop_programs;
use crate::extraction_beacon::ExtractionBeaconConfig;
use crate::floor_action::FloorActionConfig;
use crate::floor_action_program::compile_floor_action_programs;
use crate::gameplay_program::compile_gameplay_programs;
use crate::hazard::HazardConfig;
use crate::hazard_program::compile_hazard_programs;
use crate::interaction::{SwitchConfig, SwitchEffect};
use crate::inventory::{
    ArmorGrantMode, ArmorTransition, InventoryStack, ItemDefinition, ItemDefinitionId, ItemKind,
    ProjectileDefinition, WeaponAttackMode, WeaponDefinition,
};
use crate::level_exit_program::compile_level_exit_programs;
use crate::lift::LiftConfig;
use crate::lift_program::compile_lift_programs;
use crate::navigation::NavigationConfig;
use crate::pickup::PickupConfig;
use crate::pickup_program::{
    compile_pickup_programs, pickup_program_is_compatible, PickupProgramCatalog,
};
use crate::player::{PlayerControllerConfig, PlayerInputBindings};
use crate::player_program::{
    compile_player_setup_programs, resolve_player_setup_program, PlayerSetupProgramCatalog,
};
use crate::progression::{
    DoorAccessConfig, LevelExitConfig, LoadingBayInterlockConfig, RequiredKeyPolicy,
    SecretRegionConfig,
};
use crate::project_codec::decode_project_document;
use crate::secret_program::compile_secret_programs;
use crate::session::{GameSession, SessionProgramCatalogs};
use crate::standard_vitality::DoomVitalityPolicy;
use crate::stored_project::{
    diagnostic_code, validate_stored_project, StoredAsset, StoredItemDefinition, StoredItemKind,
    StoredMaterialVoxel, StoredMaterialVoxelEnvironment, StoredProject, StoredProjectError,
    StoredScene, StoredVoxelEnvironment,
};
use crate::switch_program::compile_switch_programs;
use crate::vitality::HealthConfig;

/// Static project data admitted into Rust-owned runtime state. It is the
/// construction token shared by current project decode and product load.
#[derive(Debug)]
pub struct AdmittedProject {
    pub session: GameSession,
    pub collision_scene: Option<VoxelCollisionScene>,
}

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
    admit_stored_project_with_document_and_vitality_policy(
        document,
        DoomVitalityPolicy::doom_compatibility(),
    )
}

/// Product hosts with an admitted standard vitality extension pass the typed
/// policy through this single construction route. The compatibility policy is
/// deliberately reserved for direct gameplay construction and old snapshots
/// that have no package-admission edge.
pub fn admit_stored_project_with_document_and_vitality_policy(
    document: StoredProject,
    vitality_policy: DoomVitalityPolicy,
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
    let gameplay_programs =
        compile_gameplay_programs(&document.gameplay_programs).map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VALUE,
                "gameplayPrograms",
                error.to_string(),
            )
        })?;
    let pickup_programs = compile_pickup_programs(&document.pickup_programs).map_err(|error| {
        StoredProjectError::new(
            diagnostic_code::INVALID_VALUE,
            "pickupPrograms",
            error.to_string(),
        )
    })?;
    let player_setup_programs = compile_player_setup_programs(&document.player_setup_programs)
        .map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VALUE,
                "playerSetupPrograms",
                error.to_string(),
            )
        })?;
    let enemy_attack_programs = compile_enemy_attack_programs(&document.enemy_attack_programs)
        .map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VALUE,
                "enemyAttackPrograms",
                error.to_string(),
            )
        })?;
    let enemy_defeat_programs = compile_enemy_defeat_programs(&document.enemy_defeat_programs)
        .map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VALUE,
                "enemyDefeatPrograms",
                error.to_string(),
            )
        })?;
    let hazard_programs = compile_hazard_programs(&document.hazard_programs).map_err(|error| {
        StoredProjectError::new(
            diagnostic_code::INVALID_VALUE,
            "hazardPrograms",
            error.to_string(),
        )
    })?;
    let explosive_prop_programs =
        compile_explosive_prop_programs(&document.explosive_prop_programs).map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VALUE,
                "explosivePropPrograms",
                error.to_string(),
            )
        })?;
    let encounter_programs =
        compile_encounter_programs(&document.encounter_programs).map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VALUE,
                "encounterPrograms",
                error.to_string(),
            )
        })?;
    let switch_programs = compile_switch_programs(&document.switch_programs).map_err(|error| {
        StoredProjectError::new(
            diagnostic_code::INVALID_VALUE,
            "switchPrograms",
            error.to_string(),
        )
    })?;
    let floor_action_programs = compile_floor_action_programs(&document.floor_action_programs)
        .map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VALUE,
                "floorActionPrograms",
                error.to_string(),
            )
        })?;
    let lift_programs = compile_lift_programs(&document.lift_programs).map_err(|error| {
        StoredProjectError::new(
            diagnostic_code::INVALID_VALUE,
            "liftPrograms",
            error.to_string(),
        )
    })?;
    let secret_programs = compile_secret_programs(&document.secret_programs).map_err(|error| {
        StoredProjectError::new(
            diagnostic_code::INVALID_VALUE,
            "secretPrograms",
            error.to_string(),
        )
    })?;
    let level_exit_programs =
        compile_level_exit_programs(&document.level_exit_programs).map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_VALUE,
                "levelExitPrograms",
                error.to_string(),
            )
        })?;
    let program_catalogs = AdmissionProgramCatalogs {
        gameplay: gameplay_programs,
        pickup: pickup_programs,
        player_setup: player_setup_programs,
        enemy_attack: enemy_attack_programs,
        enemy_defeat: enemy_defeat_programs,
        hazard: hazard_programs,
        explosive_prop: explosive_prop_programs,
        encounter: encounter_programs,
        switch: switch_programs,
        floor_action: floor_action_programs,
        lift: lift_programs,
        secret: secret_programs,
        level_exit: level_exit_programs,
    };

    validate_program_bindings(&document, &item_definitions, &program_catalogs)?;

    let mut entry_scene = None;
    for (scene_index, scene) in document.scenes.iter().enumerate() {
        let admitted = admit_scene(
            scene,
            scene_index,
            &catalog,
            &item_definitions,
            &program_catalogs,
            vitality_policy,
        )?;
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

/// Program families compiled once for project admission, then cloned only when
/// constructing an independent runtime scene.
struct AdmissionProgramCatalogs {
    gameplay: crate::gameplay_program::GameplayProgramCatalog,
    pickup: PickupProgramCatalog,
    player_setup: PlayerSetupProgramCatalog,
    enemy_attack: crate::enemy_program::EnemyAttackProgramCatalog,
    enemy_defeat: crate::enemy_program::EnemyDefeatProgramCatalog,
    hazard: crate::hazard_program::HazardProgramCatalog,
    explosive_prop: crate::explosive_prop_program::ExplosivePropProgramCatalog,
    encounter: crate::encounter_program::EncounterProgramCatalog,
    switch: crate::switch_program::SwitchProgramCatalog,
    floor_action: crate::floor_action_program::FloorActionProgramCatalog,
    lift: crate::lift_program::LiftProgramCatalog,
    secret: crate::secret_program::SecretProgramCatalog,
    level_exit: crate::level_exit_program::LevelExitProgramCatalog,
}

fn admit_scene(
    scene: &StoredScene,
    scene_index: usize,
    catalog: &ProjectAssetCatalog<'_>,
    item_definitions: &[ItemDefinition],
    program_catalogs: &AdmissionProgramCatalogs,
    vitality_policy: DoomVitalityPolicy,
) -> Result<AdmittedScene, StoredProjectError> {
    catalog.validate_scene(scene, scene_index)?;

    let entity_indexes = index_entities(scene, scene_index)?;
    require_spatial_source(scene, scene_index)?;
    let collision_scene = build_collision_scene(scene, scene_index, catalog)?;
    // The Engine authored-scene document is the sole generic-scene source:
    // every node becomes a runtime entity, and each entity record overlays
    // downstream bindings onto its node by id.
    let document = crate::stored_project::decoded_authored_scene(scene, scene_index)?;
    let item_definition_map = item_definitions
        .iter()
        .cloned()
        .map(|definition| (definition.id.clone(), definition))
        .collect::<BTreeMap<_, _>>();
    let mut bindings = BTreeMap::new();
    for (entity_index, entity) in scene.entities.iter().enumerate() {
        if bindings.insert(entity.id, entity_index).is_some() {
            return Err(StoredProjectError::new(
                diagnostic_code::DUPLICATE_ENTITY,
                format!("scenes[{scene_index}].entities[{entity_index}].id"),
                format!("entity {} was already declared", entity.id),
            ));
        }
    }
    let definitions = document
        .nodes
        .iter()
        .map(|node| {
            let binding_index = bindings.get(&node.id.raw()).copied();
            authored_definition(
                node,
                binding_index.map(|index| &scene.entities[index]),
                scene_index,
                binding_index.unwrap_or(0),
                &item_definition_map,
                &program_catalogs.player_setup,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let player_setup_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .inventory
                .as_ref()
                .map(|inventory| inventory.setup_program.clone())
                .map(|program| (EntityId::new(entity.id), program))
        })
        .collect();
    let hazard_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .hazard
                .as_ref()
                .map(|hazard| (EntityId::new(entity.id), hazard.program.clone()))
        })
        .collect();
    let explosive_prop_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .explosive_prop
                .as_ref()
                .map(|prop| (EntityId::new(entity.id), prop.program.clone()))
        })
        .collect();
    let encounter_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .encounter
                .as_ref()
                .map(|encounter| (EntityId::new(entity.id), encounter.program.clone()))
        })
        .collect();
    let switch_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .switch
                .as_ref()
                .map(|switch| (EntityId::new(entity.id), switch.program.clone()))
        })
        .collect();
    let floor_action_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .floor_action
                .as_ref()
                .map(|floor_action| (EntityId::new(entity.id), floor_action.program.clone()))
        })
        .collect();
    let lift_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .lift
                .as_ref()
                .map(|lift| (EntityId::new(entity.id), lift.program.clone()))
        })
        .collect();
    let secret_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .secret_region
                .as_ref()
                .map(|secret| (EntityId::new(entity.id), secret.program.clone()))
        })
        .collect();
    let level_exit_bindings = scene
        .entities
        .iter()
        .filter_map(|entity| {
            entity
                .level_exit
                .as_ref()
                .map(|exit| (EntityId::new(entity.id), exit.program.clone()))
        })
        .collect();
    let session = GameSession::from_item_entity_and_gameplay_programs(
        item_definitions.iter().cloned(),
        definitions,
        SessionProgramCatalogs {
            gameplay: program_catalogs.gameplay.clone(),
            pickup: program_catalogs.pickup.clone(),
            player_setup: program_catalogs.player_setup.clone(),
            player_setup_bindings,
            enemy_attack: program_catalogs.enemy_attack.clone(),
            enemy_defeat: program_catalogs.enemy_defeat.clone(),
            hazard: program_catalogs.hazard.clone(),
            hazard_bindings,
            explosive_prop: program_catalogs.explosive_prop.clone(),
            explosive_prop_bindings,
            encounter: program_catalogs.encounter.clone(),
            encounter_bindings,
            switch: program_catalogs.switch.clone(),
            switch_bindings,
            floor_action: program_catalogs.floor_action.clone(),
            floor_action_bindings,
            lift: program_catalogs.lift.clone(),
            lift_bindings,
            secret: program_catalogs.secret.clone(),
            secret_bindings,
            level_exit: program_catalogs.level_exit.clone(),
            level_exit_bindings,
        },
        vitality_policy,
    )
    .map_err(|error| definition_error(error, scene_index, &entity_indexes))?;

    Ok(AdmittedScene {
        session,
        collision_scene,
    })
}

pub fn authored_item_definition(
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
        } => ItemKind::Weapon(WeaponDefinition {
            attack_mode: match attack_mode.expect("validated current weapon attack mode") {
                crate::StoredWeaponAttackMode::Hitscan => WeaponAttackMode::Hitscan,
                crate::StoredWeaponAttackMode::Spread => WeaponAttackMode::Spread {
                    pellet_count: pellet_count.expect("validated current weapon pellet count"),
                    spread_degrees: spread_degrees.expect("validated current weapon spread angle"),
                },
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
            // Current project admission has no player-projectile route. This
            // field remains solely for retained legacy snapshot decoding.
            projectile: None,
        }),
        StoredItemKind::Ammunition => ItemKind::Ammunition,
        StoredItemKind::AccessKey => ItemKind::AccessKey,
        StoredItemKind::HealthSupply {
            restore_health,
            maximum_health,
            automatic_use,
            consume_at_cap,
        } => ItemKind::HealthSupply {
            restore_health: *restore_health,
            maximum_health: *maximum_health,
            automatic_use: *automatic_use,
            consume_at_cap: *consume_at_cap,
        },
        StoredItemKind::Armor {
            protection,
            maximum_armor,
            absorption_percent,
            absorption_divisor,
            grant_mode,
            transition,
            consume_at_cap,
        } => ItemKind::Armor {
            protection: *protection,
            maximum_armor: *maximum_armor,
            absorption_percent: *absorption_percent,
            absorption_divisor: *absorption_divisor,
            grant_mode: match grant_mode {
                crate::StoredArmorGrantMode::Add => ArmorGrantMode::Add,
                crate::StoredArmorGrantMode::SetMinimum => ArmorGrantMode::SetMinimum,
            },
            transition: match transition {
                crate::StoredArmorTransition::RejectDifferent => ArmorTransition::RejectDifferent,
                crate::StoredArmorTransition::Preserve => ArmorTransition::Preserve,
                crate::StoredArmorTransition::Replace => ArmorTransition::Replace,
            },
            consume_at_cap: *consume_at_cap,
        },
    };
    Ok(ItemDefinition::new(id, kind, authored.max_quantity).with_program(authored.program.clone()))
}

fn validate_program_bindings(
    document: &StoredProject,
    item_definitions: &[ItemDefinition],
    program_catalogs: &AdmissionProgramCatalogs,
) -> Result<(), StoredProjectError> {
    for (index, item) in document.item_definitions.iter().enumerate() {
        if let Some(program) = &item.program {
            if program_catalogs.gameplay.get(program).is_none() {
                return Err(StoredProjectError::new(
                    diagnostic_code::INVALID_VALUE,
                    format!("itemDefinitions[{index}].program"),
                    format!(
                        "item `{}` references unknown gameplay program `{program}`",
                        item.id
                    ),
                ));
            }
        }
    }
    for (scene_index, scene) in document.scenes.iter().enumerate() {
        // Labels live on Engine scene nodes; resolve them for diagnostics.
        let node_labels = crate::stored_project::decoded_authored_scene(scene, scene_index)
            .map(|document| {
                document
                    .nodes
                    .iter()
                    .map(|node| (node.id.raw(), node.metadata.label.clone()))
                    .collect::<BTreeMap<u64, Option<String>>>()
            })
            .unwrap_or_default();
        let label_of = |entity_id: u64| -> String {
            node_labels
                .get(&entity_id)
                .cloned()
                .flatten()
                .unwrap_or_else(|| format!("entity {entity_id}"))
        };
        for (entity_index, entity) in scene.entities.iter().enumerate() {
            if let Some(inventory) = &entity.inventory {
                let path = format!(
                    "scenes[{scene_index}].entities[{entity_index}].inventory.setupProgram"
                );
                if program_catalogs
                    .player_setup
                    .get(&inventory.setup_program)
                    .is_none()
                {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "inventory `{}` references missing or wrong-family player setup program `{}`",
                            label_of(entity.id), inventory.setup_program
                        ),
                    ));
                }
            }
            if let Some(pickup) = &entity.pickup {
                let path = format!("scenes[{scene_index}].entities[{entity_index}].pickup.program");
                let program = program_catalogs
                    .pickup
                    .get(&pickup.program)
                    .ok_or_else(|| {
                        StoredProjectError::new(
                            diagnostic_code::INVALID_VALUE,
                            &path,
                            format!(
                            "pickup `{}` references missing or wrong-family pickup program `{}`",
                            label_of(entity.id), pickup.program
                        ),
                        )
                    })?;
                let item = item_definitions
                    .iter()
                    .find(|item| item.id.as_str() == pickup.item)
                    .ok_or_else(|| {
                        StoredProjectError::new(
                            diagnostic_code::INVALID_VALUE,
                            &path,
                            format!(
                                "pickup `{}` references missing item `{}`",
                                label_of(entity.id),
                                pickup.item
                            ),
                        )
                    })?;
                if !pickup_program_is_compatible(
                    program,
                    &item.kind,
                    pickup.starter_ammunition.is_some(),
                ) {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "pickup `{}` program `{}` is incompatible with `{}`",
                            label_of(entity.id),
                            pickup.program,
                            pickup.item
                        ),
                    ));
                }
            }
            if let Some(hazard) = &entity.hazard {
                let path = format!("scenes[{scene_index}].entities[{entity_index}].hazard.program");
                if program_catalogs.hazard.get(&hazard.program).is_none() {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "hazard `{}` references missing or wrong-family hazard program `{}`",
                            label_of(entity.id),
                            hazard.program
                        ),
                    ));
                }
            }
            if let Some(prop) = &entity.explosive_prop {
                let path =
                    format!("scenes[{scene_index}].entities[{entity_index}].explosiveProp.program");
                if program_catalogs.explosive_prop.get(&prop.program).is_none() {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "explosive prop `{}` references missing or wrong-family explosive-prop program `{}`",
                            label_of(entity.id), prop.program
                        ),
                    ));
                }
            }
            if let Some(encounter) = &entity.encounter {
                let path =
                    format!("scenes[{scene_index}].entities[{entity_index}].encounter.program");
                if program_catalogs.encounter.get(&encounter.program).is_none() {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "encounter `{}` references missing or wrong-family encounter program `{}`",
                            label_of(entity.id), encounter.program
                        ),
                    ));
                }
            }
            if let Some(switch) = &entity.switch {
                let path = format!("scenes[{scene_index}].entities[{entity_index}].switch.program");
                if program_catalogs.switch.get(&switch.program).is_none() {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "switch `{}` references missing or wrong-family switch program `{}`",
                            label_of(entity.id),
                            switch.program
                        ),
                    ));
                }
            }
            if let Some(floor_action) = &entity.floor_action {
                let path =
                    format!("scenes[{scene_index}].entities[{entity_index}].floorAction.program");
                if program_catalogs
                    .floor_action
                    .get(&floor_action.program)
                    .is_none()
                {
                    return Err(StoredProjectError::new(diagnostic_code::INVALID_VALUE, path, format!("floor action `{}` references missing or wrong-family floor-action program `{}`", label_of(entity.id), floor_action.program)));
                }
            }
            if let Some(lift) = &entity.lift {
                let path = format!("scenes[{scene_index}].entities[{entity_index}].lift.program");
                if program_catalogs.lift.get(&lift.program).is_none() {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "lift `{}` references missing or wrong-family lift program `{}`",
                            label_of(entity.id),
                            lift.program
                        ),
                    ));
                }
            }
            if let Some(secret) = &entity.secret_region {
                let path =
                    format!("scenes[{scene_index}].entities[{entity_index}].secretRegion.program");
                if program_catalogs.secret.get(&secret.program).is_none() {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "secret region `{}` references missing or wrong-family secret program `{}`",
                            label_of(entity.id), secret.program
                        ),
                    ));
                }
            }
            if let Some(exit) = &entity.level_exit {
                let path =
                    format!("scenes[{scene_index}].entities[{entity_index}].levelExit.program");
                if program_catalogs.level_exit.get(&exit.program).is_none() {
                    return Err(StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        path,
                        format!(
                            "level exit `{}` references missing or wrong-family level-exit program `{}`",
                            label_of(entity.id), exit.program
                        ),
                    ));
                }
            }
            let Some(combat) = &entity.enemy_combat else {
                continue;
            };
            if program_catalogs
                .enemy_attack
                .get(&combat.attack_program)
                .is_none()
            {
                return Err(StoredProjectError::new(
                    diagnostic_code::INVALID_VALUE,
                    format!(
                        "scenes[{scene_index}].entities[{entity_index}].enemyCombat.attackProgram"
                    ),
                    format!(
                        "enemy `{}` references missing or wrong-family attack program `{}`",
                        label_of(entity.id),
                        combat.attack_program
                    ),
                ));
            }
            if program_catalogs
                .enemy_defeat
                .get(&combat.defeat_program)
                .is_none()
            {
                return Err(StoredProjectError::new(
                    diagnostic_code::INVALID_VALUE,
                    format!(
                        "scenes[{scene_index}].entities[{entity_index}].enemyCombat.defeatProgram"
                    ),
                    format!(
                        "enemy `{}` references missing or wrong-family defeat program `{}`",
                        label_of(entity.id),
                        combat.defeat_program
                    ),
                ));
            }
        }
    }
    if document.project_id == "doom-e1m1" {
        for id in [
            "weapon/fist",
            "weapon/pistol",
            "weapon/shotgun",
            "supply/health-bonus",
            "supply/medikit",
            "supply/stimpack",
        ] {
            let item = document
                .item_definitions
                .iter()
                .find(|item| item.id == id)
                .ok_or_else(|| {
                    StoredProjectError::new(
                        diagnostic_code::INVALID_VALUE,
                        "itemDefinitions",
                        format!("E1M1 is missing required item `{id}`"),
                    )
                })?;
            if item.program.is_none() {
                return Err(StoredProjectError::new(
                    diagnostic_code::INVALID_VALUE,
                    "itemDefinitions",
                    format!("E1M1 item `{id}` must bind a gameplay program"),
                ));
            }
        }
    }
    Ok(())
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

    /// Validates every renderable asset declared by the scene's Engine
    /// authored-scene nodes: the node kind owns the asset identity, so this
    /// walks nodes rather than entity binding records.
    fn validate_scene(
        &self,
        scene: &StoredScene,
        scene_index: usize,
    ) -> Result<(), StoredProjectError> {
        let document = crate::stored_project::decoded_authored_scene(scene, scene_index)?;
        for node in &document.nodes {
            let (rusty_engine::authored_scene::SceneNodeKind::StaticMesh(asset_reference)
            | rusty_engine::authored_scene::SceneNodeKind::AnimatedMesh(asset_reference)
            | rusty_engine::authored_scene::SceneNodeKind::Sprite(asset_reference)) = &node.kind
            else {
                continue;
            };
            let path = format!(
                "scenes[{scene_index}].authoredScene.nodes[{}].kind.asset",
                node.id.raw()
            );
            let id = AssetId::parse(asset_reference.id().as_str()).map_err(|error| {
                StoredProjectError::new(diagnostic_code::INVALID_ASSET_ID, &path, error.to_string())
            })?;
            if !matches!(
                id.kind(),
                AssetKind::StaticMesh | AssetKind::AnimatedMesh | AssetKind::Sprite
            ) {
                return Err(StoredProjectError::new(
                    diagnostic_code::WRONG_ASSET_KIND,
                    path,
                    format!(
                        "renderable requires static mesh, animated mesh, or sprite identity, found `{}`",
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
            if !matches!(
                kind,
                AssetKind::StaticMesh | AssetKind::AnimatedMesh | AssetKind::Sprite
            ) {
                return Err(StoredProjectError::new(
                    diagnostic_code::WRONG_ASSET_KIND,
                    path,
                    format!(
                        "catalog entry `{id}` is `{kind}`, expected static mesh, animated mesh, or sprite"
                    ),
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

/// Builds one runtime entity from its Engine scene node (the sole generic
/// source: label, hierarchy, transform, renderable asset) overlaid with the
/// downstream binding record keyed by the same id.
fn authored_definition(
    node: &rusty_engine::authored_scene::SceneNodeRecord,
    binding: Option<&crate::StoredEntityDefinition>,
    scene_index: usize,
    binding_index: usize,
    item_definitions: &BTreeMap<ItemDefinitionId, ItemDefinition>,
    player_setup_programs: &PlayerSetupProgramCatalog,
) -> Result<GameEntityDefinition, StoredProjectError> {
    use rusty_engine::authored_scene::SceneNodeKind;

    let entity = EntityId::new(node.id.raw());
    let path =
        |component: &str| format!("scenes[{scene_index}].entities[{binding_index}].{component}");
    let presentation = binding.and_then(|entity| entity.renderable.as_ref());
    // The Engine codec rejects blank labels, so admission only supplies a
    // fallback for nodes the Engine allowed to omit a label entirely.
    let label = node
        .metadata
        .label
        .clone()
        .unwrap_or_else(|| format!("scene-node-{}", node.id.raw()));
    let mut entity_definition = EntityDefinition::new(entity, label);
    // Every Engine scene node declares a transform, so every runtime entity
    // carries one — including identity transforms for origin nodes.
    let transform = &node.transform;
    entity_definition = entity_definition.with_full_transform(EntityTransform {
        translation: transform.translation,
        rotation: transform.rotation,
        scale: transform.scale,
    });
    if let Some(parent) = node.parent {
        entity_definition = entity_definition.with_transform_parent(EntityId::new(parent.raw()));
    }
    if let Some(bounds) = binding.and_then(|entity| entity.bounds) {
        entity_definition =
            entity_definition.with_bounds(array_vec3(bounds.min), array_vec3(bounds.max));
    }
    if let Some(collision) = binding.and_then(|entity| entity.collision) {
        entity_definition =
            entity_definition.with_collision(collision.enabled, collision.static_collider);
    }
    match &node.kind {
        SceneNodeKind::StaticMesh(asset) | SceneNodeKind::Sprite(asset) => {
            entity_definition = entity_definition.with_renderable(
                asset.id().as_str().to_owned(),
                presentation.is_none_or(|presentation| presentation.visible),
            );
        }
        SceneNodeKind::AnimatedMesh(asset) => {
            entity_definition = entity_definition.with_renderable(
                asset.id().as_str().to_owned(),
                presentation.is_none_or(|presentation| presentation.visible),
            );
        }
        // Lights, markers, voxel volumes, instances, and bootstrap nodes
        // carry no runtime renderable; lights are consumed by presentation
        // transports directly from the decoded document.
        _ => {}
    }
    if let Some(local_transform) = non_identity_renderable_transform(node.renderable_transform) {
        entity_definition = entity_definition.with_renderable_local_transform(EntityTransform {
            translation: local_transform.translation,
            rotation: local_transform.rotation,
            scale: local_transform.scale,
        });
    }
    if let Some(kinematic) = binding.and_then(|entity| entity.kinematic) {
        entity_definition = entity_definition.with_kinematic(
            array_vec3(kinematic.half_extents),
            array_vec3(kinematic.velocity),
        );
    }

    let Some(authored) = binding else {
        // A node without a binding record carries only generic scene
        // structure: label, transform, and any Engine-declared renderable.
        return Ok(GameEntityDefinition::new(entity_definition));
    };
    let mut definition = GameEntityDefinition::new(entity_definition);
    if let Some(door) = &authored.door {
        // The open translation is downstream door policy, but its spatial
        // validity is an Engine-owned invariant: route it through the
        // canonical Engine transform admission rules.
        if crate::stored_project::engine_transform_rejection(
            door.open_translation,
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
        )
        .is_some()
        {
            return Err(StoredProjectError::new(
                diagnostic_code::INVALID_COMPONENT,
                path("door.openTranslation"),
                "door open translation is outside the admitted spatial range",
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
            DoorConfig::new(
                transform.translation,
                array_vec3(door.open_translation),
                auto_close_after,
            )
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
            pain_duration_ticks: combat.pain_duration_ticks,
            attack_program: combat.attack_program.clone(),
            defeat_program: combat.defeat_program.clone(),
            attack: EnemyAttackConfig {
                kind: match combat.attack.kind {
                    crate::StoredEnemyAttackKind::Melee => EnemyAttackKind::Melee,
                    crate::StoredEnemyAttackKind::RangedHitscan => EnemyAttackKind::RangedHitscan,
                    crate::StoredEnemyAttackKind::Projectile => EnemyAttackKind::Projectile,
                },
                damage: combat.attack.damage,
                range: combat.attack.range,
                cooldown_ticks: combat.attack.cooldown_ticks,
                origin_offset: array_vec3(combat.attack.origin_offset),
                presentation: combat.attack.presentation.clone(),
                projectile_visual_asset: combat
                    .attack
                    .projectile
                    .as_ref()
                    .and_then(|projectile| projectile.visual_asset.clone()),
                projectile: combat.attack.projectile.as_ref().map(|projectile| {
                    ProjectileDefinition {
                        mass: projectile.mass,
                        radius: projectile.radius,
                        impulse: projectile.impulse,
                        gravity_scale: projectile.gravity_scale,
                        lifetime_ticks: projectile.lifetime_ticks,
                        restitution: projectile.restitution,
                    }
                }),
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
    if let Some(explosive_prop) = &authored.explosive_prop {
        definition = definition.with_explosive_prop(ExplosivePropConfig {
            damage: explosive_prop.damage,
            radius: explosive_prop.radius,
        });
    }
    if let Some(hazard) = &authored.hazard {
        definition = definition.as_hazard(HazardConfig {
            damage: hazard.damage,
            cooldown_ticks: hazard.cooldown_ticks,
        });
    }
    if let Some(encounter) = &authored.encounter {
        definition = definition.as_encounter(
            encounter.members.iter().copied().map(EntityId::new),
            encounter.exit.map(EntityId::new),
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
        let weapon_slots = inventory
            .weapon_slots
            .iter()
            .enumerate()
            .map(|(slot_index, item)| {
                parse_item_id(
                    item,
                    &format!("{}.weaponSlots[{slot_index}]", path("inventory")),
                )
            })
            .collect::<Result<Vec<_>, StoredProjectError>>()?;
        let program = player_setup_programs
            .get(&inventory.setup_program)
            .ok_or_else(|| {
                StoredProjectError::new(
                    diagnostic_code::INVALID_VALUE,
                    format!("{}.setupProgram", path("inventory")),
                    format!(
                        "missing admitted player setup program `{}`",
                        inventory.setup_program
                    ),
                )
            })?;
        let config = resolve_player_setup_program(
            program,
            inventory.capacity_slots,
            weapon_slots,
            item_definitions,
        )
        .map_err(|error| {
            StoredProjectError::new(
                diagnostic_code::INVALID_COMPONENT,
                format!("{}.setupProgram", path("inventory")),
                error.to_string(),
            )
        })?;
        definition = definition.with_inventory(config);
    }
    if let Some(pickup) = &authored.pickup {
        definition = definition.as_pickup(
            PickupConfig::new(
                parse_item_id(&pickup.item, &format!("{}.item", path("pickup")))?,
                pickup.quantity,
                pickup.program.clone(),
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
    Ok(definition)
}

/// Returns the node's renderable transform when it is not the identity, so
/// runtime entities only attach an explicit local transform component when
/// the authored scene declares one.
fn non_identity_renderable_transform(
    renderable_transform: rusty_engine::authored_scene::SceneTransform,
) -> Option<rusty_engine::authored_scene::SceneTransform> {
    (renderable_transform != rusty_engine::authored_scene::SceneTransform::IDENTITY)
        .then_some(renderable_transform)
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
            rusty_engine::entity_state::EntityDefinitionError::CharacterMotionMissingTransform {
                entity,
            }
            | rusty_engine::entity_state::EntityDefinitionError::CharacterMotionConflict {
                entity,
            }
            | rusty_engine::entity_state::EntityDefinitionError::InvalidCharacterMotion {
                entity,
            } => (
                diagnostic_code::INVALID_COMPONENT,
                entity_path(scene_index, indexes, *entity, "characterMotion"),
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
        | Error::InvalidEnemyCombatConfig { entity }
        | Error::MissingEnemyAttackProgram { entity }
        | Error::MissingEnemyDefeatProgram { entity }
        | Error::EnemyAttackProgramIncompatible { entity }
        | Error::EnemyDefeatProgramRequiresDrop { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "enemyCombat"),
        ),
        Error::HealthMissingTransform { entity }
        | Error::HealthMissingCollision { entity }
        | Error::InvalidHealthConfig { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "health"),
        ),
        Error::ExplosivePropOnEnemy { entity }
        | Error::ExplosivePropMissingTransform { entity }
        | Error::ExplosivePropMissingCollision { entity }
        | Error::ExplosivePropMissingRenderable { entity }
        | Error::ExplosivePropMissingHealth { entity }
        | Error::InvalidExplosivePropConfig { entity } => (
            diagnostic_code::INVALID_COMPONENT,
            entity_path(scene_index, indexes, *entity, "explosiveProp"),
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
