//! Native product projection of the admitted stored voxel environment.

use std::collections::{BTreeMap, BTreeSet};

use rusty_engine::asset_catalog::{
    decode_catalog, AssetCatalog, StoredAssetCatalog, StoredAssetReference,
    StoredAssetVersionRequirement, StoredCatalogEntry, StoredMaterialDefinition,
    StoredTextureDefinition, StoredVoxelSurfaceMapping,
};
use rusty_engine::core_assets::AssetId;
use rusty_engine::engine_spatial::VoxelCollisionScene;
use rusty_engine::render_model::{
    MeshCollisionPolicy, MeshMaterialSlot, RenderDiff, RenderFrameDiff, StaticMeshAsset,
    StaticMeshInstanceDescriptor, Transform,
};
use rusty_engine::render_projection::{
    project_catalog_material, voxel_material_id, VoxelProjectionInstance, VoxelRenderProjector,
};

use crate::StoredProject;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVoxelVolumeProjectionError(String);

impl std::fmt::Display for StoredVoxelVolumeProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StoredVoxelVolumeProjectionError {}

/// Project the Rust-admitted collision scene through the Engine voxel renderer.
///
/// This is deliberately a concrete stored-project seam. It does not admit a
/// second renderer configuration language or give browser code render authority.
pub fn project_stored_voxel_volume(
    project: &StoredProject,
    scene: &VoxelCollisionScene,
) -> Result<RenderFrameDiff, StoredVoxelVolumeProjectionError> {
    let entry_scene = project
        .scenes
        .iter()
        .find(|candidate| candidate.id == project.entry_scene)
        .ok_or_else(|| failure("entry scene is missing"))?;
    if entry_scene.voxel_instances.len() != 1 {
        return Err(failure(format!(
            "native voxel projection requires one admitted environment instance, found {}",
            entry_scene.voxel_instances.len()
        )));
    }
    let instance = &entry_scene.voxel_instances[0];
    let asset = project
        .assets
        .iter()
        .find(|candidate| candidate.id == instance.voxel_asset_id)
        .and_then(|candidate| candidate.voxel_volume.as_ref())
        .ok_or_else(|| failure("voxel environment instance asset is missing"))?;
    let catalog = project_catalog(project)?;
    let used_slots = scene
        .material_voxels()
        .iter()
        .map(|voxel| voxel.material_slot)
        .collect::<BTreeSet<_>>();
    let material_assets = asset
        .material_palette
        .iter()
        .map(|binding| (binding.material_slot, binding.material_asset_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut materials = BTreeMap::new();
    for slot in used_slots {
        let asset_id = material_assets
            .get(&slot)
            .ok_or_else(|| failure(format!("voxel material slot {slot} is unresolved")))?;
        let parsed = AssetId::parse(asset_id).map_err(|error| failure(error.to_string()))?;
        let mut descriptor = project_catalog_material(&catalog, &parsed).map_err(|error| {
            failure(format!("material {asset_id} projection failed: {error:?}"))
        })?;
        descriptor.id = voxel_material_id(slot);
        materials.insert(slot, descriptor);
    }
    let projected = VoxelRenderProjector::new()
        .project(
            &[VoxelProjectionInstance {
                instance_id: instance.instance_id.clone(),
                asset_id: instance.voxel_asset_id.clone(),
                transform: Transform {
                    translation: instance.translation,
                    rotation: instance.rotation,
                    scale: instance.scale,
                },
                scene,
            }],
            &materials,
        )
        .map_err(|error| failure(format!("Engine voxel projection failed: {error:?}")))?;
    retain_voxel_chunks_as_static_meshes(projected.frame)
}

fn retain_voxel_chunks_as_static_meshes(
    frame: RenderFrameDiff,
) -> Result<RenderFrameDiff, StoredVoxelVolumeProjectionError> {
    let payloads = frame
        .ops
        .iter()
        .filter_map(|operation| match operation {
            RenderDiff::ReplaceMeshPayload { handle, payload } => Some((*handle, payload.clone())),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let nodes = frame
        .ops
        .iter()
        .filter_map(|operation| match operation {
            RenderDiff::Create {
                handle,
                parent,
                node,
            } if payloads.contains_key(handle) => Some((*handle, (*parent, node.clone()))),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut operations = Vec::with_capacity(frame.ops.len() + payloads.len());
    for operation in frame.ops {
        match operation {
            RenderDiff::Create { handle, .. } if payloads.contains_key(&handle) => {}
            RenderDiff::ReplaceMeshPayload { handle, payload } => {
                let (parent, node) = nodes
                    .get(&handle)
                    .cloned()
                    .ok_or_else(|| failure(format!("voxel chunk {handle:?} has no root node")))?;
                let asset_id = format!("mesh/doom-e1m1-chunk-{}", handle.raw());
                let material_slots = payload
                    .groups
                    .iter()
                    .map(|group| group.material_slot)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .map(|slot| MeshMaterialSlot {
                        slot,
                        material: voxel_material_id(slot),
                    })
                    .collect();
                operations.push(RenderDiff::DefineStaticMesh {
                    asset: StaticMeshAsset {
                        asset: asset_id.clone(),
                        payload,
                        material_slots,
                        collision: MeshCollisionPolicy::VisualOnly,
                    },
                });
                operations.push(RenderDiff::CreateStaticMeshInstance {
                    handle,
                    parent,
                    instance: StaticMeshInstanceDescriptor {
                        asset: asset_id,
                        transform: node.transform,
                        visible: node.visible,
                        material_overrides: Vec::new(),
                        metadata: node.metadata,
                    },
                });
            }
            other => operations.push(other),
        }
    }
    RenderFrameDiff::try_from_ops(operations)
        .map_err(|error| failure(format!("retained Doom frame is invalid: {error:?}")))
}

fn project_catalog(
    project: &StoredProject,
) -> Result<AssetCatalog, StoredVoxelVolumeProjectionError> {
    let entries = project
        .assets
        .iter()
        .filter(|asset| asset.material.is_some() || asset.id.starts_with("texture/doom-"))
        .map(|asset| {
            let metadata = asset.catalog.as_ref();
            let dependencies = metadata.map_or_else(
                || {
                    asset
                        .voxel_volume
                        .iter()
                        .flat_map(|voxel| &voxel.material_palette)
                        .map(|binding| StoredAssetReference {
                            id: binding.material_asset_id.clone(),
                            version: StoredAssetVersionRequirement::Exact { value: 1 },
                            hash: None,
                        })
                        .collect()
                },
                |metadata| metadata.dependencies.clone(),
            );
            StoredCatalogEntry {
                id: asset.id.clone(),
                version: metadata.map_or(1, |metadata| metadata.version),
                hash: metadata.and_then(|metadata| normalized_hash(metadata.hash.as_deref())),
                source_path: metadata.and_then(|metadata| metadata.source_path.clone()),
                label: metadata.and_then(|metadata| metadata.label.clone()),
                dependencies: dependencies.into_iter().map(normalize_reference).collect(),
                material: asset.material.clone().map(normalize_material),
                texture: asset
                    .id
                    .starts_with("texture/doom-")
                    .then_some(StoredTextureDefinition {
                        width: 64,
                        height: 64,
                        filter: "nearest".to_owned(),
                        wrap: "repeat".to_owned(),
                    }),
                voxel_atlas: None,
            }
        })
        .collect();
    let encoded = serde_json::to_string(&StoredAssetCatalog { entries })
        .map_err(|error| failure(error.to_string()))?;
    decode_catalog(&encoded)
        .map(|catalog| catalog.canonical())
        .map_err(|error| failure(format!("project catalog admission failed: {error}")))
}

fn normalize_material(mut material: StoredMaterialDefinition) -> StoredMaterialDefinition {
    material.style.texture = material.style.texture.map(normalize_reference);
    if let Some(surface) = &mut material.style.voxel_surface {
        match &mut surface.mapping {
            StoredVoxelSurfaceMapping::Repeat { texture, .. } => {
                *texture = normalize_reference(texture.clone());
            }
            StoredVoxelSurfaceMapping::Atlas { atlas, .. } => {
                *atlas = normalize_reference(atlas.clone());
            }
        }
    }
    material
}

fn normalize_reference(mut reference: StoredAssetReference) -> StoredAssetReference {
    reference.hash = normalized_hash(reference.hash.as_deref());
    reference
}

fn normalized_hash(hash: Option<&str>) -> Option<String> {
    hash.map(|value| value.strip_prefix("sha256:").unwrap_or(value).to_owned())
}

fn failure(message: impl Into<String>) -> StoredVoxelVolumeProjectionError {
    StoredVoxelVolumeProjectionError(message.into())
}
