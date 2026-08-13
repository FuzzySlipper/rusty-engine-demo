//! Complete E1M1 application content projected from admitted Rust state.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use rusty_engine::engine_spatial::VoxelCollisionScene;
use rusty_engine::render_model::{
    pack_mesh_resources, AnimatedMeshPlaybackCommand, AnimationLoopMode, BillboardMode,
    RenderAssetKind, RenderDiff, RenderFrameDiff, RenderHandle, RenderMetadata,
    ResolvedRenderAsset, SpriteAttachment, SpriteDepthPolicy, SpriteInstanceDescriptor,
    SpriteShading, SpriteSizeMode, TextureDescriptor, TextureFilter, TextureWrap, Transform,
    MAX_MESH_RESOURCE_BYTES,
};
use rusty_engine::render_projection::EntityRenderProjector;
use rusty_engine::renderer_webview_host::RendererResource;

use crate::{
    project_stored_voxel_volume, EnemyCombatPosture, EnemyState, GameRuntime,
    StoredDirectionalSpriteView, StoredProject, StoredVisualAnimationLoopMode,
    StoredVisualPresentation, StoredVisualState,
};

#[derive(Debug, Clone)]
pub struct ProjectedApplicationContent {
    pub frame: RenderFrameDiff,
    pub resources: Vec<RendererResource>,
}

#[derive(Debug, Clone, Copy)]
struct HitEffectProjection {
    started_at: u64,
    created: bool,
}

#[derive(Debug, Clone)]
struct DoomSpriteInspectionEntry {
    entity: u64,
    family: String,
    clip: String,
    label: String,
    display_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoomSpriteInspectionReadout {
    pub entity: u64,
    pub family: String,
    pub clip: String,
    pub label: String,
    pub sequence_index: usize,
    pub sequence_count: usize,
    pub elapsed_ticks: u64,
    pub display_ticks: u64,
    pub frame: u32,
    pub frame_index: usize,
    pub frame_count: usize,
    pub loop_mode: StoredVisualAnimationLoopMode,
}

#[derive(Debug, Clone)]
pub struct GameplayApplicationProjector {
    entities: EntityRenderProjector,
    assets: BTreeMap<String, ResolvedRenderAsset>,
    bindings: BTreeMap<u64, BTreeMap<StoredVisualState, StoredVisualPresentation>>,
    camera_entity: Option<u64>,
    visual_states: BTreeMap<u64, StoredVisualState>,
    visual_frames: BTreeMap<u64, u32>,
    visual_mirrors: BTreeMap<u64, bool>,
    visual_offsets: BTreeMap<u64, [f32; 2]>,
    visual_state_started_at: BTreeMap<u64, u64>,
    health: BTreeMap<u64, u32>,
    hit_effects: BTreeMap<u64, HitEffectProjection>,
    doom_sprite_inspection: Vec<DoomSpriteInspectionEntry>,
    inspection_visibility: BTreeMap<u64, bool>,
}

impl GameplayApplicationProjector {
    pub fn new(project: &StoredProject) -> Self {
        let assets = project
            .assets
            .iter()
            .filter_map(|asset| {
                let catalog = asset.catalog.as_ref()?;
                let kind = if asset.animated_mesh.is_some() {
                    RenderAssetKind::AnimatedMesh
                } else if asset.static_mesh.is_some() {
                    RenderAssetKind::StaticMesh
                } else if asset.sprite_atlas.is_some() {
                    RenderAssetKind::Sprite
                } else {
                    return None;
                };
                Some((
                    asset.id.clone(),
                    ResolvedRenderAsset {
                        id: asset.id.clone(),
                        kind,
                        content_hash: catalog.hash.clone(),
                        version: catalog.version,
                    },
                ))
            })
            .collect();
        let bindings = project
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                let binding = entity.renderable.as_ref()?.visual_binding.as_ref()?;
                Some((
                    entity.id,
                    binding
                        .states
                        .iter()
                        .map(|state| (state.state, state.presentation.clone()))
                        .collect(),
                ))
            })
            .collect();
        let camera_entity = project
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .find(|entity| entity.player_controller.is_some())
            .map(|entity| entity.id);
        let mut doom_sprite_inspection = project
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                let inspection = entity.doom_sprite_inspection.as_ref()?;
                Some((
                    inspection.sequence_order,
                    DoomSpriteInspectionEntry {
                        entity: entity.id,
                        family: inspection.family.clone(),
                        clip: inspection.clip.clone(),
                        label: inspection.label.clone(),
                        display_ticks: inspection.display_ticks,
                    },
                ))
            })
            .collect::<Vec<_>>();
        doom_sprite_inspection.sort_by_key(|(sequence_order, _)| *sequence_order);
        Self {
            entities: EntityRenderProjector::new(),
            assets,
            bindings,
            camera_entity,
            visual_states: BTreeMap::new(),
            visual_frames: BTreeMap::new(),
            visual_mirrors: BTreeMap::new(),
            visual_offsets: BTreeMap::new(),
            visual_state_started_at: BTreeMap::new(),
            health: BTreeMap::new(),
            hit_effects: BTreeMap::new(),
            doom_sprite_inspection: doom_sprite_inspection
                .into_iter()
                .map(|(_, entry)| entry)
                .collect(),
            inspection_visibility: BTreeMap::new(),
        }
    }

    pub fn doom_sprite_inspection_readout(
        &self,
        runtime: &GameRuntime,
    ) -> Option<DoomSpriteInspectionReadout> {
        let (sequence_index, entry, elapsed_ticks) =
            self.doom_sprite_inspection_selection(runtime.tick().raw())?;
        let states = self.bindings.get(&entry.entity)?;
        let StoredVisualPresentation::SpriteFrames {
            frames,
            ticks_per_frame,
            loop_mode,
            ..
        } = states.get(&StoredVisualState::Default)?
        else {
            return None;
        };
        let animation_tick = elapsed_ticks / ticks_per_frame;
        let frame_index = match loop_mode {
            StoredVisualAnimationLoopMode::Repeat => animation_tick as usize % frames.len(),
            StoredVisualAnimationLoopMode::Once => (animation_tick as usize).min(frames.len() - 1),
        };
        Some(DoomSpriteInspectionReadout {
            entity: entry.entity,
            family: entry.family.clone(),
            clip: entry.clip.clone(),
            label: entry.label.clone(),
            sequence_index,
            sequence_count: self.doom_sprite_inspection.len(),
            elapsed_ticks,
            display_ticks: entry.display_ticks,
            frame: frames[frame_index],
            frame_index,
            frame_count: frames.len(),
            loop_mode: *loop_mode,
        })
    }

    fn doom_sprite_inspection_selection(
        &self,
        tick: u64,
    ) -> Option<(usize, &DoomSpriteInspectionEntry, u64)> {
        let cycle_ticks = self
            .doom_sprite_inspection
            .iter()
            .map(|entry| entry.display_ticks)
            .sum::<u64>();
        if cycle_ticks == 0 {
            return None;
        }
        let mut cursor = tick % cycle_ticks;
        for (index, entry) in self.doom_sprite_inspection.iter().enumerate() {
            if cursor < entry.display_ticks {
                return Some((index, entry, cursor));
            }
            cursor -= entry.display_ticks;
        }
        None
    }

    pub fn project(&mut self, runtime: &GameRuntime) -> anyhow::Result<RenderFrameDiff> {
        let projected = self
            .entities
            .project(runtime.session().entities(), &self.assets)
            .map_err(|error| anyhow::anyhow!("project live gameplay entities: {error:?}"))?;
        let mut operations = projected.frame.ops;
        let inspection_selection = self
            .doom_sprite_inspection_selection(runtime.tick().raw())
            .map(|(_, entry, elapsed)| (entry.entity, elapsed));
        for (entity, states) in &self.bindings {
            let id = rusty_engine::core_ids::EntityId::new(*entity);
            let combat = runtime.session().enemy_combat(id);
            let logical_facing_target = combat
                .as_ref()
                .and_then(|combat| combat.state.last_known_target_position);
            if combat.is_some() {
                if let Some(health) = runtime.session().health(id) {
                    if self
                        .health
                        .get(entity)
                        .is_some_and(|previous| *previous > health.current)
                    {
                        self.hit_effects
                            .entry(*entity)
                            .and_modify(|effect| effect.started_at = runtime.tick().raw())
                            .or_insert(HitEffectProjection {
                                started_at: runtime.tick().raw(),
                                created: false,
                            });
                    }
                    self.health.insert(*entity, health.current);
                }
            }
            let mut desired = if let Some(combat) = combat {
                if runtime
                    .session()
                    .enemy(id)
                    .is_some_and(|enemy| enemy.state == EnemyState::Defeated)
                {
                    StoredVisualState::Defeated
                } else if combat.state.pain_ticks_remaining > 0 {
                    StoredVisualState::Hit
                } else {
                    match combat.state.posture {
                        EnemyCombatPosture::Sleeping => StoredVisualState::Idle,
                        EnemyCombatPosture::Alert => StoredVisualState::Alert,
                        EnemyCombatPosture::Pursuing => StoredVisualState::Moving,
                        EnemyCombatPosture::Attacking => StoredVisualState::Attacking,
                        EnemyCombatPosture::Dead => StoredVisualState::Defeated,
                    }
                }
            } else {
                StoredVisualState::Default
            };
            if !matches!(
                desired,
                StoredVisualState::Hit | StoredVisualState::Defeated
            ) && self.visual_states.get(entity) == Some(&StoredVisualState::Attacking)
            {
                if let Some(StoredVisualPresentation::SpriteFrames {
                    frames,
                    ticks_per_frame,
                    ..
                }) = states.get(&StoredVisualState::Attacking)
                {
                    let started_at = self
                        .visual_state_started_at
                        .get(entity)
                        .copied()
                        .unwrap_or(runtime.tick().raw());
                    let duration = ticks_per_frame.saturating_mul(frames.len() as u64);
                    if runtime.tick().raw().saturating_sub(started_at) < duration {
                        desired = StoredVisualState::Attacking;
                    }
                }
            }
            let Some(presentation) = states.get(&desired) else {
                continue;
            };
            let Some(handle) = self.entities.handle_of(id) else {
                continue;
            };
            let inspection_elapsed = if self
                .doom_sprite_inspection
                .iter()
                .any(|entry| entry.entity == *entity)
            {
                let active =
                    inspection_selection.is_some_and(|(active_entity, _)| active_entity == *entity);
                if self.inspection_visibility.get(entity) != Some(&active) {
                    operations.push(RenderDiff::UpdateSprite {
                        handle,
                        frame: None,
                        tint: None,
                        render_order: None,
                        visible: Some(active),
                    });
                    self.inspection_visibility.insert(*entity, active);
                }
                if !active {
                    continue;
                }
                inspection_selection.map(|(_, elapsed)| elapsed)
            } else {
                None
            };
            match presentation {
                StoredVisualPresentation::Animation {
                    clip,
                    loop_mode,
                    speed,
                    fade_seconds,
                } if self.visual_states.get(entity) != Some(&desired) => {
                    operations.push(RenderDiff::SetAnimatedMeshPlayback {
                        handle,
                        playback: AnimatedMeshPlaybackCommand::Play {
                            clip: clip.clone(),
                            r#loop: match loop_mode {
                                StoredVisualAnimationLoopMode::Once => AnimationLoopMode::Once,
                                StoredVisualAnimationLoopMode::Repeat => AnimationLoopMode::Repeat,
                            },
                            speed: *speed,
                            weight: 1.0,
                            restart: false,
                            fade_seconds: *fade_seconds,
                        },
                    });
                    self.visual_states.insert(*entity, desired);
                }
                StoredVisualPresentation::SpriteFrames {
                    frames,
                    ticks_per_frame,
                    loop_mode,
                    directional_views,
                } => {
                    if self.visual_states.get(entity) != Some(&desired) {
                        self.visual_state_started_at
                            .insert(*entity, runtime.tick().raw());
                    }
                    let started_at = self
                        .visual_state_started_at
                        .get(entity)
                        .copied()
                        .unwrap_or(runtime.tick().raw());
                    let elapsed = inspection_elapsed
                        .unwrap_or_else(|| runtime.tick().raw().saturating_sub(started_at))
                        / ticks_per_frame;
                    let index = match loop_mode {
                        StoredVisualAnimationLoopMode::Repeat => elapsed as usize % frames.len(),
                        StoredVisualAnimationLoopMode::Once => {
                            (elapsed as usize).min(frames.len() - 1)
                        }
                    };
                    let (frame, mirrored, source_origin_offset) = if directional_views.is_empty() {
                        (frames[index], false, [0.0, 0.0])
                    } else {
                        let camera = self
                            .camera_entity
                            .and_then(|entity| {
                                runtime
                                    .session()
                                    .entity(rusty_engine::core_ids::EntityId::new(entity))
                                    .ok()?
                                    .world_transform
                            })
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "directional sprite entity {id} has no live player camera"
                                )
                            })?;
                        let actor = runtime
                            .session()
                            .entity(id)
                            .map_err(|error| {
                                anyhow::anyhow!("read directional sprite entity {id}: {error}")
                            })?
                            .world_transform
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "directional sprite entity {id} has no world transform"
                                )
                            })?;
                        let selected = select_directional_sprite_view(
                            directional_views,
                            index,
                            camera,
                            actor,
                            logical_facing_target,
                        )
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "directional sprite entity {id} has no view for animation frame {index}"
                            )
                        })?;
                        (
                            selected.frames[index],
                            selected.mirrored,
                            selected.source_origin_offsets[index],
                        )
                    };
                    if self.visual_frames.get(entity) != Some(&frame) {
                        operations.push(RenderDiff::UpdateSprite {
                            handle,
                            frame: Some(frame),
                            tint: None,
                            render_order: None,
                            visible: None,
                        });
                        self.visual_frames.insert(*entity, frame);
                    }
                    let authored_transform_changed =
                        operations.iter().any(|operation| match operation {
                            RenderDiff::CreateSprite {
                                handle: operation_handle,
                                ..
                            } => *operation_handle == handle,
                            RenderDiff::Update {
                                handle: operation_handle,
                                transform: Some(_),
                                ..
                            } => *operation_handle == handle,
                            _ => false,
                        });
                    if self.visual_mirrors.get(entity) != Some(&mirrored)
                        || self.visual_offsets.get(entity) != Some(&source_origin_offset)
                        || mirrored && authored_transform_changed
                        // The horizontal source origin is camera-right relative. Recompose it
                        // as either participant moves even when its numeric value is unchanged.
                        || source_origin_offset != [0.0, 0.0]
                    {
                        let view = runtime.session().entity(id).map_err(|error| {
                            anyhow::anyhow!("read directional sprite entity {id}: {error}")
                        })?;
                        let world = view
                            .world_transform
                            .unwrap_or(rusty_engine::entity_state::EntityTransform::IDENTITY);
                        let composed = world.compose(
                            view.renderable
                                .map(|renderable| renderable.local_transform)
                                .unwrap_or(rusty_engine::entity_state::EntityTransform::IDENTITY),
                        );
                        let camera = self
                            .camera_entity
                            .and_then(|entity| {
                                runtime
                                    .session()
                                    .entity(rusty_engine::core_ids::EntityId::new(entity))
                                    .ok()?
                                    .world_transform
                            })
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "directional sprite entity {id} has no live player camera"
                                )
                            })?;
                        let to_camera_x = camera.translation.x - world.translation.x;
                        let to_camera_z = camera.translation.z - world.translation.z;
                        let camera_distance = to_camera_x.hypot(to_camera_z);
                        let (right_x, right_z) = if camera_distance <= f32::EPSILON {
                            (1.0, 0.0)
                        } else {
                            (
                                to_camera_z / camera_distance,
                                -to_camera_x / camera_distance,
                            )
                        };
                        let horizontal_offset =
                            source_origin_offset[0] * if mirrored { -1.0 } else { 1.0 };
                        operations.push(RenderDiff::Update {
                            handle,
                            transform: Some(Transform {
                                translation: [
                                    composed.translation.x + right_x * horizontal_offset,
                                    composed.translation.y + source_origin_offset[1],
                                    composed.translation.z + right_z * horizontal_offset,
                                ],
                                rotation: [
                                    composed.rotation.x,
                                    composed.rotation.y,
                                    composed.rotation.z,
                                    composed.rotation.w,
                                ],
                                scale: [
                                    composed.scale.x.abs() * if mirrored { -1.0 } else { 1.0 },
                                    composed.scale.y,
                                    composed.scale.z,
                                ],
                            }),
                            material: None,
                            visible: None,
                            metadata: None,
                        });
                        self.visual_mirrors.insert(*entity, mirrored);
                        self.visual_offsets.insert(*entity, source_origin_offset);
                    }
                    self.visual_states.insert(*entity, desired);
                }
                _ => {}
            }
        }
        let mut completed_effects = Vec::new();
        for (entity, effect) in &mut self.hit_effects {
            let elapsed = runtime.tick().raw().saturating_sub(effect.started_at);
            let handle = hit_effect_handle(*entity)?;
            let entity_id = rusty_engine::core_ids::EntityId::new(*entity);
            if !effect.created {
                let translation = runtime
                    .session()
                    .enemy(entity_id)
                    .and_then(|enemy| enemy.entity_view.transform)
                    .map(|transform| transform.translation.to_array())
                    .unwrap_or([0.0; 3]);
                operations.push(RenderDiff::CreateSprite {
                    handle,
                    parent: None,
                    sprite: SpriteInstanceDescriptor {
                        asset: "sprite/doom-blood".to_owned(),
                        frame: 0,
                        pivot: [0.5, 0.5],
                        size: [2.0, 2.0],
                        size_mode: SpriteSizeMode::World,
                        billboard: BillboardMode::Spherical,
                        tint: [1.0; 4],
                        render_order: 1,
                        depth: SpriteDepthPolicy::Default,
                        shading: SpriteShading::Unlit,
                        visible: true,
                        transform: Transform {
                            translation,
                            ..Transform::IDENTITY
                        },
                        attachment: SpriteAttachment {
                            source_entity: Some(*entity),
                            source_scene_node: None,
                            attachment_point: Some("doom-hit".to_owned()),
                        },
                        metadata: RenderMetadata {
                            source_entity: Some(*entity),
                            source_scene_node: None,
                            tags: vec!["doom-hit-effect".to_owned()],
                            label: Some("Doom blood hit effect".to_owned()),
                        },
                    },
                });
                effect.created = true;
            } else if elapsed < 12 {
                operations.push(RenderDiff::UpdateSprite {
                    handle,
                    frame: Some(((elapsed / 4) as u32).min(2)),
                    tint: None,
                    render_order: None,
                    visible: None,
                });
            } else {
                operations.push(RenderDiff::Destroy { handle });
                completed_effects.push(*entity);
            }
        }
        for entity in completed_effects {
            self.hit_effects.remove(&entity);
        }
        RenderFrameDiff::try_from_ops(operations)
            .map_err(|error| anyhow::anyhow!("build live gameplay frame: {error:?}"))
    }

    pub fn project_current(&self, runtime: &GameRuntime) -> anyhow::Result<RenderFrameDiff> {
        let mut current = self.clone();
        current.visual_states.clear();
        current.visual_frames.clear();
        current.visual_mirrors.clear();
        current.visual_offsets.clear();
        current.visual_state_started_at.clear();
        current.project(runtime)
    }
}

fn select_directional_sprite_view(
    views: &[StoredDirectionalSpriteView],
    animation_index: usize,
    camera: rusty_engine::entity_state::EntityTransform,
    actor: rusty_engine::entity_state::EntityTransform,
    logical_facing_target: Option<rusty_engine::core_math::Vec3>,
) -> Option<&StoredDirectionalSpriteView> {
    let to_camera_x = camera.translation.x - actor.translation.x;
    let to_camera_z = camera.translation.z - actor.translation.z;
    let distance = to_camera_x.hypot(to_camera_z);
    let rotation = if distance <= f32::EPSILON {
        1
    } else {
        let [forward_x, forward_z] = logical_facing_target
            .map(|target| target - actor.translation)
            .filter(|facing| facing.x.hypot(facing.z) > f32::EPSILON)
            .map_or_else(
                || horizontal_forward(actor.rotation),
                |facing| {
                    let length = facing.x.hypot(facing.z);
                    [facing.x / length, facing.z / length]
                },
            );
        let camera_x = to_camera_x / distance;
        let camera_z = to_camera_z / distance;
        let dot = forward_x * camera_x + forward_z * camera_z;
        let cross_y = forward_z * camera_x - forward_x * camera_z;
        let sector = (cross_y.atan2(dot) / std::f32::consts::FRAC_PI_4).round() as i32;
        sector.rem_euclid(8) as u8 + 1
    };
    views
        .iter()
        .find(|view| view.rotation == rotation && animation_index < view.frames.len())
}

fn horizontal_forward(rotation: rusty_engine::entity_state::Quat) -> [f32; 2] {
    // Rotate local Doom forward (negative Z) by the entity quaternion, then
    // normalize only the horizontal plane used by the camera-sector contract.
    let x = -2.0 * (rotation.x * rotation.z + rotation.w * rotation.y);
    let z = -1.0 + 2.0 * (rotation.x * rotation.x + rotation.y * rotation.y);
    let length = x.hypot(z);
    if length <= f32::EPSILON {
        [0.0, -1.0]
    } else {
        [x / length, z / length]
    }
}

fn hit_effect_handle(entity: u64) -> anyhow::Result<RenderHandle> {
    const LOCAL_BITS: u32 = 40;
    const LOCAL_MASK: u64 = (1_u64 << LOCAL_BITS) - 1;
    if entity > LOCAL_MASK {
        anyhow::bail!("Doom hit-effect entity identity exceeds the presentation namespace");
    }
    Ok(RenderHandle::new((7_u64 << LOCAL_BITS) | entity))
}

/// Build the complete immutable E1M1 frame/resource closure consumed by an
/// Engine-owned application surface. TypeScript transports these facts but
/// does not derive renderer manifests or backend configuration.
pub fn project_doom_e1m1_application_content(
    project: &StoredProject,
    scene: &VoxelCollisionScene,
    object_frame: &RenderFrameDiff,
    entity_frame: &RenderFrameDiff,
) -> anyhow::Result<ProjectedApplicationContent> {
    let (volume_frame, mut resources) = if matches!(
        project.project_id.as_str(),
        "doom-sprite-scale-room"
            | "doom-sprite-orbit-room"
            | "doom-sprite-animation-room"
            | "doom-combat-room"
    ) {
        (
            RenderFrameDiff::try_from_ops(Vec::new())
                .expect("an empty calibration-room volume frame is valid"),
            Vec::new(),
        )
    } else {
        externalize_frame_meshes(project_stored_voxel_volume(project, scene)?)?
    };
    let (texture_resources, texture_ops) = doom_texture_projection(project)?;
    if texture_resources.len() != 54 {
        anyhow::bail!(
            "Doom E1M1 application content requires 54 textures, found {}",
            texture_resources.len()
        );
    }
    resources.extend(texture_resources);
    let (sprite_resources, sprite_ops) = doom_sprite_projection(project)?;
    resources.extend(sprite_resources);
    let (static_frame, static_resources) = static_mesh_projection(project)?;
    resources.extend(static_resources);
    let (animated_resources, animated_ops) = animated_mesh_projection(project)?;
    resources.extend(animated_resources);

    let mut operations = texture_ops;
    operations.extend(sprite_ops);
    operations.extend(static_frame.ops);
    operations.extend(animated_ops);
    operations.extend(volume_frame.ops);
    operations.extend(object_frame.ops.iter().cloned());
    operations.extend(entity_frame.ops.iter().cloned());
    let frame = RenderFrameDiff::try_from_ops(operations)
        .map_err(|error| anyhow::anyhow!("build complete E1M1 application frame: {error:?}"))?;
    Ok(ProjectedApplicationContent { frame, resources })
}

fn static_mesh_projection(
    project: &StoredProject,
) -> anyhow::Result<(RenderFrameDiff, Vec<RendererResource>)> {
    let operations = project
        .assets
        .iter()
        .filter_map(|asset| asset.static_mesh.clone())
        .map(|asset| RenderDiff::DefineStaticMesh { asset })
        .collect();
    let frame = RenderFrameDiff::try_from_ops(operations)
        .map_err(|error| anyhow::anyhow!("build authored static-mesh frame: {error:?}"))?;
    externalize_frame_meshes(frame)
}

fn animated_mesh_projection(
    project: &StoredProject,
) -> anyhow::Result<(Vec<RendererResource>, Vec<RenderDiff>)> {
    project
        .assets
        .iter()
        .filter_map(|asset| asset.animated_mesh.as_ref().map(|mesh| (asset, mesh)))
        .map(|(stored, mesh)| {
            let hash = mesh.content_hash.as_ref().ok_or_else(|| {
                anyhow::anyhow!("animated mesh {} has no content hash", mesh.asset)
            })?;
            let digest = hash.strip_prefix("sha256:").ok_or_else(|| {
                anyhow::anyhow!("animated mesh {} hash is not SHA-256", mesh.asset)
            })?;
            let source = stored
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.source_path.as_ref())
                .ok_or_else(|| {
                    anyhow::anyhow!("animated mesh {} has no source path", mesh.asset)
                })?;
            let bytes = fs::read(repository_root().join(source))?;
            Ok((
                RendererResource {
                    identity: format!("mesh-resource/{digest}"),
                    content_hash: hash.clone(),
                    media_type: "application/octet-stream".to_owned(),
                    bytes,
                },
                RenderDiff::DefineAnimatedMesh {
                    asset: mesh.clone(),
                },
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map(|entries| entries.into_iter().unzip())
}

pub fn doom_texture_projection(
    project: &StoredProject,
) -> anyhow::Result<(Vec<RendererResource>, Vec<RenderDiff>)> {
    let projected = project
        .assets
        .iter()
        .filter(|asset| {
            asset.id.starts_with("texture/doom-flat-") || asset.id.starts_with("texture/doom-wall-")
        })
        .map(|asset| {
            let metadata = asset
                .catalog
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Doom texture is missing catalog metadata"))?;
            let source_path = metadata.source_path.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Doom texture is missing its checked-in source path")
            })?;
            let content_hash = metadata.hash.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Doom texture is missing its declared content hash")
            })?;
            let source = repository_root().join(source_path);
            let bytes = fs::read(&source).map_err(|error| {
                anyhow::anyhow!("read checked-in Doom texture {}: {error}", source.display())
            })?;
            let texture = TextureDescriptor::admit_png_rgba8_resource(
                asset.id.clone(),
                &bytes,
                TextureFilter::Nearest,
                TextureWrap::Repeat,
                metadata.version,
            )
            .map_err(|error| anyhow::anyhow!("admit Doom texture {source_path}: {error:?}"))?;
            if texture.content_hash.as_ref() != Some(content_hash) {
                anyhow::bail!("Doom texture {source_path} differs from its declared hash");
            }
            let resource_identity = format!(
                "texture-resource/{}",
                content_hash
                    .strip_prefix("sha256:")
                    .ok_or_else(|| anyhow::anyhow!("Doom texture hash is not SHA-256"))?
            );
            Ok((
                RendererResource {
                    identity: resource_identity,
                    content_hash: content_hash.clone(),
                    media_type: "image/png".to_owned(),
                    bytes,
                },
                RenderDiff::DefineTexture { texture },
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(projected.into_iter().unzip())
}

fn doom_sprite_projection(
    project: &StoredProject,
) -> anyhow::Result<(Vec<RendererResource>, Vec<RenderDiff>)> {
    let mut resources = Vec::new();
    let mut operations = Vec::new();
    let mut textures = std::collections::BTreeSet::new();
    for asset in project
        .assets
        .iter()
        .filter(|asset| asset.sprite_atlas.is_some())
    {
        let atlas = asset.sprite_atlas.as_ref().expect("filtered sprite atlas");
        let metadata = asset
            .catalog
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Doom sprite atlas is missing catalog metadata"))?;
        let source_path = metadata.source_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Doom sprite atlas is missing its checked-in source path")
        })?;
        let content_hash = metadata.hash.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Doom sprite atlas is missing its declared content hash")
        })?;
        let source = repository_root().join(source_path);
        let bytes = fs::read(&source).map_err(|error| {
            anyhow::anyhow!(
                "read checked-in Doom sprite atlas {}: {error}",
                source.display()
            )
        })?;
        if textures.insert(atlas.texture.clone()) {
            let texture = TextureDescriptor::admit_png_rgba8_resource(
                atlas.texture.clone(),
                &bytes,
                TextureFilter::Nearest,
                TextureWrap::Clamp,
                metadata.version,
            )
            .map_err(|error| anyhow::anyhow!("admit Doom sprite atlas {source_path}: {error:?}"))?;
            if texture.content_hash.as_ref() != Some(content_hash) {
                anyhow::bail!("Doom sprite atlas {source_path} differs from its declared hash");
            }
            let digest = content_hash
                .strip_prefix("sha256:")
                .ok_or_else(|| anyhow::anyhow!("Doom sprite atlas hash is not SHA-256"))?;
            resources.push(RendererResource {
                identity: format!("texture-resource/{digest}"),
                content_hash: content_hash.clone(),
                media_type: "image/png".to_owned(),
                bytes,
            });
            operations.push(RenderDiff::DefineTexture { texture });
        }
        operations.push(RenderDiff::DefineSpriteAtlas {
            atlas: atlas.clone(),
        });
    }
    Ok((resources, operations))
}

pub fn externalize_frame_meshes(
    mut frame: RenderFrameDiff,
) -> anyhow::Result<(RenderFrameDiff, Vec<RendererResource>)> {
    let payloads = frame
        .ops
        .iter()
        .filter_map(|operation| match operation {
            RenderDiff::ReplaceMeshPayload { payload, .. } => Some(payload.clone()),
            RenderDiff::DefineStaticMesh { asset } => Some(asset.payload.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let packed = pack_mesh_resources(&payloads, MAX_MESH_RESOURCE_BYTES)
        .map_err(|error| anyhow::anyhow!("pack E1M1 voxel meshes: {error:?}"))?;
    let mut replacements = packed.payloads.into_iter();
    for operation in &mut frame.ops {
        match operation {
            RenderDiff::ReplaceMeshPayload { payload, .. } => {
                *payload = replacements
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("packed E1M1 mesh payload is missing"))?;
            }
            RenderDiff::DefineStaticMesh { asset } => {
                asset.payload = replacements
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("packed E1M1 static-mesh payload is missing"))?;
            }
            _ => {}
        }
    }
    if replacements.next().is_some() {
        anyhow::bail!("packed E1M1 mesh payload count exceeded the frame");
    }
    let resources = packed
        .resources
        .into_iter()
        .map(|resource| RendererResource {
            identity: resource.resource,
            content_hash: resource.content_hash,
            media_type: "application/vnd.rusty-engine.mesh-resource".to_owned(),
            bytes: resource.bytes,
        })
        .collect();
    Ok((frame, resources))
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[cfg(test)]
mod tests {
    use rusty_engine::core_math::Vec3;
    use rusty_engine::engine_spatial::VoxelCollisionScene;
    use rusty_engine::entity_state::{EntityTransform, Quat};
    use rusty_engine::render_model::RenderDiff;

    use crate::{
        decode_project_document, project_stored_voxel_objects, DamageCommand, DamageService,
        DamageSource, GameRuntime, StoredDirectionalSpriteView,
    };

    use super::{
        project_doom_e1m1_application_content, select_directional_sprite_view,
        GameplayApplicationProjector,
    };

    #[test]
    fn directional_sprite_selector_maps_one_orbit_to_eight_views_and_mirrors() {
        let views = (1_u8..=8)
            .map(|rotation| StoredDirectionalSpriteView {
                rotation,
                frames: vec![100 + u32::from(rotation)],
                mirrored: rotation >= 6,
                source_origin_offsets: vec![[0.0, 0.0]],
            })
            .collect::<Vec<_>>();
        let actor = EntityTransform::IDENTITY;
        let orbit = [
            ([0.0, 0.0, -8.0], 1, false),
            ([-8.0, 0.0, -8.0], 2, false),
            ([-8.0, 0.0, 0.0], 3, false),
            ([-8.0, 0.0, 8.0], 4, false),
            ([0.0, 0.0, 8.0], 5, false),
            ([8.0, 0.0, 8.0], 6, true),
            ([8.0, 0.0, 0.0], 7, true),
            ([8.0, 0.0, -8.0], 8, true),
        ];

        for (translation, expected_rotation, expected_mirror) in orbit {
            let camera =
                EntityTransform::at(Vec3::new(translation[0], translation[1], translation[2]));
            let selected = select_directional_sprite_view(&views, 0, camera, actor, None).unwrap();
            assert_eq!(selected.rotation, expected_rotation);
            assert_eq!(selected.frames, [100 + u32::from(expected_rotation)]);
            assert_eq!(selected.mirrored, expected_mirror);
        }
    }

    #[test]
    fn directional_sprite_selector_respects_actor_yaw_for_front_and_rear() {
        let views = (1_u8..=8)
            .map(|rotation| StoredDirectionalSpriteView {
                rotation,
                frames: vec![u32::from(rotation)],
                mirrored: false,
                source_origin_offsets: vec![[0.0, 0.0]],
            })
            .collect::<Vec<_>>();
        let actor = EntityTransform {
            rotation: Quat::new(0.0, 1.0, 0.0, 0.0),
            ..EntityTransform::IDENTITY
        };

        let front = select_directional_sprite_view(
            &views,
            0,
            EntityTransform::at(Vec3::new(0.0, 0.0, 8.0)),
            actor,
            None,
        )
        .unwrap();
        let rear = select_directional_sprite_view(
            &views,
            0,
            EntityTransform::at(Vec3::new(0.0, 0.0, -8.0)),
            actor,
            None,
        )
        .unwrap();

        assert_eq!(front.rotation, 1);
        assert_eq!(rear.rotation, 5);
    }

    #[test]
    fn directional_sprite_selector_keeps_combat_facing_separate_from_billboarding() {
        let views = (1_u8..=8)
            .map(|rotation| StoredDirectionalSpriteView {
                rotation,
                frames: vec![u32::from(rotation)],
                mirrored: false,
                source_origin_offsets: vec![[0.0, 0.0]],
            })
            .collect::<Vec<_>>();
        let actor = EntityTransform::IDENTITY;
        let camera = EntityTransform::at(Vec3::new(0.0, 0.0, 8.0));

        let authored = select_directional_sprite_view(&views, 0, camera, actor, None).unwrap();
        let combat_facing = select_directional_sprite_view(
            &views,
            0,
            camera,
            actor,
            Some(Vec3::new(0.0, 0.0, 8.0)),
        )
        .unwrap();

        assert_eq!(authored.rotation, 5);
        assert_eq!(combat_facing.rotation, 1);
    }

    #[test]
    fn directional_sprite_projection_recomposes_camera_relative_source_offsets() {
        let source = include_str!("../../../../content/projects/doom-combat-room.project.json");
        let project = decode_project_document(source).unwrap().project;
        let runtime = GameRuntime::from_stored_project(source).unwrap();
        let enemy = runtime.session().enemy_combatants().next().unwrap().entity;
        let mut projector = GameplayApplicationProjector::new(&project);

        projector.project(&runtime).unwrap();
        let repeated = projector.project(&runtime).unwrap();
        let handle = projector.entities.handle_of(enemy).unwrap();

        assert!(repeated.ops.iter().any(|operation| matches!(
            operation,
            RenderDiff::Update {
                handle: operation_handle,
                transform: Some(_),
                ..
            } if *operation_handle == handle
        )));
    }

    #[test]
    fn e1m1_application_content_is_complete() {
        let source = include_str!("../../../../content/projects/doom-e1m1.project.json");
        let project = decode_project_document(source).unwrap().project;
        let runtime = GameRuntime::from_stored_project(source).unwrap();
        let admitted = runtime.collision_scene().unwrap();
        let scene = VoxelCollisionScene::from_material_voxels(
            admitted.voxel_size(),
            admitted.chunk_size(),
            admitted.material_voxels().to_vec(),
        )
        .unwrap();
        let objects = project_stored_voxel_objects(&project).unwrap();
        let mut gameplay = GameplayApplicationProjector::new(&project);
        let entities = gameplay.project(&runtime).unwrap();
        let content =
            project_doom_e1m1_application_content(&project, &scene, &objects, &entities).unwrap();

        assert!(!content.frame.ops.is_empty());
        assert_eq!(
            content
                .resources
                .iter()
                .filter(|resource| resource.identity.starts_with("texture-resource/"))
                .count(),
            56
        );
        assert_eq!(
            content
                .frame
                .ops
                .iter()
                .filter(|operation| matches!(operation, RenderDiff::DefineSpriteAtlas { .. }))
                .count(),
            5
        );
        assert!(content
            .resources
            .iter()
            .any(|resource| resource.identity.starts_with("mesh-resource/")));
        assert!(content
            .resources
            .iter()
            .all(|resource| !resource.bytes.is_empty()));
        let defined_static_meshes = content
            .frame
            .ops
            .iter()
            .filter_map(|operation| match operation {
                RenderDiff::DefineStaticMesh { asset } => Some(asset.asset.as_str()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert!(content.frame.ops.iter().all(|operation| match operation {
            RenderDiff::CreateStaticMeshInstance { instance, .. } => {
                defined_static_meshes.contains(instance.asset.as_str())
            }
            _ => true,
        }));
    }

    #[test]
    fn gameplay_projection_emits_authored_hit_and_death_frames_without_hiding_the_enemy() {
        let source = include_str!("../../../../content/projects/doom-e1m1.project.json");
        let project = decode_project_document(source).unwrap().project;
        let mut runtime = GameRuntime::from_stored_project(source).unwrap();
        let enemy = runtime.session().enemy_combatants().next().unwrap().entity;
        let mut projector = GameplayApplicationProjector::new(&project);
        projector.project(&runtime).unwrap();

        DamageService::apply(
            runtime.session_mut(),
            DamageCommand {
                source: DamageSource::Direct { actor: enemy },
                target: enemy,
                amount: 1,
            },
        )
        .unwrap();
        let hit = projector.project(&runtime).unwrap();
        assert!(hit
            .ops
            .iter()
            .any(|operation| matches!(operation, RenderDiff::UpdateSprite { frame: Some(_), .. })));
        assert!(hit.ops.iter().any(|operation| matches!(
            operation,
            RenderDiff::CreateSprite { sprite, .. }
                if sprite.asset == "sprite/doom-blood" && sprite.frame == 0
        )));
        let repeated = projector.project(&runtime).unwrap();
        assert!(!repeated.ops.iter().any(|operation| matches!(
            operation,
            RenderDiff::CreateSprite { sprite, .. }
                if sprite.asset == "sprite/doom-blood"
        )));

        DamageService::apply(
            runtime.session_mut(),
            DamageCommand {
                source: DamageSource::Direct { actor: enemy },
                target: enemy,
                amount: 1_000,
            },
        )
        .unwrap();
        let death = projector.project(&runtime).unwrap();
        assert!(death
            .ops
            .iter()
            .any(|operation| matches!(operation, RenderDiff::UpdateSprite { frame: Some(_), .. })));
        let hit_frame = hit.ops.iter().find_map(|operation| match operation {
            RenderDiff::UpdateSprite { frame, .. } => *frame,
            _ => None,
        });
        let death_frame = death.ops.iter().find_map(|operation| match operation {
            RenderDiff::UpdateSprite { frame, .. } => *frame,
            _ => None,
        });
        assert_ne!(hit_frame, death_frame);
        assert!(!death.ops.iter().any(|operation| matches!(
            operation,
            RenderDiff::Update {
                visible: Some(false),
                ..
            }
        )));
    }
}
