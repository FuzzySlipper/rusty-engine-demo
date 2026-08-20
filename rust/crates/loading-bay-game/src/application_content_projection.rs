//! Complete E1M1 application content projected from admitted Rust state.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use rusty_engine::engine_spatial::VoxelCollisionScene;
use rusty_engine::render_model::{
    pack_mesh_resources, AnimatedMeshPlaybackCommand, AnimationLoopMode, BillboardMode, Geometry,
    RenderAssetKind, RenderDiff, RenderFrameDiff, RenderHandle, RenderLayer, RenderMetadata,
    RenderNode, ResolvedRenderAsset, SkyBackgroundDescriptor, SpriteAttachment, SpriteDepthPolicy,
    SpriteInstanceDescriptor, SpriteShading, SpriteSizeMode, TextureDescriptor, TextureFilter,
    TexturePayloadSource, TextureWrap, Transform, MAX_MESH_RESOURCE_BYTES,
};
use rusty_engine::render_projection::EntityRenderProjector;
use rusty_engine::renderer_webview_host::RendererResource;

use crate::{
    project_stored_voxel_volume, CombatFact, CombatImpactKind, EnemyCombatFact, EnemyCombatPosture,
    EnemyState, GameLoopFact, GameRuntime, PickupState, StoredDirectionalSpriteView, StoredProject,
    StoredVisualAnimationLoopMode, StoredVisualPresentation, StoredVisualState,
};

const MAX_DOOM_TRANSIENT_EFFECTS: usize = 256;
const DOOM_SKY_TEXTURE_ID: &str = "texture/doom-sky1-panorama";
const DOOM_SKY_SOURCE_PATH: &str = "content/doom-e1m1/textures/sky/SKY1.png";

#[derive(Debug, Clone)]
pub struct ProjectedApplicationContent {
    pub frame: RenderFrameDiff,
    pub resources: Vec<RendererResource>,
}

#[derive(Debug, Clone, Copy)]
struct TransientEffectProjection {
    started_at: u64,
    created: bool,
    clip: DoomEffectClipKind,
    position: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DoomEffectClipKind {
    Blood,
    BulletPuff,
    ProjectileFlight,
    ProjectileImpact,
}

#[derive(Debug, Clone)]
struct DoomEffectClip {
    asset: String,
    frames: Vec<u32>,
    source_origin_offsets: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DoomWeaponViewmodelFrame {
    frame: u32,
    translation: [f32; 3],
}

#[derive(Debug, Clone)]
struct DoomWeaponViewmodel {
    asset: String,
    ready: DoomWeaponViewmodelFrame,
    fire: Vec<DoomWeaponViewmodelFrame>,
    flash_asset: Option<String>,
    flash: Vec<DoomWeaponViewmodelFrame>,
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
    visual_transforms: BTreeMap<u64, Transform>,
    visual_state_started_at: BTreeMap<u64, u64>,
    health: BTreeMap<u64, u32>,
    effect_clips: BTreeMap<DoomEffectClipKind, DoomEffectClip>,
    transient_effects: BTreeMap<u64, TransientEffectProjection>,
    active_projectiles: BTreeMap<u64, u64>,
    next_effect_identity: u64,
    doom_sprite_inspection: Vec<DoomSpriteInspectionEntry>,
    inspection_visibility: BTreeMap<u64, bool>,
    weapon_viewmodels: BTreeMap<String, DoomWeaponViewmodel>,
    viewmodel_root_created: bool,
    viewmodel_weapon: Option<String>,
    viewmodel_base_frame: Option<DoomWeaponViewmodelFrame>,
    viewmodel_flash_frame: Option<DoomWeaponViewmodelFrame>,
    viewmodel_flash_visible: bool,
    viewmodel_attack: Option<(String, u64)>,
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
        let shared_doom_bindings: BTreeMap<
            String,
            BTreeMap<StoredVisualState, StoredVisualPresentation>,
        > = project
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter(|entity| entity.name.starts_with("doom-visual-template-"))
            .filter_map(|entity| {
                let renderable = entity.renderable.as_ref()?;
                let binding = renderable.visual_binding.as_ref()?;
                Some((
                    renderable.asset.clone(),
                    binding
                        .states
                        .iter()
                        .map(|state| (state.state, state.presentation.clone()))
                        .collect(),
                ))
            })
            .collect();
        let bindings = project
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(|entity| {
                let renderable = entity.renderable.as_ref()?;
                let states = renderable
                    .visual_binding
                    .as_ref()
                    .map(|binding| {
                        binding
                            .states
                            .iter()
                            .map(|state| (state.state, state.presentation.clone()))
                            .collect()
                    })
                    .or_else(|| shared_doom_bindings.get(&renderable.asset).cloned())?;
                Some((entity.id, states))
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
        let effect_clips = project
            .scenes
            .iter()
            .flat_map(|scene| &scene.entities)
            .filter_map(doom_effect_clip)
            .collect();
        let weapon_viewmodels = doom_weapon_viewmodels(project);
        Self {
            entities: EntityRenderProjector::new(),
            assets,
            bindings,
            camera_entity,
            visual_states: BTreeMap::new(),
            visual_frames: BTreeMap::new(),
            visual_mirrors: BTreeMap::new(),
            visual_offsets: BTreeMap::new(),
            visual_transforms: BTreeMap::new(),
            visual_state_started_at: BTreeMap::new(),
            health: BTreeMap::new(),
            effect_clips,
            transient_effects: BTreeMap::new(),
            active_projectiles: BTreeMap::new(),
            next_effect_identity: 1,
            doom_sprite_inspection: doom_sprite_inspection
                .into_iter()
                .map(|(_, entry)| entry)
                .collect(),
            inspection_visibility: BTreeMap::new(),
            weapon_viewmodels,
            viewmodel_root_created: false,
            viewmodel_weapon: None,
            viewmodel_base_frame: None,
            viewmodel_flash_frame: None,
            viewmodel_flash_visible: false,
            viewmodel_attack: None,
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

    pub fn project_with_facts(
        &mut self,
        runtime: &GameRuntime,
        facts: &[GameLoopFact],
    ) -> anyhow::Result<RenderFrameDiff> {
        self.observe_combat_outcomes(runtime, facts);
        self.project(runtime)
    }

    fn observe_combat_outcomes(&mut self, runtime: &GameRuntime, facts: &[GameLoopFact]) {
        for fact in facts {
            match fact {
                GameLoopFact::Combat(CombatFact::ImpactResolved {
                    kind,
                    position,
                    direction,
                    ..
                }) => {
                    let clip = match kind {
                        CombatImpactKind::Blood => DoomEffectClipKind::Blood,
                        CombatImpactKind::BulletPuff => DoomEffectClipKind::BulletPuff,
                    };
                    let doom_backoff = match kind {
                        CombatImpactKind::Blood => 10.0 / 16.0,
                        CombatImpactKind::BulletPuff => 4.0 / 16.0,
                    };
                    self.spawn_transient_effect(
                        clip,
                        (*position - *direction * doom_backoff).to_array(),
                        runtime.tick().raw(),
                    );
                }
                GameLoopFact::Combat(CombatFact::ProjectileImpacted {
                    entity, position, ..
                }) => {
                    self.active_projectiles.remove(&entity.raw());
                    self.spawn_transient_effect(
                        DoomEffectClipKind::ProjectileImpact,
                        position.to_array(),
                        runtime.tick().raw(),
                    );
                }
                GameLoopFact::Combat(CombatFact::ProjectileExpired { entity, .. }) => {
                    self.active_projectiles.remove(&entity.raw());
                }
                GameLoopFact::Combat(CombatFact::AttackFired {
                    attacker,
                    presentation,
                    ..
                }) if self.camera_entity == Some(attacker.raw())
                    && self.weapon_viewmodels.contains_key(presentation) =>
                {
                    self.viewmodel_attack = Some((presentation.clone(), runtime.tick().raw()));
                }
                GameLoopFact::EnemyCombat(EnemyCombatFact::ProjectileSpawned {
                    projectile,
                    ..
                })
                | GameLoopFact::Combat(CombatFact::ProjectileSpawned {
                    entity: projectile, ..
                }) => {
                    self.active_projectiles
                        .insert(projectile.raw(), runtime.tick().raw());
                }
                GameLoopFact::EnemyCombat(EnemyCombatFact::AttackFired { enemy, .. })
                    if self.bindings.get(&enemy.raw()).is_some_and(|states| {
                        matches!(
                            states.get(&StoredVisualState::Attacking),
                            Some(StoredVisualPresentation::SpriteFrames { .. })
                        )
                    }) =>
                {
                    // Attacking is a sustained combat posture, while firing is a
                    // repeatable edge. Restart the authored one-shot sprite clip
                    // for every authoritative shot even when posture stays unchanged.
                    self.visual_state_started_at
                        .insert(enemy.raw(), runtime.tick().raw());
                }
                _ => {}
            }
        }
    }

    fn spawn_transient_effect(
        &mut self,
        clip: DoomEffectClipKind,
        position: [f32; 3],
        started_at: u64,
    ) {
        if !self.effect_clips.contains_key(&clip)
            || self.transient_effects.len() >= MAX_DOOM_TRANSIENT_EFFECTS
        {
            return;
        }
        let identity = self.next_effect_identity;
        self.next_effect_identity = self.next_effect_identity.saturating_add(1);
        self.transient_effects.insert(
            identity,
            TransientEffectProjection {
                started_at,
                created: false,
                clip,
                position,
            },
        );
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
            } else if let Some(pickup) = runtime.session().pickup(id) {
                match pickup.state {
                    PickupState::Dormant => StoredVisualState::Dormant,
                    PickupState::Available => StoredVisualState::Available,
                    PickupState::Collected { .. } => StoredVisualState::Collected,
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
                        let horizontal_offset =
                            source_origin_offset[0] * if mirrored { -1.0 } else { 1.0 };
                        let translation = camera_relative_sprite_translation(
                            composed.translation.to_array(),
                            camera,
                            [horizontal_offset, source_origin_offset[1]],
                        );
                        let transform = Transform {
                            translation,
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
                        };
                        if self.visual_transforms.get(entity) != Some(&transform) {
                            operations.push(RenderDiff::Update {
                                handle,
                                transform: Some(transform),
                                material: None,
                                visible: None,
                                metadata: None,
                            });
                            self.visual_transforms.insert(*entity, transform);
                        }
                        self.visual_mirrors.insert(*entity, mirrored);
                        self.visual_offsets.insert(*entity, source_origin_offset);
                    }
                    self.visual_states.insert(*entity, desired);
                }
                _ => {}
            }
        }
        if let Some(clip) = self.effect_clips.get(&DoomEffectClipKind::ProjectileFlight) {
            let camera = live_camera_transform(self.camera_entity, runtime)?;
            let mut completed_projectiles = Vec::new();
            for (entity, started_at) in &self.active_projectiles {
                let entity_id = rusty_engine::core_ids::EntityId::new(*entity);
                let Ok(view) = runtime.session().entity(entity_id) else {
                    completed_projectiles.push(*entity);
                    continue;
                };
                if view
                    .renderable
                    .as_ref()
                    .is_none_or(|renderable| renderable.asset != clip.asset)
                {
                    continue;
                }
                let Some(handle) = self.entities.handle_of(entity_id) else {
                    continue;
                };
                let elapsed = runtime.tick().raw().saturating_sub(*started_at) as usize;
                let frame_index = elapsed % clip.frames.len();
                operations.push(RenderDiff::UpdateSprite {
                    handle,
                    frame: Some(clip.frames[frame_index]),
                    tint: None,
                    render_order: None,
                    visible: None,
                });
                let world = view
                    .world_transform
                    .unwrap_or(rusty_engine::entity_state::EntityTransform::IDENTITY);
                let composed = world.compose(
                    view.renderable
                        .map(|renderable| renderable.local_transform)
                        .unwrap_or(rusty_engine::entity_state::EntityTransform::IDENTITY),
                );
                let translation = camera_relative_sprite_translation(
                    composed.translation.to_array(),
                    camera,
                    clip.source_origin_offsets[frame_index],
                );
                operations.push(RenderDiff::Update {
                    handle,
                    transform: Some(Transform {
                        translation,
                        rotation: [
                            composed.rotation.x,
                            composed.rotation.y,
                            composed.rotation.z,
                            composed.rotation.w,
                        ],
                        scale: [composed.scale.x, composed.scale.y, composed.scale.z],
                    }),
                    material: None,
                    visible: None,
                    metadata: None,
                });
            }
            for entity in completed_projectiles {
                self.active_projectiles.remove(&entity);
            }
        }

        let mut completed_effects = Vec::new();
        let camera = live_camera_transform(self.camera_entity, runtime)?;
        for (identity, effect) in &mut self.transient_effects {
            let elapsed = runtime.tick().raw().saturating_sub(effect.started_at);
            let handle = transient_effect_handle(*identity)?;
            let clip = self
                .effect_clips
                .get(&effect.clip)
                .expect("transient effect is admitted only with its clip");
            let frame_index = elapsed as usize;
            let offset = clip
                .source_origin_offsets
                .get(frame_index)
                .copied()
                .unwrap_or([0.0, 0.0]);
            let translation = camera_relative_sprite_translation(effect.position, camera, offset);
            if !effect.created {
                operations.push(RenderDiff::CreateSprite {
                    handle,
                    parent: None,
                    sprite: SpriteInstanceDescriptor {
                        asset: clip.asset.clone(),
                        frame: clip.frames[0],
                        pivot: [0.5, 0.5],
                        size: [1.0, 1.0],
                        size_mode: SpriteSizeMode::World,
                        billboard: BillboardMode::Spherical,
                        tint: [1.0; 4],
                        render_order: 1,
                        depth: SpriteDepthPolicy::Default,
                        shading: SpriteShading::Unlit,
                        material: Default::default(),
                        visible: true,
                        transform: Transform {
                            translation,
                            ..Transform::IDENTITY
                        },
                        attachment: SpriteAttachment {
                            source_entity: None,
                            source_scene_node: None,
                            attachment_point: Some("doom-combat-fx".to_owned()),
                        },
                        metadata: RenderMetadata {
                            source_entity: None,
                            source_scene_node: None,
                            tags: vec!["doom-combat-fx".to_owned()],
                            label: Some(format!("Doom {:?} effect", effect.clip)),
                        },
                    },
                });
                effect.created = true;
            } else if frame_index < clip.frames.len() {
                operations.push(RenderDiff::UpdateSprite {
                    handle,
                    frame: Some(clip.frames[frame_index]),
                    tint: None,
                    render_order: None,
                    visible: None,
                });
                operations.push(RenderDiff::Update {
                    handle,
                    transform: Some(Transform {
                        translation,
                        ..Transform::IDENTITY
                    }),
                    material: None,
                    visible: None,
                    metadata: None,
                });
            } else {
                operations.push(RenderDiff::Destroy { handle });
                completed_effects.push(*identity);
            }
        }
        for identity in completed_effects {
            self.transient_effects.remove(&identity);
        }
        self.project_weapon_viewmodel(runtime, &mut operations)?;
        RenderFrameDiff::try_from_ops(operations)
            .map_err(|error| anyhow::anyhow!("build live gameplay frame: {error:?}"))
    }

    pub fn project_current(&self, runtime: &GameRuntime) -> anyhow::Result<RenderFrameDiff> {
        let mut current = self.clone();
        current.visual_states.clear();
        current.visual_frames.clear();
        current.visual_mirrors.clear();
        current.visual_offsets.clear();
        current.visual_transforms.clear();
        current.visual_state_started_at.clear();
        current.viewmodel_root_created = false;
        current.viewmodel_weapon = None;
        current.viewmodel_base_frame = None;
        current.viewmodel_flash_frame = None;
        current.viewmodel_flash_visible = false;
        current.project(runtime)
    }

    fn project_weapon_viewmodel(
        &mut self,
        runtime: &GameRuntime,
        operations: &mut Vec<RenderDiff>,
    ) -> anyhow::Result<()> {
        let Some(player) = self
            .camera_entity
            .map(rusty_engine::core_ids::EntityId::new)
        else {
            return Ok(());
        };
        let weapon = runtime.session().weapon(player);
        let presentation = weapon
            .as_ref()
            .map(|weapon| weapon.definition.presentation.clone());
        let cooldown_ticks = weapon
            .as_ref()
            .map(|weapon| weapon.definition.cooldown_ticks as usize)
            .unwrap_or(0);
        let definition = presentation
            .as_ref()
            .and_then(|presentation| self.weapon_viewmodels.get(presentation));
        if definition.is_none() {
            if let Some(previous) = self.viewmodel_weapon.take() {
                operations.push(RenderDiff::UpdateSprite {
                    handle: doom_weapon_viewmodel_base_handle(&previous),
                    frame: None,
                    tint: None,
                    render_order: None,
                    visible: Some(false),
                });
                if self.viewmodel_flash_frame.is_some() {
                    operations.push(RenderDiff::UpdateSprite {
                        handle: doom_weapon_viewmodel_flash_handle(&previous),
                        frame: None,
                        tint: None,
                        render_order: None,
                        visible: Some(false),
                    });
                }
            }
            return Ok(());
        }
        let definition = definition.expect("checked above");
        let presentation = presentation.expect("definition requires presentation");
        let elapsed = self
            .viewmodel_attack
            .as_ref()
            .and_then(|(attack, started_at)| {
                let elapsed = runtime.tick().raw().saturating_sub(*started_at) as usize;
                (attack == &presentation && elapsed < cooldown_ticks).then_some(elapsed)
            });
        let base = elapsed
            .and_then(|elapsed| definition.fire.get(elapsed).copied())
            .unwrap_or(definition.ready);
        let flash = elapsed.and_then(|elapsed| definition.flash.get(elapsed).copied());

        if !self.viewmodel_root_created {
            let mut root = RenderNode::new(Geometry::Group);
            root.layer = RenderLayer::Viewmodel;
            root.transform = Transform {
                translation: [0.0, 0.0, -1.0],
                // The Engine application host owns a 55-degree viewmodel camera.
                // This maps Doom's 200-pixel-tall presentation plane exactly at z=-1.
                scale: [0.083_290_72, 0.083_290_72, 0.083_290_72],
                ..Transform::IDENTITY
            };
            root.metadata.tags = vec!["doom-player-weapon".to_owned()];
            root.metadata.label = Some("Doom player weapon viewmodel root".to_owned());
            operations.push(RenderDiff::Create {
                handle: doom_weapon_viewmodel_root_handle(),
                parent: None,
                node: root,
            });
            for (candidate, candidate_definition) in &self.weapon_viewmodels {
                operations.push(create_doom_weapon_sprite(
                    doom_weapon_viewmodel_base_handle(candidate),
                    &candidate_definition.asset,
                    candidate_definition.ready,
                    candidate == &presentation,
                    "Doom player weapon",
                ));
                if let (Some(asset), Some(initial)) = (
                    &candidate_definition.flash_asset,
                    candidate_definition.flash.first().copied(),
                ) {
                    operations.push(create_doom_weapon_sprite(
                        doom_weapon_viewmodel_flash_handle(candidate),
                        asset,
                        initial,
                        candidate == &presentation && flash.is_some(),
                        "Doom player weapon muzzle flash",
                    ));
                }
            }
            self.viewmodel_root_created = true;
            self.viewmodel_weapon = Some(presentation);
            self.viewmodel_base_frame = Some(base);
            self.viewmodel_flash_frame = definition.flash.first().copied();
            self.viewmodel_flash_visible = flash.is_some();
            if base != definition.ready {
                update_doom_weapon_sprite(
                    operations,
                    doom_weapon_viewmodel_base_handle(
                        self.viewmodel_weapon.as_deref().expect("weapon was set"),
                    ),
                    base,
                );
            }
            return Ok(());
        }

        if self.viewmodel_weapon.as_deref() != Some(&presentation) {
            if let Some(previous) = &self.viewmodel_weapon {
                operations.push(RenderDiff::UpdateSprite {
                    handle: doom_weapon_viewmodel_base_handle(previous),
                    frame: None,
                    tint: None,
                    render_order: None,
                    visible: Some(false),
                });
                if self.weapon_viewmodels[previous].flash_asset.is_some() {
                    operations.push(RenderDiff::UpdateSprite {
                        handle: doom_weapon_viewmodel_flash_handle(previous),
                        frame: None,
                        tint: None,
                        render_order: None,
                        visible: Some(false),
                    });
                }
            }
            update_doom_weapon_sprite(
                operations,
                doom_weapon_viewmodel_base_handle(&presentation),
                base,
            );
            operations.push(RenderDiff::UpdateSprite {
                handle: doom_weapon_viewmodel_base_handle(&presentation),
                frame: None,
                tint: None,
                render_order: None,
                visible: Some(true),
            });
            if definition.flash_asset.is_some() {
                let initial = flash
                    .or_else(|| definition.flash.first().copied())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Doom weapon {presentation} has a flash asset without frames"
                        )
                    })?;
                update_doom_weapon_sprite(
                    operations,
                    doom_weapon_viewmodel_flash_handle(&presentation),
                    initial,
                );
                operations.push(RenderDiff::UpdateSprite {
                    handle: doom_weapon_viewmodel_flash_handle(&presentation),
                    frame: None,
                    tint: None,
                    render_order: None,
                    visible: Some(flash.is_some()),
                });
                self.viewmodel_flash_frame = Some(initial);
            } else {
                self.viewmodel_flash_frame = None;
            }
            self.viewmodel_weapon = Some(presentation);
            self.viewmodel_base_frame = Some(base);
            self.viewmodel_flash_visible = flash.is_some();
            return Ok(());
        }

        if self.viewmodel_base_frame != Some(base) {
            update_doom_weapon_sprite(
                operations,
                doom_weapon_viewmodel_base_handle(&presentation),
                base,
            );
            self.viewmodel_base_frame = Some(base);
        }
        if let Some(frame) = flash {
            if self.viewmodel_flash_frame != Some(frame) {
                update_doom_weapon_sprite(
                    operations,
                    doom_weapon_viewmodel_flash_handle(&presentation),
                    frame,
                );
                self.viewmodel_flash_frame = Some(frame);
            }
        }
        if self.viewmodel_flash_visible != flash.is_some() && definition.flash_asset.is_some() {
            operations.push(RenderDiff::UpdateSprite {
                handle: doom_weapon_viewmodel_flash_handle(&presentation),
                frame: None,
                tint: None,
                render_order: None,
                visible: Some(flash.is_some()),
            });
            self.viewmodel_flash_visible = flash.is_some();
        }
        Ok(())
    }
}

fn doom_weapon_viewmodels(project: &StoredProject) -> BTreeMap<String, DoomWeaponViewmodel> {
    if !project
        .assets
        .iter()
        .any(|asset| asset.id == "sprite/doom-pistol-viewmodel")
    {
        return BTreeMap::new();
    }
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../content/doom-e1m1/sprites/manifest.json"
    ))
    .expect("checked-in Doom sprite manifest is generated JSON");
    let definitions = [
        ("fist", "PUNG", "sprite/doom-fist-viewmodel", None),
        (
            "pistol",
            "PISG",
            "sprite/doom-pistol-viewmodel",
            Some(("PISF", "sprite/doom-pistol-flash-viewmodel")),
        ),
        (
            "shotgun",
            "SHTG",
            "sprite/doom-shotgun-viewmodel",
            Some(("SHTF", "sprite/doom-shotgun-flash-viewmodel")),
        ),
    ];
    definitions
        .into_iter()
        .map(|(presentation, family, asset, flash)| {
            let ready = doom_weapon_clip(&manifest, family, "ready")
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("generated Doom {family} ready clip is empty"));
            let fire = doom_weapon_clip(&manifest, family, "fire");
            let (flash_asset, flash) = flash.map_or_else(
                || (None, Vec::new()),
                |(flash_family, flash_asset)| {
                    (
                        Some(flash_asset.to_owned()),
                        doom_weapon_clip(&manifest, flash_family, "flash"),
                    )
                },
            );
            (
                presentation.to_owned(),
                DoomWeaponViewmodel {
                    asset: asset.to_owned(),
                    ready,
                    fire,
                    flash_asset,
                    flash,
                },
            )
        })
        .collect()
}

fn doom_weapon_clip(
    manifest: &serde_json::Value,
    family: &str,
    clip_id: &str,
) -> Vec<DoomWeaponViewmodelFrame> {
    let atlas = manifest["atlases"]
        .as_array()
        .and_then(|atlases| {
            atlases.iter().find(|atlas| {
                atlas["frames"].as_array().is_some_and(|frames| {
                    frames
                        .iter()
                        .any(|frame| frame["family"].as_str() == Some(family))
                })
            })
        })
        .unwrap_or_else(|| panic!("generated Doom atlas has no {family} family"));
    let family_frames = atlas["frames"]
        .as_array()
        .expect("generated Doom atlas frames")
        .iter()
        .filter(|frame| frame["family"].as_str() == Some(family))
        .collect::<Vec<_>>();
    let contract = manifest["contract"]["families"]
        .as_array()
        .and_then(|families| {
            families
                .iter()
                .find(|candidate| candidate["prefix"].as_str() == Some(family))
        })
        .unwrap_or_else(|| panic!("generated Doom contract has no {family} family"));
    let clip = contract["clips"]
        .as_array()
        .and_then(|clips| {
            clips
                .iter()
                .find(|candidate| candidate["id"].as_str() == Some(clip_id))
        })
        .unwrap_or_else(|| panic!("generated Doom {family} contract has no {clip_id} clip"));
    let mut source_ticks = 0_i64;
    let mut runtime_ticks = 0_i64;
    let mut result = Vec::new();
    for step in clip["steps"]
        .as_array()
        .expect("generated Doom weapon clip steps")
    {
        let frame_name = step["frame"]
            .as_str()
            .expect("generated Doom weapon frame name");
        let source_lump = contract["directionalFrames"]
            .as_array()
            .and_then(|frames| {
                frames
                    .iter()
                    .find(|frame| frame["frame"].as_str() == Some(frame_name))
            })
            .and_then(|frame| frame["rotations"].as_array())
            .and_then(|rotations| rotations.first())
            .and_then(|rotation| rotation["sourceLump"].as_str())
            .expect("generated Doom weapon source lump");
        let (local_frame, source) = family_frames
            .iter()
            .enumerate()
            .find(|(_, frame)| frame["name"].as_str() == Some(source_lump))
            .unwrap_or_else(|| panic!("generated Doom family {family} is missing {source_lump}"));
        let width = source["pixelSize"][0]
            .as_f64()
            .expect("generated Doom weapon width") as f32;
        let height = source["pixelSize"][1]
            .as_f64()
            .expect("generated Doom weapon height") as f32;
        let left_offset = source["origin"][0]
            .as_f64()
            .expect("generated Doom weapon left offset") as f32;
        let top_offset = source["origin"][1]
            .as_f64()
            .expect("generated Doom weapon top offset") as f32;
        let center_x = 1.0 - left_offset + width * 0.5;
        let center_y = 32.0 - top_offset + height * 0.5;
        let frame = DoomWeaponViewmodelFrame {
            frame: u32::try_from(local_frame).expect("bounded Doom weapon frame"),
            translation: [(center_x - 160.0) / 16.0, (100.0 - center_y) / 16.0, 0.0],
        };
        let tics = step["tics"]
            .as_i64()
            .expect("generated Doom weapon source tics");
        let duration = if tics < 0 {
            1
        } else {
            source_ticks += tics;
            let next_runtime_ticks = ((source_ticks as f64 * 60.0) / 35.0).round() as i64;
            let duration = (next_runtime_ticks - runtime_ticks).max(1);
            runtime_ticks = next_runtime_ticks;
            duration
        };
        result.extend(std::iter::repeat_n(frame, duration as usize));
    }
    result
}

fn create_doom_weapon_sprite(
    handle: RenderHandle,
    asset: &str,
    frame: DoomWeaponViewmodelFrame,
    visible: bool,
    label: &str,
) -> RenderDiff {
    RenderDiff::CreateSprite {
        handle,
        parent: Some(doom_weapon_viewmodel_root_handle()),
        sprite: SpriteInstanceDescriptor {
            asset: asset.to_owned(),
            frame: frame.frame,
            pivot: [0.5, 0.5],
            size: [1.0, 1.0],
            size_mode: SpriteSizeMode::World,
            billboard: BillboardMode::None,
            tint: [1.0; 4],
            render_order: 100,
            depth: SpriteDepthPolicy::DepthTestOff,
            shading: SpriteShading::Unlit,
            material: Default::default(),
            visible,
            transform: Transform {
                translation: frame.translation,
                ..Transform::IDENTITY
            },
            attachment: SpriteAttachment {
                source_entity: None,
                source_scene_node: None,
                attachment_point: Some("doom-player-weapon".to_owned()),
            },
            metadata: RenderMetadata {
                source_entity: None,
                source_scene_node: None,
                tags: vec!["doom-player-weapon".to_owned()],
                label: Some(label.to_owned()),
            },
        },
    }
}

fn update_doom_weapon_sprite(
    operations: &mut Vec<RenderDiff>,
    handle: RenderHandle,
    frame: DoomWeaponViewmodelFrame,
) {
    operations.push(RenderDiff::UpdateSprite {
        handle,
        frame: Some(frame.frame),
        tint: None,
        render_order: None,
        visible: None,
    });
    operations.push(RenderDiff::Update {
        handle,
        transform: Some(Transform {
            translation: frame.translation,
            ..Transform::IDENTITY
        }),
        material: None,
        visible: None,
        metadata: None,
    });
}

fn doom_weapon_viewmodel_root_handle() -> RenderHandle {
    RenderHandle::new((6_u64 << 40) | 1)
}

fn doom_weapon_viewmodel_base_handle(presentation: &str) -> RenderHandle {
    RenderHandle::new((6_u64 << 40) | doom_weapon_viewmodel_handle_offset(presentation))
}

fn doom_weapon_viewmodel_flash_handle(presentation: &str) -> RenderHandle {
    RenderHandle::new((6_u64 << 40) | doom_weapon_viewmodel_handle_offset(presentation) | 1)
}

fn doom_weapon_viewmodel_handle_offset(presentation: &str) -> u64 {
    match presentation {
        "fist" => 2,
        "pistol" => 4,
        "shotgun" => 6,
        _ => unreachable!("viewmodel definitions admit only known Doom weapon presentations"),
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

fn live_camera_transform(
    camera_entity: Option<u64>,
    runtime: &GameRuntime,
) -> anyhow::Result<rusty_engine::entity_state::EntityTransform> {
    camera_entity
        .and_then(|entity| {
            runtime
                .session()
                .entity(rusty_engine::core_ids::EntityId::new(entity))
                .ok()?
                .world_transform
        })
        .ok_or_else(|| anyhow::anyhow!("Doom sprite projection has no live player camera"))
}

fn camera_relative_sprite_translation(
    anchor: [f32; 3],
    camera: rusty_engine::entity_state::EntityTransform,
    source_origin_offset: [f32; 2],
) -> [f32; 3] {
    let to_camera_x = camera.translation.x - anchor[0];
    let to_camera_z = camera.translation.z - anchor[2];
    let camera_distance = to_camera_x.hypot(to_camera_z);
    let (right_x, right_z) = if camera_distance <= f32::EPSILON {
        (1.0, 0.0)
    } else {
        (
            to_camera_z / camera_distance,
            -to_camera_x / camera_distance,
        )
    };
    [
        anchor[0] + right_x * source_origin_offset[0],
        anchor[1] + source_origin_offset[1],
        anchor[2] + right_z * source_origin_offset[0],
    ]
}

fn doom_effect_clip(
    entity: &crate::StoredEntityDefinition,
) -> Option<(DoomEffectClipKind, DoomEffectClip)> {
    let kind = match entity.name.as_str() {
        "doom-fx-template-blood" => DoomEffectClipKind::Blood,
        "doom-fx-template-bullet-puff" => DoomEffectClipKind::BulletPuff,
        "doom-fx-template-projectile-flight" => DoomEffectClipKind::ProjectileFlight,
        "doom-fx-template-projectile-impact" => DoomEffectClipKind::ProjectileImpact,
        _ => return None,
    };
    let renderable = entity.renderable.as_ref()?;
    let binding = renderable.visual_binding.as_ref()?;
    let presentation = binding
        .states
        .iter()
        .find(|state| state.state == StoredVisualState::Default)?;
    let StoredVisualPresentation::SpriteFrames {
        frames,
        ticks_per_frame,
        directional_views,
        ..
    } = &presentation.presentation
    else {
        return None;
    };
    if frames.is_empty() || *ticks_per_frame != 1 {
        return None;
    }
    let source_origin_offsets = directional_views
        .first()
        .filter(|view| view.source_origin_offsets.len() == frames.len())
        .map(|view| view.source_origin_offsets.clone())
        .unwrap_or_else(|| vec![[0.0, 0.0]; frames.len()]);
    Some((
        kind,
        DoomEffectClip {
            asset: renderable.asset.clone(),
            frames: frames.clone(),
            source_origin_offsets,
        },
    ))
}

fn transient_effect_handle(identity: u64) -> anyhow::Result<RenderHandle> {
    const LOCAL_BITS: u32 = 40;
    const LOCAL_MASK: u64 = (1_u64 << LOCAL_BITS) - 1;
    if identity > LOCAL_MASK {
        anyhow::bail!("Doom transient-effect identity exceeds the presentation namespace");
    }
    Ok(RenderHandle::new((7_u64 << LOCAL_BITS) | identity))
}

/// Build the complete immutable E1M1 frame/resource closure consumed by an
/// Engine-owned application surface. TypeScript transports these facts but
/// does not derive renderer manifests or backend configuration.
pub fn project_doom_e1m1_application_content(
    project: &StoredProject,
    scene: &VoxelCollisionScene,
    object_frame: &RenderFrameDiff,
    entity_frame: &RenderFrameDiff,
    content_root: &Path,
) -> anyhow::Result<ProjectedApplicationContent> {
    let (volume_frame, mut resources) =
        externalize_frame_meshes(project_stored_voxel_volume(project, scene)?)?;
    let (texture_resources, texture_ops) = doom_texture_projection(project, content_root)?;
    if texture_resources.len() != 54 {
        anyhow::bail!(
            "Doom E1M1 application content requires 54 textures, found {}",
            texture_resources.len()
        );
    }
    resources.extend(texture_resources);
    let (sky_resources, sky_ops) = doom_sky_projection(project, content_root)?;
    resources.extend(sky_resources);
    let (sprite_resources, sprite_ops) = doom_sprite_projection(project, content_root)?;
    resources.extend(sprite_resources);
    let (static_frame, static_resources) = static_mesh_projection(project)?;
    resources.extend(static_resources);
    let (animated_resources, animated_ops) = animated_mesh_projection(project, content_root)?;
    resources.extend(animated_resources);

    let mut operations = texture_ops;
    operations.extend(sky_ops);
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

/// Project canonical E1M1's original SKY1 composite through the Engine-owned
/// camera-relative background contract. Calibration rooms intentionally retain
/// their neutral clear background.
pub fn doom_sky_projection(
    project: &StoredProject,
    content_root: &Path,
) -> anyhow::Result<(Vec<RendererResource>, Vec<RenderDiff>)> {
    if project.project_id != "doom-e1m1" {
        return Ok((Vec::new(), Vec::new()));
    }
    let source = content_root.join(DOOM_SKY_SOURCE_PATH);
    let bytes = fs::read(&source).map_err(|error| {
        anyhow::anyhow!(
            "read checked-in Doom SKY1 panorama {}: {error}",
            source.display()
        )
    })?;
    let texture = TextureDescriptor::admit_png_rgba8_resource(
        DOOM_SKY_TEXTURE_ID.to_owned(),
        &bytes,
        TextureFilter::Nearest,
        TextureWrap::Clamp,
        1,
    )
    .map_err(|error| anyhow::anyhow!("admit Doom SKY1 panorama: {error:?}"))?;
    if texture.width != texture.height * 2 {
        anyhow::bail!(
            "Doom SKY1 panorama must be exact 2:1, got {}x{}",
            texture.width,
            texture.height
        );
    }
    let payload = texture
        .payload
        .as_ref()
        .expect("admitted SKY1 panorama has a retained payload");
    let TexturePayloadSource::Resource { resource } = &payload.source else {
        anyhow::bail!("Doom SKY1 panorama must use a retained resource");
    };
    let renderer_resource = RendererResource {
        identity: resource.clone(),
        content_hash: payload.content_hash.clone(),
        media_type: "image/png".to_owned(),
        bytes,
    };
    let background = SkyBackgroundDescriptor {
        texture: texture.id.clone(),
    };
    Ok((
        vec![renderer_resource],
        vec![
            RenderDiff::DefineTexture { texture },
            RenderDiff::SetSkyBackground {
                background: Some(background),
            },
        ],
    ))
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
    content_root: &Path,
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
            let bytes = fs::read(content_root.join(source))?;
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
    content_root: &Path,
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
            let source = content_root.join(source_path);
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
    content_root: &Path,
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
        let source = content_root.join(source_path);
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

#[cfg(test)]
mod tests {
    use rusty_engine::core_math::Vec3;
    use rusty_engine::engine_spatial::VoxelCollisionScene;
    use rusty_engine::entity_state::{EntityTransform, Quat};
    use rusty_engine::render_model::RenderDiff;
    use std::path::Path;

    use crate::{
        decode_project_document, project_stored_voxel_objects, DamageCommand, DamageService,
        DamageSource, GameRuntime, StoredDirectionalSpriteView, StoredVisualPresentation,
        StoredVisualState,
    };

    use super::{
        camera_relative_sprite_translation, project_doom_e1m1_application_content,
        select_directional_sprite_view, GameplayApplicationProjector,
    };

    #[test]
    fn e1m1_resolves_shared_actor_and_pickup_sprite_bindings_by_asset() {
        let source = include_str!("../../../../content/projects/doom-e1m1.project.json");
        let project = decode_project_document(source).unwrap().project;
        let projector = GameplayApplicationProjector::new(&project);
        let entities = &project.scenes[0].entities;

        let actor = entities
            .iter()
            .find(|entity| entity.name == "doom-zombieman-12")
            .unwrap();
        assert!(actor.renderable.as_ref().unwrap().visual_binding.is_none());
        let actor_states = &projector.bindings[&actor.id];
        assert!(matches!(
            actor_states.get(&StoredVisualState::Attacking),
            Some(StoredVisualPresentation::SpriteFrames {
                directional_views,
                ..
            }) if directional_views.len() == 8
        ));

        let pickup = entities
            .iter()
            .find(|entity| entity.name == "doom-pickup-2014-5")
            .unwrap();
        assert!(pickup.renderable.as_ref().unwrap().visual_binding.is_none());
        assert!(matches!(
            projector.bindings[&pickup.id].get(&StoredVisualState::Available),
            Some(StoredVisualPresentation::SpriteFrames {
                directional_views,
                ..
            }) if directional_views.len() == 8
        ));
    }

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
    fn combat_fx_source_origins_follow_billboard_camera_right_across_frames() {
        let anchor = [0.0, 1.0, 0.0];
        let front_camera = EntityTransform::at(Vec3::new(0.0, 0.0, 8.0));
        let side_camera = EntityTransform::at(Vec3::new(8.0, 0.0, 0.0));

        assert_eq!(
            camera_relative_sprite_translation(anchor, front_camera, [0.5, 0.25]),
            [0.5, 1.25, 0.0]
        );
        assert_eq!(
            camera_relative_sprite_translation(anchor, side_camera, [0.5, 0.25]),
            [0.0, 1.25, -0.5]
        );
        assert_eq!(
            camera_relative_sprite_translation(anchor, side_camera, [-0.25, 0.5]),
            [0.0, 1.5, 0.25]
        );
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
        let content = project_doom_e1m1_application_content(
            &project,
            &scene,
            &objects,
            &entities,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        )
        .unwrap();

        assert!(!content.frame.ops.is_empty());
        assert_eq!(
            content
                .resources
                .iter()
                .filter(|resource| resource.identity.starts_with("texture-resource/"))
                .count(),
            58
        );
        let sky_define = content
            .frame
            .ops
            .iter()
            .position(|operation| {
                matches!(
                    operation,
                    RenderDiff::DefineTexture { texture }
                        if texture.id == "texture/doom-sky1-panorama"
                )
            })
            .expect("canonical E1M1 defines SKY1");
        let sky_select = content
            .frame
            .ops
            .iter()
            .position(|operation| {
                matches!(
                    operation,
                    RenderDiff::SetSkyBackground {
                        background: Some(background)
                    } if background.texture == "texture/doom-sky1-panorama"
                )
            })
            .expect("canonical E1M1 selects SKY1");
        assert!(sky_define < sky_select);
        assert_eq!(
            content
                .frame
                .ops
                .iter()
                .filter(|operation| matches!(operation, RenderDiff::DefineSpriteAtlas { .. }))
                .count(),
            22
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
