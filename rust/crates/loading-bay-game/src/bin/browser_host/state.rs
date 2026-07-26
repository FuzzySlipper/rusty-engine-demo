//! Product-specific static resources and dynamic readouts for the browser shell.

use core_ids::EntityId;
use core_math::Vec3;
use loading_bay_game::{
    DoorState, EncounterState, EnemyState, ExtractionBeaconState, GameRuntime, NavigationState,
    PlayerInputSessionView,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserWeaponState {
    damage: u32,
    ammo_remaining: u32,
    ammo_capacity: u32,
    ready_at_tick: u64,
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
    extraction_beacon: Option<BrowserExtractionBeaconState>,
    enemies: Vec<BrowserEnemyState>,
    presentation: BrowserPresentation,
    pub(super) last_events: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BrowserStaticResources {
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
            }
        })
        .collect();
    let player = runtime
        .session()
        .player_controller(ACTOR)
        .expect("browser player controller");
    let bindings = &player.config.bindings;
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
        },
    };
    let weapon = runtime
        .session()
        .weapon(ACTOR)
        .expect("browser player weapon");
    let weapon_state = BrowserWeaponState {
        damage: weapon.config.damage,
        ammo_remaining: weapon.state.ammo_remaining,
        ammo_capacity: weapon.config.ammo_capacity,
        ready_at_tick: weapon.state.ready_at_tick.raw(),
    };
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
        extraction_beacon,
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
