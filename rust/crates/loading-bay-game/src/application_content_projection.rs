//! Complete E1M1 application content projected from admitted Rust state.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use rusty_engine::engine_spatial::VoxelCollisionScene;
use rusty_engine::render_model::{
    pack_mesh_resources, AnimatedMeshPlaybackCommand, AnimationLoopMode, RenderAssetKind,
    RenderDiff, RenderFrameDiff, ResolvedRenderAsset, TextureDescriptor, TextureFilter,
    TextureWrap, MAX_MESH_RESOURCE_BYTES,
};
use rusty_engine::render_projection::EntityRenderProjector;
use rusty_engine::renderer_webview_host::RendererResource;

use crate::{
    project_stored_voxel_volume, EnemyCombatPosture, EnemyState, GameRuntime, StoredProject,
    StoredVisualAnimationLoopMode, StoredVisualPresentation, StoredVisualState,
};

#[derive(Debug, Clone)]
pub struct ProjectedApplicationContent {
    pub frame: RenderFrameDiff,
    pub resources: Vec<RendererResource>,
}

#[derive(Debug, Clone)]
pub struct GameplayApplicationProjector {
    entities: EntityRenderProjector,
    assets: BTreeMap<String, ResolvedRenderAsset>,
    bindings: BTreeMap<u64, BTreeMap<StoredVisualState, StoredVisualPresentation>>,
    visual_states: BTreeMap<u64, StoredVisualState>,
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
        Self {
            entities: EntityRenderProjector::new(),
            assets,
            bindings,
            visual_states: BTreeMap::new(),
        }
    }

    pub fn project(&mut self, runtime: &GameRuntime) -> anyhow::Result<RenderFrameDiff> {
        let projected = self
            .entities
            .project(runtime.session().entities(), &self.assets)
            .map_err(|error| anyhow::anyhow!("project live gameplay entities: {error:?}"))?;
        let mut operations = projected.frame.ops;
        for (entity, states) in &self.bindings {
            let id = rusty_engine::core_ids::EntityId::new(*entity);
            let Some(combat) = runtime.session().enemy_combat(id) else {
                continue;
            };
            let desired = if runtime
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
            };
            if self.visual_states.get(entity) == Some(&desired) {
                continue;
            }
            let Some(StoredVisualPresentation::Animation {
                clip,
                loop_mode,
                speed,
                fade_seconds,
            }) = states.get(&desired)
            else {
                continue;
            };
            let Some(handle) = self.entities.handle_of(id) else {
                continue;
            };
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
        RenderFrameDiff::try_from_ops(operations)
            .map_err(|error| anyhow::anyhow!("build live gameplay frame: {error:?}"))
    }

    pub fn project_current(&self, runtime: &GameRuntime) -> anyhow::Result<RenderFrameDiff> {
        let mut current = self.clone();
        current.visual_states.clear();
        current.project(runtime)
    }
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
    let volume_frame = project_stored_voxel_volume(project, scene)?;
    let (volume_frame, mut resources) = externalize_frame_meshes(volume_frame)?;
    let (texture_resources, texture_ops) = doom_texture_projection(project)?;
    if texture_resources.len() != 54 {
        anyhow::bail!(
            "Doom E1M1 application content requires 54 textures, found {}",
            texture_resources.len()
        );
    }
    resources.extend(texture_resources);
    let (static_frame, static_resources) = static_mesh_projection(project)?;
    resources.extend(static_resources);
    let (animated_resources, animated_ops) = animated_mesh_projection(project)?;
    resources.extend(animated_resources);

    let mut operations = texture_ops;
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
        .filter(|asset| asset.id.starts_with("texture/doom-"))
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
    use rusty_engine::engine_spatial::VoxelCollisionScene;

    use crate::{
        decode_project_document, project_stored_voxel_objects, DamageCommand, DamageService,
        DamageSource, GameRuntime,
    };
    use rusty_engine::render_model::{AnimatedMeshPlaybackCommand, RenderDiff};

    use super::{project_doom_e1m1_application_content, GameplayApplicationProjector};

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
            54
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
    fn gameplay_projection_emits_authored_hit_and_death_clips_without_hiding_the_enemy() {
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
        let hit = projector.project_current(&runtime).unwrap();
        assert!(hit.ops.iter().any(|operation| matches!(
            operation,
            RenderDiff::SetAnimatedMeshPlayback {
                playback: AnimatedMeshPlaybackCommand::Play { clip, restart: false, .. },
                ..
            } if clip == "hit"
        )));
        assert_eq!(projector.project_current(&runtime).unwrap(), hit);

        DamageService::apply(
            runtime.session_mut(),
            DamageCommand {
                source: DamageSource::Direct { actor: enemy },
                target: enemy,
                amount: 1_000,
            },
        )
        .unwrap();
        let death = projector.project_current(&runtime).unwrap();
        assert!(death.ops.iter().any(|operation| matches!(
            operation,
            RenderDiff::SetAnimatedMeshPlayback {
                playback: AnimatedMeshPlaybackCommand::Play { clip, restart: false, .. },
                ..
            } if clip == "death"
        )));
        assert!(!death.ops.iter().any(|operation| matches!(
            operation,
            RenderDiff::Update {
                visible: Some(false),
                ..
            }
        )));
    }
}
