//! Product-specific static resources and dynamic readouts for the browser shell.

use core_ids::EntityId;
use core_math::Vec3;
use loading_bay_game::{
    DoorState, EncounterState, EnemyAttackKind, EnemyCombatPosture, EnemyState,
    ExtractionBeaconState, GameRuntime, ItemKind, LevelExitState, NavigationState,
    PickupCollectionCause, PickupState, PlayerInputSessionView, RequiredKeyPolicy,
    SecretRegionState, VitalityState, LOADING_BAY_INTERLOCK_ACTIVATION_RADIUS,
};
use serde::Serialize;

use super::presentation::{project_presentation, BrowserFeedbackProjection, BrowserPresentation};
use super::{
    BrowserRuntime, ACTOR, BEACON, ENCOUNTER, EXIT, FIRST_ENEMY, MOTION_PROBE, SECOND_ENEMY,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserProjectionNode {
    id: u64,
    name: String,
    asset: String,
    translation: Option<[f32; 3]>,
    visible: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserEnemyState {
    id: u64,
    name: String,
    state: &'static str,
    position: [f32; 3],
    current_health: u32,
    max_health: u32,
    combat_posture: Option<&'static str>,
    attack_kind: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserPlayerBindings {
    move_forward: String,
    move_backward: String,
    move_left: String,
    move_right: String,
    mouse_look: String,
    primary_fire: String,
    select_weapon: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserPlayerState {
    id: u64,
    position: [f32; 3],
    yaw_degrees: f32,
    pitch_degrees: f32,
    move_step_seconds: f32,
    look_degrees_per_unit: f32,
    bindings: BrowserPlayerBindings,
    current_health: u32,
    max_health: u32,
    armor: u32,
    max_armor: u32,
    vitality_state: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserHazardState {
    id: u64,
    damage: u32,
    cooldown_ticks: u64,
    ready_at_tick: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserRestartState {
    authored_baseline_available: bool,
    checkpoint_available: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserWeaponState {
    item: String,
    presentation: String,
    damage: u32,
    ammunition: String,
    ammunition_cost: u32,
    ammo_remaining: u32,
    ammo_capacity: u32,
    ready_at_tick: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserInventoryWeapon {
    slot: usize,
    item: String,
    owned: bool,
    selected: bool,
    ammunition: String,
    ammunition_quantity: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserInventoryStack {
    item: String,
    quantity: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserInventoryState {
    owner: u64,
    capacity_slots: usize,
    stacks: Vec<BrowserInventoryStack>,
    equipped_weapon: Option<String>,
    weapons: Vec<BrowserInventoryWeapon>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserPickupState {
    id: u64,
    item: String,
    quantity: u32,
    state: &'static str,
    collected_by: Option<u64>,
    collected_at_tick: Option<u64>,
    collection_cause: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserExtractionBeaconState {
    id: u64,
    state: &'static str,
    activation_radius: f32,
    activated_by: Option<u64>,
    activated_at_tick: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserDoorAccessState {
    id: u64,
    state: &'static str,
    required_key: String,
    key_policy: &'static str,
    activation_radius: f32,
    denied_presentation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserSecretRegionState {
    id: u64,
    state: &'static str,
    presentation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserLevelExitState {
    id: u64,
    state: &'static str,
    activation_radius: f32,
    presentation: String,
    completed_by: Option<u64>,
    completed_at_tick: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserInteractionState {
    target: u64,
    prompt: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserVoxelMeshGroup {
    material_slot: u16,
    start: u32,
    count: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserVoxelMeshChunk {
    chunk: [i64; 3],
    content_hash: String,
    translation: [f32; 3],
    positions: Vec<f32>,
    normals: Vec<f32>,
    indices: Vec<u32>,
    groups: Vec<BrowserVoxelMeshGroup>,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserGeneratedEnvironment {
    seed: u64,
    output_hash: String,
    solid_voxels: usize,
    mesh_vertices: u32,
    mesh_quads: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BrowserState {
    #[serde(flatten)]
    pub(super) dynamic: BrowserDynamicState,
    #[serde(flatten)]
    pub(super) resources: BrowserStaticResources,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BrowserDynamicState {
    tick: u64,
    entity_revision: u64,
    projection: Vec<BrowserProjectionNode>,
    door_state: &'static str,
    encounter_state: &'static str,
    motion_state: &'static str,
    navigation_state: &'static str,
    player_motion_state: &'static str,
    combat_state: &'static str,
    input: PlayerInputSessionView,
    player: BrowserPlayerState,
    weapon: BrowserWeaponState,
    inventory: Option<BrowserInventoryState>,
    pickups: Vec<BrowserPickupState>,
    hazards: Vec<BrowserHazardState>,
    restart: BrowserRestartState,
    extraction_beacon: Option<BrowserExtractionBeaconState>,
    door_access: Vec<BrowserDoorAccessState>,
    secret_regions: Vec<BrowserSecretRegionState>,
    level_exits: Vec<BrowserLevelExitState>,
    level_complete: bool,
    interaction: Option<BrowserInteractionState>,
    enemies: Vec<BrowserEnemyState>,
    presentation: BrowserPresentation,
    pub(super) last_events: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BrowserStaticResources {
    host_session_id: String,
    static_revision: String,
    voxel_revision: u64,
    voxel_authority_hash: String,
    voxel_solid_count: usize,
    voxel_navigation_hash: String,
    voxel_probe_path_length: usize,
    voxel_meshes: Vec<BrowserVoxelMeshChunk>,
    generated_environment: Option<BrowserGeneratedEnvironment>,
}

pub(super) fn browser_state(
    host: &BrowserRuntime,
    last_events: Vec<String>,
    feedback: BrowserFeedbackProjection,
) -> BrowserState {
    BrowserState {
        dynamic: browser_dynamic_state(host, last_events, feedback),
        resources: browser_static_resources(host),
    }
}

pub(super) fn browser_dynamic_state(
    host: &BrowserRuntime,
    last_events: Vec<String>,
    feedback: BrowserFeedbackProjection,
) -> BrowserDynamicState {
    let runtime: &GameRuntime = host.runtime.runtime();
    let readout = runtime.readout();
    let projection = readout
        .projection
        .into_iter()
        .map(|node| BrowserProjectionNode {
            id: node.entity.raw(),
            name: node.name,
            asset: node.asset,
            translation: node.translation.map(|value| value.to_array()),
            visible: node.visible,
        })
        .collect();
    let enemies = [FIRST_ENEMY, SECOND_ENEMY]
        .into_iter()
        .map(|raw| {
            let view = runtime
                .session()
                .enemy(EntityId::new(raw))
                .expect("browser enemy");
            let combat = runtime.session().enemy_combat(EntityId::new(raw));
            BrowserEnemyState {
                id: raw,
                name: view.entity_view.name,
                state: match view.state {
                    EnemyState::Alive => "alive",
                    EnemyState::Defeated => "defeated",
                },
                position: view
                    .entity_view
                    .transform
                    .expect("browser enemy transform")
                    .translation
                    .to_array(),
                current_health: runtime
                    .session()
                    .health(EntityId::new(raw))
                    .expect("browser enemy health")
                    .current,
                max_health: runtime
                    .session()
                    .health(EntityId::new(raw))
                    .expect("browser enemy health")
                    .config
                    .max,
                combat_posture: combat.as_ref().map(|combat| match combat.state.posture {
                    EnemyCombatPosture::Sleeping => "sleeping",
                    EnemyCombatPosture::Alert => "alert",
                    EnemyCombatPosture::Pursuing => "pursuing",
                    EnemyCombatPosture::Attacking => "attacking",
                    EnemyCombatPosture::Dead => "dead",
                }),
                attack_kind: combat
                    .as_ref()
                    .map(|combat| match combat.config.attack.kind {
                        EnemyAttackKind::Melee => "melee",
                        EnemyAttackKind::RangedHitscan => "rangedHitscan",
                    }),
            }
        })
        .collect();
    let player = runtime
        .session()
        .player_controller(ACTOR)
        .expect("browser player controller");
    let bindings = &player.config.bindings;
    let player_vitality = runtime.session().health(ACTOR);
    let (current_health, max_health, armor, max_armor, vitality_state) = player_vitality
        .as_ref()
        .map_or((0, 0, 0, 0, "alive"), |health| {
            (
                health.current,
                health.config.max,
                health.armor,
                health.config.max_armor,
                match health.state {
                    VitalityState::Alive => "alive",
                    VitalityState::Dead => "dead",
                },
            )
        });
    let player_state = BrowserPlayerState {
        id: ACTOR.raw(),
        position: player
            .entity_view
            .transform
            .expect("browser player transform")
            .translation
            .to_array(),
        yaw_degrees: player.state.yaw_degrees,
        pitch_degrees: player.state.pitch_degrees,
        move_step_seconds: player.config.move_step_seconds,
        look_degrees_per_unit: player.config.look_degrees_per_unit,
        bindings: BrowserPlayerBindings {
            move_forward: bindings.move_forward.clone(),
            move_backward: bindings.move_backward.clone(),
            move_left: bindings.move_left.clone(),
            move_right: bindings.move_right.clone(),
            mouse_look: bindings.mouse_look.clone(),
            primary_fire: bindings.primary_fire.clone(),
            select_weapon: bindings.select_weapon.clone(),
        },
        current_health,
        max_health,
        armor,
        max_armor,
        vitality_state,
    };
    let weapon = runtime
        .session()
        .weapon(ACTOR)
        .expect("browser player weapon");
    let weapon_state = BrowserWeaponState {
        item: weapon.item.as_str().to_owned(),
        presentation: weapon.definition.presentation.clone(),
        damage: weapon.definition.damage,
        ammunition: weapon.definition.ammunition.as_str().to_owned(),
        ammunition_cost: weapon.definition.ammunition_cost,
        ammo_remaining: runtime
            .session()
            .inventory(ACTOR)
            .and_then(|inventory| {
                inventory
                    .stacks
                    .into_iter()
                    .find(|stack| stack.item == weapon.definition.ammunition)
                    .map(|stack| stack.quantity)
            })
            .unwrap_or(0),
        ammo_capacity: runtime
            .session()
            .item_definition(&weapon.definition.ammunition)
            .map_or(0, |definition| definition.max_quantity),
        ready_at_tick: weapon.state.ready_at_tick.raw(),
    };
    let inventory_state =
        runtime
            .session()
            .inventory(ACTOR)
            .map(|inventory| BrowserInventoryState {
                owner: inventory.owner.raw(),
                capacity_slots: inventory.capacity_slots,
                stacks: inventory
                    .stacks
                    .iter()
                    .map(|stack| BrowserInventoryStack {
                        item: stack.item.as_str().to_owned(),
                        quantity: stack.quantity,
                    })
                    .collect(),
                equipped_weapon: inventory
                    .equipped_weapon
                    .as_ref()
                    .map(|item| item.as_str().to_owned()),
                weapons: inventory
                    .weapon_slots
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, item)| {
                        let definition = runtime.session().item_definition(item)?;
                        let ItemKind::Weapon(weapon) = definition.kind else {
                            return None;
                        };
                        Some(BrowserInventoryWeapon {
                            slot,
                            item: item.as_str().to_owned(),
                            owned: inventory.stacks.iter().any(|stack| stack.item == *item),
                            selected: inventory.equipped_weapon.as_ref() == Some(item),
                            ammunition: weapon.ammunition.as_str().to_owned(),
                            ammunition_quantity: inventory
                                .stacks
                                .iter()
                                .find(|stack| stack.item == weapon.ammunition)
                                .map_or(0, |stack| stack.quantity),
                        })
                    })
                    .collect(),
            });
    let door_access = runtime
        .session()
        .door_accesses()
        .map(|access| {
            let door = runtime
                .session()
                .door(access.door)
                .expect("admitted keyed door");
            BrowserDoorAccessState {
                id: access.door.raw(),
                state: match door.state {
                    DoorState::Closed => "closed",
                    DoorState::Open => "open",
                },
                required_key: access.config.required_key.as_str().to_owned(),
                key_policy: match access.config.key_policy {
                    RequiredKeyPolicy::Retain => "retain",
                    RequiredKeyPolicy::Consume => "consume",
                },
                activation_radius: access.config.activation_radius,
                denied_presentation: access.config.denied_presentation,
            }
        })
        .collect::<Vec<_>>();
    let secret_regions = runtime
        .session()
        .secret_regions()
        .map(|secret| BrowserSecretRegionState {
            id: secret.entity.raw(),
            state: match secret.state {
                SecretRegionState::Undiscovered => "undiscovered",
                SecretRegionState::Discovered { .. } => "discovered",
            },
            presentation: secret.config.presentation,
        })
        .collect::<Vec<_>>();
    let level_exits = runtime
        .session()
        .level_exits()
        .map(|exit| {
            let (state, completed_by, completed_at_tick) = match exit.state {
                LevelExitState::Available => ("available", None, None),
                LevelExitState::Completed {
                    actor,
                    completed_at,
                } => ("completed", Some(actor.raw()), Some(completed_at.raw())),
            };
            BrowserLevelExitState {
                id: exit.entity.raw(),
                state,
                activation_radius: exit.config.activation_radius,
                presentation: exit.config.presentation,
                completed_by,
                completed_at_tick,
            }
        })
        .collect::<Vec<_>>();
    let interaction = available_interaction(runtime, &player_state, inventory_state.as_ref());
    let pickups = runtime
        .session()
        .pickups()
        .map(|pickup| {
            let (state, collected_by, collected_at_tick, collection_cause) = match pickup.state {
                PickupState::Dormant => ("dormant", None, None, None),
                PickupState::Available => ("available", None, None, None),
                PickupState::Collected {
                    actor,
                    collected_at_tick,
                    cause,
                } => (
                    "collected",
                    Some(actor.raw()),
                    Some(collected_at_tick),
                    Some(match cause {
                        PickupCollectionCause::Overlap { .. } => "overlap",
                        PickupCollectionCause::Interaction { .. } => "interaction",
                    }),
                ),
            };
            BrowserPickupState {
                id: pickup.entity.raw(),
                item: pickup.config.item.as_str().to_owned(),
                quantity: pickup.config.quantity,
                state,
                collected_by,
                collected_at_tick,
                collection_cause,
            }
        })
        .collect();
    let extraction_beacon = runtime.session().extraction_beacon(BEACON).map(|beacon| {
        let (state, activated_by, activated_at_tick) = match beacon.state {
            ExtractionBeaconState::Standby => ("standby", None, None),
            ExtractionBeaconState::Active {
                actor,
                activated_at,
            } => ("active", Some(actor.raw()), Some(activated_at.raw())),
        };
        BrowserExtractionBeaconState {
            id: beacon.entity.raw(),
            state,
            activation_radius: beacon.config.activation_radius,
            activated_by,
            activated_at_tick,
        }
    });
    let hazards = runtime
        .session()
        .hazards()
        .map(|hazard| BrowserHazardState {
            id: hazard.entity.raw(),
            damage: hazard.config.damage,
            cooldown_ticks: hazard.config.cooldown_ticks,
            ready_at_tick: hazard.ready_at_tick.raw(),
        })
        .collect();
    let player_motion_state = if last_events.iter().any(|event| event == "PlayerBlocked") {
        "blocked"
    } else if last_events.iter().any(|event| event == "PlayerMoved") {
        "moved"
    } else {
        "idle"
    };
    let combat_state = if last_events.iter().any(|event| event == "CombatHit") {
        "hit"
    } else if last_events
        .iter()
        .any(|event| event.starts_with("CombatMissed"))
    {
        "missed"
    } else {
        "ready"
    };
    BrowserDynamicState {
        tick: readout.tick.raw(),
        entity_revision: readout.entity_revision,
        projection,
        door_state: match runtime.session().door(EXIT).expect("exit door").state {
            DoorState::Closed => "closed",
            DoorState::Open => "open",
        },
        encounter_state: match runtime
            .session()
            .encounter(ENCOUNTER)
            .expect("browser encounter")
            .state
        {
            EncounterState::Dormant => "dormant",
            EncounterState::Active => "active",
            EncounterState::Cleared => "cleared",
        },
        motion_state: if runtime
            .session()
            .entity(MOTION_PROBE)
            .expect("motion probe")
            .kinematic
            .expect("motion capability")
            .velocity
            .x
            == 0.0
        {
            "blocked"
        } else {
            "moving"
        },
        navigation_state: match runtime
            .session()
            .navigation(EntityId::new(FIRST_ENEMY))
            .expect("browser navigator")
            .state
        {
            NavigationState::Following => "following",
            NavigationState::Arrived => "arrived",
            NavigationState::Blocked => "blocked",
            NavigationState::Unreachable => "unreachable",
        },
        player_motion_state,
        combat_state,
        input: host.runtime.input_session(),
        player: player_state,
        weapon: weapon_state,
        inventory: inventory_state,
        pickups,
        hazards,
        restart: BrowserRestartState {
            authored_baseline_available: true,
            checkpoint_available: false,
        },
        extraction_beacon,
        door_access,
        secret_regions,
        level_exits,
        level_complete: runtime.is_level_complete(),
        interaction,
        enemies,
        presentation: project_presentation(
            runtime,
            ACTOR,
            &[EntityId::new(FIRST_ENEMY), EntityId::new(SECOND_ENEMY)],
            EXIT,
            BEACON,
            feedback,
        ),
        last_events,
    }
}

fn available_interaction(
    runtime: &GameRuntime,
    player: &BrowserPlayerState,
    inventory: Option<&BrowserInventoryState>,
) -> Option<BrowserInteractionState> {
    if player.vitality_state == "dead" || runtime.is_level_complete() {
        return None;
    }
    let player_position = Vec3::new(player.position[0], player.position[1], player.position[2]);
    let mut candidates = Vec::new();
    for access in runtime.session().door_accesses() {
        let door = runtime
            .session()
            .door(access.door)
            .expect("admitted keyed door");
        if door.state == DoorState::Open {
            continue;
        }
        let translation = door
            .entity_view
            .transform
            .expect("admitted keyed door transform")
            .translation;
        let distance_squared = (player_position - translation).length_squared();
        if distance_squared > access.config.activation_radius * access.config.activation_radius {
            continue;
        }
        let owns_key = inventory.is_some_and(|inventory| {
            inventory.stacks.iter().any(|stack| {
                stack.item == access.config.required_key.as_str() && stack.quantity > 0
            })
        });
        candidates.push((
            distance_squared,
            access.door,
            if owns_key {
                format!("Open {}", door.entity_view.name.replace('-', " "))
            } else {
                access.config.denied_presentation
            },
        ));
    }
    for interlock in runtime.session().loading_bay_interlocks() {
        let translation = interlock
            .entity_view
            .transform
            .expect("admitted Loading Bay interlock transform")
            .translation;
        let distance_squared = (player_position - translation).length_squared();
        if distance_squared
            <= LOADING_BAY_INTERLOCK_ACTIVATION_RADIUS * LOADING_BAY_INTERLOCK_ACTIVATION_RADIUS
        {
            candidates.push((
                distance_squared,
                interlock.switch,
                format!("Activate {}", interlock.entity_view.name.replace('-', " ")),
            ));
        }
    }
    for exit in runtime.session().level_exits() {
        if exit.state != LevelExitState::Available {
            continue;
        }
        let translation = exit
            .entity_view
            .transform
            .expect("admitted level exit transform")
            .translation;
        let distance_squared = (player_position - translation).length_squared();
        if distance_squared <= exit.config.activation_radius * exit.config.activation_radius {
            candidates.push((
                distance_squared,
                exit.entity,
                format!("Use {}", exit.entity_view.name.replace('-', " ")),
            ));
        }
    }
    candidates.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    candidates
        .into_iter()
        .next()
        .map(|(_, target, prompt)| BrowserInteractionState {
            target: target.raw(),
            prompt,
        })
}

pub(super) fn browser_static_revision(host: &BrowserRuntime) -> String {
    let scene = host
        .runtime
        .runtime()
        .collision_scene()
        .expect("browser project collision scene");
    format!(
        "{}:{:016x}",
        scene.source_revision().raw(),
        scene.authority_hash()
    )
}

pub(super) fn browser_static_resources(host: &BrowserRuntime) -> BrowserStaticResources {
    let runtime: &GameRuntime = host.runtime.runtime();
    let scene = runtime
        .collision_scene()
        .expect("browser project collision scene");
    let voxel_meshes = scene
        .mesh_chunks()
        .iter()
        .map(|mesh| BrowserVoxelMeshChunk {
            chunk: mesh.chunk,
            content_hash: format!("{:016x}", mesh.content_hash),
            translation: mesh.translation,
            positions: mesh.positions.clone(),
            normals: mesh.normals.clone(),
            indices: mesh.indices.clone(),
            groups: mesh
                .groups
                .iter()
                .map(|group| BrowserVoxelMeshGroup {
                    material_slot: group.material_slot,
                    start: group.start,
                    count: group.count,
                })
                .collect(),
            bounds_min: mesh.bounds_min,
            bounds_max: mesh.bounds_max,
        })
        .collect();
    let generated_environment = scene.generated_room().map(|(config, record)| {
        let mesh_vertices = scene.mesh_chunks().iter().map(|mesh| mesh.vertices).sum();
        let mesh_quads = scene.mesh_chunks().iter().map(|mesh| mesh.quads).sum();
        BrowserGeneratedEnvironment {
            seed: config.seed,
            output_hash: format!("{:016x}", record.output_hash),
            solid_voxels: record.solid_voxel_count,
            mesh_vertices,
            mesh_quads,
        }
    });
    BrowserStaticResources {
        host_session_id: host.host_session_id.clone(),
        static_revision: browser_static_revision(host),
        voxel_revision: scene.source_revision().raw(),
        voxel_authority_hash: format!("{:016x}", scene.authority_hash()),
        voxel_solid_count: scene.solid_voxel_count(),
        voxel_navigation_hash: format!("{:016x}", scene.navigation_hash()),
        voxel_probe_path_length: scene
            .navigation_step(
                Vec3::new(1.5, 1.5, 6.5),
                Vec3::new(7.5, 1.5, 6.5),
                Vec3::ZERO,
                0.1,
                512,
            )
            .map_or(0, |step| step.path_len),
        voxel_meshes,
        generated_environment,
    }
}
