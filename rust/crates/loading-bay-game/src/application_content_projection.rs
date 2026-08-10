//! Complete E1M1 application content projected from admitted Rust state.

use std::fs;
use std::path::PathBuf;

use rusty_engine::engine_spatial::VoxelCollisionScene;
use rusty_engine::render_model::{
    pack_mesh_resources, RenderDiff, RenderFrameDiff, TextureDescriptor, TextureFilter,
    TextureWrap, MAX_MESH_RESOURCE_BYTES,
};
use rusty_engine::renderer_webview_host::RendererResource;

use crate::{project_stored_voxel_volume, StoredProject};

#[derive(Debug, Clone)]
pub struct ProjectedApplicationContent {
    pub frame: RenderFrameDiff,
    pub resources: Vec<RendererResource>,
}

/// Build the complete immutable E1M1 frame/resource closure consumed by an
/// Engine-owned application surface. TypeScript transports these facts but
/// does not derive renderer manifests or backend configuration.
pub fn project_doom_e1m1_application_content(
    project: &StoredProject,
    scene: &VoxelCollisionScene,
    object_frame: &RenderFrameDiff,
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

    let mut operations = texture_ops;
    operations.extend(volume_frame.ops);
    operations.extend(object_frame.ops.iter().cloned());
    let frame = RenderFrameDiff::try_from_ops(operations)
        .map_err(|error| anyhow::anyhow!("build complete E1M1 application frame: {error:?}"))?;
    Ok(ProjectedApplicationContent { frame, resources })
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

    use crate::{decode_project_document, project_stored_voxel_objects, GameRuntime};

    use super::project_doom_e1m1_application_content;

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
        let content = project_doom_e1m1_application_content(&project, &scene, &objects).unwrap();

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
    }
}
