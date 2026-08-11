use std::collections::{BTreeMap, BTreeSet};

use rusty_engine::asset_catalog::{
    decode_catalog, encode_catalog, generate_lock, validate_catalog, AssetCatalog,
    StoredAssetCatalog, StoredAssetReference, StoredAssetVersionRequirement, StoredCatalogEntry,
    UvStrategy,
};
use rusty_engine::authored_scene::{
    composed_world_transforms, encode_scene, validate_scene, AvailableSceneAsset,
    FlatSceneDocument, NodeMetadata, SceneAdmissionPlan, SceneEditCommand, SceneEditService,
    SceneLight, SceneLightShadowIntent, SceneMetadata, SceneNode, SceneNodeKind, SceneNodeRecord,
    SceneResolutionContext, SceneTransform, CURRENT_SCENE_SCHEMA_VERSION,
};
use rusty_engine::content_store::{
    admit_source_batch, encode_manifest, ArtifactRole, ContentArtifact, ContentBody, ContentHash,
    ContentManifest, ContentSourceBatch, ContentStoreIdentity, ContentWrite, ContentWriteCandidate,
    ContentWriteSetDraft,
};
use rusty_engine::core_assets::{AssetId, AssetKind, AssetReference, AssetVersionReq};
use rusty_engine::core_ids::{SceneId, SceneNodeId};
use rusty_engine::core_math::Vec3;
use rusty_engine::engine_inspector::{
    inspect_catalog, inspect_content_manifest, inspect_entity_state, inspect_scene,
    inspect_voxel_state, NamedCount,
};
use rusty_engine::entity_state::{encode_durable_snapshot, EntityState, EntityTransform, Quat};
use rusty_engine::render_model::{
    AnimatedMeshPlaybackCommand, AnimationLoopMode, LightDescriptor, LightShadowIntent,
    MaterialUvStrategy, MeshAttribute, MeshAttributeKind, MeshAttributeName, MeshBoundsDescriptor,
    MeshBufferLayout, MeshCollisionPolicy, MeshGroupDescriptor, MeshIndexWidth, MeshMaterialSlot,
    MeshPayloadDescriptor, MeshPayloadSource, MeshProvenance, RenderAssetKind, RenderDiff,
    RenderFrameDiff, RenderMaterialDescriptor, RenderMetadata, ResolvedRenderAsset,
    StaticMeshAsset, Transform,
};
use rusty_engine::render_projection::{
    AppearanceLight, AppearanceScene, EntityProjectionDiagnostic, EntityProjectionReadout,
    EntityRenderProjector, ProjectionAvailability, ProjectionMode, SceneAppearanceProjector,
    VoxelObjectProjectionInstance, VoxelObjectRenderProjector,
};
use rusty_engine::voxel_asset::{VoxelFrame, VoxelObjectAsset};
use rusty_engine::voxel_convert::VoxelObjectFrameSelection;
use rusty_engine::voxel_object_runtime::{admit_voxel_object, VoxelObjectRuntimeLimits};

use crate::stored_project::validate_voxel_object_aggregate_budget;
use crate::weapon_authoring::loading_bay_weapon_owner_entity_ids;
use crate::{
    admit_stored_project_with_document, encode_project_document, project_stored_voxel_objects_with,
    AdmittedProject, AdmittedStoredProject, DecodedProjectDocument, ProjectSaveMode, ProjectStore,
    StoredAssetImport, StoredCollision, StoredEntityDefinition, StoredKinematic, StoredLight,
    StoredProject, StoredRenderable, StoredRenderableTransform, StoredScene,
    LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID, LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
    LOADING_BAY_WEAPON_COMPONENT_TYPE_ID, STORED_PROJECT_SCHEMA_VERSION,
};

use super::path::ProjectLocation;
use super::protocol::{
    AdapterRejection, AnimatedMeshResourceReadout, AssetBrowserReadout, AssetEntryReadout,
    AssetImportReadout, AssetLockEntryReadout, CanonicalOwnerContent,
    EmptyVoxelSurfaceAuthoringReadout, EntityTranslationReceipt, OwnerInspections,
    ProjectMutationReceipt, ProjectionDiagnosticReadout, ProjectionReadout,
    SceneHierarchyNodeReadout, SceneHierarchyReadout, StudioEntityComponentReference,
    StudioEntityInspectorContractIdentity, StudioProjectIdentity, StudioProjectReadout,
    StudioSceneAppearance, StudioSceneObjectDraft, StudioVoxelObjectInstance, TransformReadout,
    VoxelObjectAssetAuthoringReadout, VoxelObjectAuthoringReadout, VoxelObjectClipAuthoringReadout,
    VoxelObjectFrameAuthoringReadout, VoxelObjectGridReadout, VoxelObjectInstanceReadout,
    VoxelObjectProvenanceReadout, MAX_STUDIO_ENTITY_COMPONENTS_PER_OWNER,
    MAX_STUDIO_ENTITY_COMPONENT_REFERENCES, VOXEL_OBJECT_COMPONENT_TYPE_ID,
    VOXEL_OBJECT_INSPECTOR_CONTRACT_ID, VOXEL_OBJECT_INSPECTOR_CONTRACT_VERSION,
};
use super::voxel::{project_voxel_authoring, voxel_authoring_readout};

const PROJECT_RESOURCE_ROLE: &str = "resource:loading-bay-project";
const JSON_SAFE_U64_MASK: u64 = (1_u64 << 53) - 1;

pub struct OpenedOwnerProject {
    location: ProjectLocation,
    source_schema_version: u32,
    source_hash: ContentHash,
    stored: AdmittedStoredProject,
    admitted: AdmittedProject,
    catalog: AssetCatalog,
    scene: FlatSceneDocument,
    manifest: ContentManifest,
    relative_project_file: String,
}

impl OpenedOwnerProject {
    pub fn load(location: &ProjectLocation) -> Result<Self, AdapterRejection> {
        location
            .revalidate()
            .map_err(|error| reject("path.rejected", error.to_string()))?;
        let source = ProjectStore::default()
            .load_source(location.project_file())
            .map_err(project_store_rejection)?;
        let source_schema_version = source.decoded.source_schema_version;
        Self::admit_source(
            location,
            source.source_bytes().to_string(),
            source.decoded,
            source_schema_version,
        )
    }

    fn admit_source(
        location: &ProjectLocation,
        source_bytes: String,
        decoded: DecodedProjectDocument,
        source_schema_version: u32,
    ) -> Result<Self, AdapterRejection> {
        let source_hash = ContentHash::of(source_bytes.as_bytes());
        let relative_project_file = location.relative_project_file();
        let manifest = project_manifest(relative_project_file, source_bytes.as_bytes());
        let manifest_json = encode_manifest(&manifest)
            .map_err(|error| reject("content.manifestEncode", error.to_string()))?;
        admit_source_batch(ContentSourceBatch {
            manifest_json,
            bodies: vec![ContentBody::new(
                relative_project_file,
                source_bytes.as_bytes(),
            )],
        })
        .map_err(|error| reject("content.sourceRejected", error.to_string()))?;

        let (stored, admitted) = admit_stored_project_with_document(decoded.project)
            .map_err(stored_project_rejection)?;
        let catalog = project_catalog(stored.document())?;
        let scene = project_scene(stored.document(), source_hash)?;
        validate_owner_admission(&catalog, &scene)?;

        Ok(Self {
            location: location.clone(),
            source_schema_version,
            source_hash,
            stored,
            admitted,
            catalog,
            scene,
            manifest,
            relative_project_file: relative_project_file.to_string(),
        })
    }

    pub fn scene_revision(&self) -> u64 {
        self.scene.revision
    }

    pub(crate) fn document(&self) -> &StoredProject {
        self.stored.document()
    }

    pub(crate) fn source_hash(&self) -> ContentHash {
        self.source_hash
    }

    pub(crate) fn catalog(&self) -> &AssetCatalog {
        &self.catalog
    }

    pub(crate) fn scene(&self) -> &FlatSceneDocument {
        &self.scene
    }

    pub fn readout(&self) -> Result<StudioProjectReadout, AdapterRejection> {
        self.readout_with_voxel_object_projector(&mut VoxelObjectRenderProjector::new())
    }

    pub(crate) fn readout_with_voxel_object_projector(
        &self,
        voxel_object_projector: &mut VoxelObjectRenderProjector,
    ) -> Result<StudioProjectReadout, AdapterRejection> {
        let project = self.stored.document();
        let catalog_json = encode_catalog(&self.catalog)
            .map_err(|error| reject("catalog.encode", error.to_string()))?;
        let scene_json =
            encode_scene(&self.scene).map_err(|error| reject("scene.encode", error.to_string()))?;
        let entity_state_json = encode_durable_snapshot(self.admitted.session.entities())
            .map_err(|error| reject("entityState.encode", error.to_string()))?;
        let manifest_json = encode_manifest(&self.manifest)
            .map_err(|error| reject("content.manifestEncode", error.to_string()))?;
        let canonical_project_json =
            encode_project_document(project).map_err(stored_project_rejection)?;

        let render_assets = resolved_render_assets(&self.catalog);
        let projected = EntityRenderProjector::new()
            .project(self.admitted.session.entities(), &render_assets)
            .map_err(|error| reject("projection.rejected", format!("{error:?}")))?;
        let diagnostics = projected
            .diagnostics
            .into_iter()
            .map(projection_diagnostic)
            .collect();

        let (projection, projection_readout) = self.compose_projection(
            projected.frame,
            projected.readout,
            diagnostics,
            None,
            voxel_object_projector,
            None,
        )?;
        let catalog_lock = generate_lock(&self.catalog);
        let scene_hierarchy = scene_hierarchy(&self.scene, self.admitted.session.entities());
        let mut scene_inspection = inspect_scene(&self.scene, Some(&self.catalog));
        let composed_node_count = scene_hierarchy.nodes.len();
        let derived_entity_count = composed_node_count.saturating_sub(scene_inspection.node_count);
        scene_inspection.node_count = composed_node_count;
        scene_inspection.root_count = scene_hierarchy.root_node_ids.len();
        if derived_entity_count > 0 {
            if let Some(kind) = scene_inspection
                .node_kinds
                .iter_mut()
                .find(|kind| kind.name == "entityInstance")
            {
                kind.count += derived_entity_count;
            } else {
                scene_inspection.node_kinds.push(NamedCount {
                    name: "entityInstance".to_string(),
                    count: derived_entity_count,
                });
            }
        }
        let entity_components = entity_component_references(project, &scene_hierarchy)?;

        Ok(StudioProjectReadout {
            identity: StudioProjectIdentity {
                project_id: project.project_id.clone(),
                name: project.name.clone(),
                entry_scene: project.entry_scene.clone(),
                source_schema_version: self.source_schema_version,
                current_schema_version: STORED_PROJECT_SCHEMA_VERSION,
                project_hash: self.source_hash.to_hex(),
                scene_revision: self.scene.revision,
                relative_project_file: self.relative_project_file.clone(),
            },
            canonical: CanonicalOwnerContent {
                project_json: canonical_project_json,
                asset_catalog_json: catalog_json,
                authored_scene_json: scene_json,
                entity_state_json,
                content_manifest_json: manifest_json,
            },
            inspections: OwnerInspections {
                catalog: inspect_catalog(&self.catalog, Some(&catalog_lock)),
                scene: scene_inspection,
                entity_state: inspect_entity_state(self.admitted.session.entities()).into(),
                persistence: inspect_content_manifest(&self.manifest),
            },
            scene_hierarchy,
            asset_browser: asset_browser_readout(project, &self.catalog, &self.location),
            voxel: self
                .admitted
                .collision_scene
                .as_ref()
                .map(inspect_voxel_state),
            voxel_authoring: voxel_authoring_readout(project)?,
            voxel_object_authoring: voxel_object_authoring_readout(project),
            voxel_surface_authoring: EmptyVoxelSurfaceAuthoringReadout {
                textures: [],
                atlases: [],
                materials: [],
            },
            texture_resources: [],
            animated_mesh_resources: animated_mesh_resources(project)?,
            entity_components,
            projection,
            projection_readout,
        })
    }

    pub(crate) fn voxel_object_candidate_projection(
        &self,
        candidate: &VoxelObjectAsset,
        frame: &VoxelObjectFrameSelection,
    ) -> Result<(RenderFrameDiff, ProjectionReadout), AdapterRejection> {
        self.validate_voxel_object_candidate(candidate)?;
        let render_assets = resolved_render_assets(&self.catalog);
        let projected = EntityRenderProjector::new()
            .project(self.admitted.session.entities(), &render_assets)
            .map_err(|error| reject("projection.rejected", format!("{error:?}")))?;
        let diagnostics = projected
            .diagnostics
            .into_iter()
            .map(projection_diagnostic)
            .collect();
        self.compose_projection(
            projected.frame,
            projected.readout,
            diagnostics,
            Some((candidate, frame)),
            &mut VoxelObjectRenderProjector::new(),
            None,
        )
    }

    pub(crate) fn voxel_object_placement_resource(
        &self,
        asset_id: &str,
        expected_object_content_hash: &str,
    ) -> Result<RenderFrameDiff, AdapterRejection> {
        let expected_digest = expected_object_content_hash
            .strip_prefix("sha256:")
            .ok_or_else(|| {
                reject(
                    "voxelObject.invalidContentHash",
                    "object content hash must use the sha256:<lowercase-hex> form",
                )
            })?;
        ContentHash::parse(expected_digest)
            .map_err(|error| reject("voxelObject.invalidContentHash", error.to_string()))?;
        let object = self
            .stored
            .document()
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| {
                reject(
                    "voxelObject.assetMissing",
                    format!("project has no asset `{asset_id}`"),
                )
            })?
            .voxel_object
            .as_ref()
            .ok_or_else(|| {
                reject(
                    "voxelObject.assetKindMismatch",
                    format!("asset `{asset_id}` is not a voxel object"),
                )
            })?;
        if object.content_hash != expected_object_content_hash {
            return Err(reject(
                "voxelObject.staleContent",
                format!(
                    "expected object content hash {expected_object_content_hash}, found {}",
                    object.content_hash
                ),
            ));
        }

        let admitted = admit_voxel_object(object, VoxelObjectRuntimeLimits::default())
            .map_err(|error| reject("voxelObject.admissionRejected", error.to_string()))?;
        let projected = VoxelObjectRenderProjector::new()
            .project(
                &[VoxelObjectProjectionInstance {
                    instance_id: "studio-voxel-object-placement-resource".to_string(),
                    object: &admitted,
                    frame: 0,
                    transform: Transform::IDENTITY,
                    visible: true,
                    material_overrides: Vec::new(),
                    metadata: RenderMetadata::default(),
                }],
                &project_material_descriptors(&self.catalog),
            )
            .map_err(|error| reject("projection.voxelObjectRejected", format!("{error:?}")))?;
        let operations = projected
            .frame
            .ops
            .into_iter()
            .filter(|operation| {
                matches!(
                    operation,
                    RenderDiff::DefineMaterial { .. }
                        | RenderDiff::DefineTexture { .. }
                        | RenderDiff::DefineVoxelObject { .. }
                )
            })
            .collect::<Vec<_>>();
        if operations.len() > 513 {
            return Err(reject(
                "voxelObject.placementResourceLimit",
                format!(
                    "placement resource frame has {} definitions, exceeding the 513-operation protocol bound",
                    operations.len()
                ),
            ));
        }
        RenderFrameDiff::try_from_ops(operations)
            .map_err(|error| reject("projection.voxelObjectRejected", format!("{error:?}")))
    }

    pub(crate) fn project_voxel_object_frame(
        &self,
        projector: &mut VoxelObjectRenderProjector,
        instance_id: &str,
        runtime_frame: u32,
    ) -> Result<(RenderFrameDiff, ProjectionReadout), AdapterRejection> {
        let render_assets = resolved_render_assets(&self.catalog);
        let retained_entities = EntityRenderProjector::new()
            .project(self.admitted.session.entities(), &render_assets)
            .map_err(|error| reject("projection.rejected", format!("{error:?}")))?
            .readout
            .retained_entities;
        let projected = project_voxel_objects(
            self.stored.document(),
            None,
            projector,
            Some((instance_id, runtime_frame)),
        )?;
        let voxel_projected = project_voxel_authoring(self.stored.document(), &self.catalog)?;
        let retained_lights = self
            .scene
            .nodes
            .iter()
            .filter(|node| matches!(node.kind, SceneNodeKind::Light(_)))
            .count();
        Ok((
            projected.frame,
            ProjectionReadout {
                frame_kind: "incremental",
                source_revision: self.scene.revision,
                retained_entities,
                retained_lights,
                retained_voxel_instances: voxel_projected.instance_count,
                retained_voxel_chunks: voxel_projected.chunk_count,
                diagnostics: Vec::new(),
            },
        ))
    }

    pub(crate) fn prime_voxel_object_projector(
        &self,
        projector: &mut VoxelObjectRenderProjector,
    ) -> Result<(), AdapterRejection> {
        project_voxel_objects(self.stored.document(), None, projector, None)?;
        Ok(())
    }

    pub(crate) fn validate_voxel_object_candidate(
        &self,
        candidate: &VoxelObjectAsset,
    ) -> Result<(), AdapterRejection> {
        validate_voxel_object_aggregate_budget(self.stored.document(), Some(candidate))
            .map_err(stored_project_rejection)
    }

    fn compose_projection(
        &self,
        entity_frame: RenderFrameDiff,
        entity_readout: EntityProjectionReadout,
        diagnostics: Vec<ProjectionDiagnosticReadout>,
        candidate: Option<(&VoxelObjectAsset, &VoxelObjectFrameSelection)>,
        voxel_object_projector: &mut VoxelObjectRenderProjector,
        frame_override: Option<(&str, u32)>,
    ) -> Result<(RenderFrameDiff, ProjectionReadout), AdapterRejection> {
        let project = self.stored.document();
        let voxel_projected = project_voxel_authoring(project, &self.catalog)?;
        let object_projected =
            project_voxel_objects(project, candidate, voxel_object_projector, frame_override)?;
        let light_projection = project_scene_lights(&self.scene)?;
        let retained_lights = self
            .scene
            .nodes
            .iter()
            .filter(|node| matches!(node.kind, SceneNodeKind::Light(_)))
            .count();
        let entity_projection = install_animation_playback(project, entity_frame)?;
        let projection = complete_projection(
            project,
            &self.catalog,
            entity_projection,
            light_projection,
            voxel_projected.frame,
            object_projected.frame,
        )?;
        Ok((
            projection,
            ProjectionReadout {
                frame_kind: "complete",
                source_revision: entity_readout.source_revision,
                retained_entities: entity_readout.retained_entities,
                retained_lights,
                retained_voxel_instances: voxel_projected.instance_count,
                retained_voxel_chunks: voxel_projected.chunk_count,
                diagnostics,
            },
        ))
    }
}

fn asset_browser_readout(
    project: &StoredProject,
    catalog: &AssetCatalog,
    location: &ProjectLocation,
) -> AssetBrowserReadout {
    let mut dependents = BTreeMap::<String, Vec<String>>::new();
    for entry in catalog.iter() {
        for dependency in &entry.dependencies {
            dependents
                .entry(dependency.id().as_str().to_string())
                .or_default()
                .push(entry.id.as_str().to_string());
        }
    }
    let assets = catalog
        .canonical()
        .entries
        .into_iter()
        .map(|entry| {
            let stored = project
                .assets
                .iter()
                .find(|asset| asset.id == entry.id.as_str());
            AssetEntryReadout {
                asset_id: entry.id.as_str().to_string(),
                kind: entry.kind().prefix().to_string(),
                version: entry.version,
                hash: entry.hash.map(|hash| hash.as_str().to_string()),
                source_path: entry.source_path,
                label: entry.label,
                dependencies: entry
                    .dependencies
                    .into_iter()
                    .map(|dependency| dependency.id().as_str().to_string())
                    .collect(),
                dependents: dependents.remove(entry.id.as_str()).unwrap_or_default(),
                material: entry.material.is_some(),
                imported_mesh: stored.is_some_and(|asset| asset.static_mesh.is_some()),
                import: stored
                    .and_then(|asset| asset.import.as_ref())
                    .map(|import| AssetImportReadout {
                        source: import.source.clone(),
                        source_hash: import.source_hash.clone(),
                        source_byte_count: import.source_byte_count,
                        importer_version: import.importer_version,
                        generated_asset_ids: import.generated_asset_ids.clone(),
                        status: import_status(location, import),
                    }),
            }
        })
        .collect();
    let lock_entries = generate_lock(catalog)
        .entries
        .into_iter()
        .map(|entry| AssetLockEntryReadout {
            asset_id: entry.id.as_str().to_string(),
            kind: entry.kind.prefix().to_string(),
            version: entry.version,
            hash: entry.hash.map(|hash| hash.as_str().to_string()),
            dependencies: entry
                .dependencies
                .into_iter()
                .map(|dependency| dependency.as_str().to_string())
                .collect(),
        })
        .collect();
    AssetBrowserReadout {
        assets,
        lock_entries,
    }
}

fn import_status(location: &ProjectLocation, import: &StoredAssetImport) -> &'static str {
    super::asset_import::import_source_status(location, import)
}

pub fn apply_entity_translation(
    location: &ProjectLocation,
    expected_project_hash: &str,
    expected_scene_revision: u64,
    entity_id: u64,
    translation: [f32; 3],
) -> Result<(EntityTranslationReceipt, StudioProjectReadout), AdapterRejection> {
    let published = publish_project_mutation(
        location,
        expected_project_hash,
        |current, document_candidate| {
            if current.scene.revision != expected_scene_revision {
                return Err(reject(
                    "scene.staleRevision",
                    format!(
                        "expected scene revision {expected_scene_revision}, found {}",
                        current.scene.revision
                    ),
                ));
            }

            let node_id = SceneNodeId::new(entity_id);
            let Some(source_node) = current.scene.nodes.iter().find(|node| node.id == node_id)
            else {
                return Err(reject(
                    "scene.missingEntity",
                    format!("entry scene has no entity {entity_id}"),
                )
                .at_path(format!("entities[{entity_id}]")));
            };
            let mut scene_candidate = current.scene.clone();
            let owner_transform = SceneTransform {
                translation: Vec3::new(translation[0], translation[1], translation[2]),
                ..source_node.transform
            };
            SceneEditService
                .apply(
                    &mut scene_candidate,
                    expected_scene_revision,
                    SceneEditCommand::SetTransform {
                        id: node_id,
                        transform: owner_transform,
                    },
                )
                .map_err(|error| reject(error.code(), error.to_string()))?;

            let scene = entry_scene_mut(document_candidate);
            let Some(entity) = scene
                .entities
                .iter_mut()
                .find(|entity| entity.id == entity_id)
            else {
                return Err(reject(
                    "project.entityMappingMismatch",
                    format!("authored scene entity {entity_id} has no Loading Bay record"),
                ));
            };
            entity.translation = Some(translation);
            if let Some(instance) = scene
                .voxel_object_instances
                .iter_mut()
                .find(|instance| instance.owner_entity_id == entity_id)
            {
                instance.translation = translation;
            }
            Ok(())
        },
    )?;

    let receipt = EntityTranslationReceipt {
        entity_id,
        translation,
        project_hash_before: published.project_hash_before.to_hex(),
        project_hash_after: published.project_hash_after.to_hex(),
        scene_revision_before: expected_scene_revision,
        scene_revision_after: published.scene_revision_after,
        content_candidate_hash: published.content_candidate_hash.to_hex(),
    };
    Ok((receipt, published.readout))
}

pub fn create_project(
    location: &ProjectLocation,
    project_id: String,
    name: String,
    entry_scene: String,
    entry_scene_name: String,
) -> Result<StudioProjectReadout, AdapterRejection> {
    let document = StoredProject {
        schema_version: STORED_PROJECT_SCHEMA_VERSION,
        project_id,
        name,
        entry_scene: entry_scene.clone(),
        assets: Vec::new(),
        item_definitions: Vec::new(),
        scenes: vec![StoredScene {
            id: entry_scene,
            name: entry_scene_name,
            voxel_environment: None,
            voxel_instances: Vec::new(),
            voxel_object_instances: Vec::new(),
            entities: Vec::new(),
        }],
    };
    publish_new_project(location, document)
}

pub fn save_project_as(
    source: &ProjectLocation,
    target: &ProjectLocation,
    expected_project_hash: &str,
    project_id: String,
    name: String,
) -> Result<StudioProjectReadout, AdapterRejection> {
    let expected_hash = ContentHash::parse(expected_project_hash)
        .map_err(|error| reject("project.invalidHash", error.to_string()))?;
    let current = OpenedOwnerProject::load(source)?;
    if current.source_hash != expected_hash {
        return Err(reject(
            "project.staleHash",
            format!(
                "expected project hash {expected_hash}, found {}",
                current.source_hash
            ),
        ));
    }
    let mut document = current.stored.document().clone();
    document.schema_version = STORED_PROJECT_SCHEMA_VERSION;
    document.project_id = project_id;
    document.name = name;
    publish_new_project(target, document)
}

fn publish_new_project(
    location: &ProjectLocation,
    document: StoredProject,
) -> Result<StudioProjectReadout, AdapterRejection> {
    let (stored, _) =
        admit_stored_project_with_document(document).map_err(stored_project_rejection)?;
    let candidate_bytes =
        encode_project_document(stored.document()).map_err(stored_project_rejection)?;
    let candidate_decoded = DecodedProjectDocument {
        project: stored.document().clone(),
        source_schema_version: STORED_PROJECT_SCHEMA_VERSION,
    };
    let staged = OpenedOwnerProject::admit_source(
        location,
        candidate_bytes,
        candidate_decoded,
        STORED_PROJECT_SCHEMA_VERSION,
    )?;
    let readout = staged.readout()?;
    ProjectStore::default()
        .save(location.project_file(), &stored, ProjectSaveMode::CreateNew)
        .map_err(project_store_rejection)?;
    Ok(readout)
}

pub fn create_scene(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
    name: String,
    make_entry: bool,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_current, candidate| {
            if candidate.scenes.iter().any(|scene| scene.id == scene_id) {
                return Err(reject(
                    "project.duplicateScene",
                    format!("scene `{scene_id}` already exists"),
                ));
            }
            candidate.scenes.push(StoredScene {
                id: scene_id.clone(),
                name,
                voxel_environment: None,
                voxel_instances: Vec::new(),
                voxel_object_instances: Vec::new(),
                entities: Vec::new(),
            });
            if make_entry {
                candidate.entry_scene = scene_id.clone();
            }
            Ok(ProjectMutationReceipt::SceneCreated {
                scene_id,
                made_entry: make_entry,
            })
        },
    )?)
}

pub fn rename_scene(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
    name: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_current, candidate| {
            let scene = candidate
                .scenes
                .iter_mut()
                .find(|scene| scene.id == scene_id)
                .ok_or_else(|| {
                    reject(
                        "project.missingScene",
                        format!("scene `{scene_id}` does not exist"),
                    )
                })?;
            scene.name = name;
            Ok(ProjectMutationReceipt::SceneRenamed { scene_id })
        },
    )?)
}

pub fn delete_scene(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_current, candidate| {
            if candidate.entry_scene == scene_id {
                return Err(reject(
                    "project.entrySceneDeletion",
                    "select another entry scene before deleting this scene",
                ));
            }
            let before = candidate.scenes.len();
            candidate.scenes.retain(|scene| scene.id != scene_id);
            if candidate.scenes.len() == before {
                return Err(reject(
                    "project.missingScene",
                    format!("scene `{scene_id}` does not exist"),
                ));
            }
            Ok(ProjectMutationReceipt::SceneDeleted { scene_id })
        },
    )?)
}

pub fn set_entry_scene(
    location: &ProjectLocation,
    expected_project_hash: &str,
    scene_id: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_current, candidate| {
            if !candidate.scenes.iter().any(|scene| scene.id == scene_id) {
                return Err(reject(
                    "project.missingScene",
                    format!("scene `{scene_id}` does not exist"),
                ));
            }
            candidate.entry_scene = scene_id.clone();
            Ok(ProjectMutationReceipt::EntrySceneSet { scene_id })
        },
    )?)
}

pub fn create_scene_object(
    location: &ProjectLocation,
    expected_project_hash: &str,
    expected_scene_revision: u64,
    object: StudioSceneObjectDraft,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |current, candidate| {
            let kind = appearance_kind(&object.appearance)?;
            apply_scene_edit(
                current,
                expected_scene_revision,
                SceneEditCommand::Create {
                    record: SceneNodeRecord {
                        id: SceneNodeId::new(object.entity_id),
                        parent: object.parent_entity_id.map(SceneNodeId::new),
                        child_order: object.child_order,
                        transform: scene_transform(object.transform),
                        renderable_transform: SceneTransform::IDENTITY,
                        kind,
                        metadata: NodeMetadata {
                            label: Some(object.name.clone()),
                            tags: Vec::new(),
                        },
                    },
                },
            )?;
            let (renderable, light) = stored_appearance(object.appearance);
            entry_scene_mut(candidate)
                .entities
                .push(StoredEntityDefinition {
                    id: object.entity_id,
                    name: object.name,
                    parent: object.parent_entity_id,
                    child_order: object.child_order,
                    translation: Some(object.transform.translation),
                    rotation: object.transform.rotation,
                    scale: object.transform.scale,
                    light,
                    bounds: None,
                    collision: object.collision,
                    renderable,
                    door: None,
                    switch: None,
                    floor_action: None,
                    lift: None,
                    enemy: false,
                    enemy_combat: None,
                    defeat_drop: None,
                    health: None,
                    hazard: None,
                    encounter: None,
                    extraction_beacon: None,
                    kinematic: object.kinematic,
                    navigation: None,
                    player_controller: None,
                    inventory: None,
                    pickup: None,
                    weapon: None,
                    secret_region: None,
                    level_exit: None,
                });
            Ok(ProjectMutationReceipt::SceneObjectCreated {
                entity_id: object.entity_id,
            })
        },
    )?)
}

pub fn delete_scene_object(
    location: &ProjectLocation,
    expected_project_hash: &str,
    expected_scene_revision: u64,
    entity_id: u64,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |current, candidate| {
            let scene = apply_scene_edit(
                current,
                expected_scene_revision,
                SceneEditCommand::Delete {
                    id: SceneNodeId::new(entity_id),
                },
            )?;
            let retained = scene
                .nodes
                .iter()
                .map(|node| node.id.raw())
                .collect::<BTreeSet<_>>();
            let stored_scene = entry_scene_mut(candidate);
            let removed = stored_scene.entities.len() - retained.len();
            stored_scene
                .entities
                .retain(|entity| retained.contains(&entity.id));
            stored_scene
                .voxel_object_instances
                .retain(|instance| retained.contains(&instance.owner_entity_id));
            Ok(ProjectMutationReceipt::SceneObjectDeleted {
                entity_id,
                removed_objects: removed,
            })
        },
    )?)
}

pub fn rename_scene_object(
    location: &ProjectLocation,
    expected_project_hash: &str,
    expected_scene_revision: u64,
    entity_id: u64,
    name: String,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |current, candidate| {
            apply_scene_edit(
                current,
                expected_scene_revision,
                SceneEditCommand::Rename {
                    id: SceneNodeId::new(entity_id),
                    label: Some(name.clone()),
                },
            )?;
            stored_entity_mut(candidate, entity_id)?.name = name;
            Ok(ProjectMutationReceipt::SceneObjectRenamed { entity_id })
        },
    )?)
}

pub fn reparent_scene_object(
    location: &ProjectLocation,
    expected_project_hash: &str,
    expected_scene_revision: u64,
    entity_id: u64,
    parent_entity_id: Option<u64>,
    child_order: u32,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |current, candidate| {
            apply_scene_edit(
                current,
                expected_scene_revision,
                SceneEditCommand::Reparent {
                    id: SceneNodeId::new(entity_id),
                    parent: parent_entity_id.map(SceneNodeId::new),
                    child_order,
                },
            )?;
            let entity = stored_entity_mut(candidate, entity_id)?;
            entity.parent = parent_entity_id;
            entity.child_order = child_order;
            Ok(ProjectMutationReceipt::SceneObjectReparented { entity_id })
        },
    )?)
}

pub fn set_scene_object_transform(
    location: &ProjectLocation,
    expected_project_hash: &str,
    expected_scene_revision: u64,
    entity_id: u64,
    transform: TransformReadout,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |current, candidate| {
            apply_scene_edit(
                current,
                expected_scene_revision,
                SceneEditCommand::SetTransform {
                    id: SceneNodeId::new(entity_id),
                    transform: scene_transform(transform),
                },
            )?;
            let scene = entry_scene_mut(candidate);
            let entity = scene
                .entities
                .iter_mut()
                .find(|entity| entity.id == entity_id)
                .ok_or_else(|| {
                    reject(
                        "project.missingEntity",
                        format!("entry scene has no entity {entity_id}"),
                    )
                })?;
            entity.translation = Some(transform.translation);
            entity.rotation = transform.rotation;
            entity.scale = transform.scale;
            if let Some(instance) = scene
                .voxel_object_instances
                .iter_mut()
                .find(|instance| instance.owner_entity_id == entity_id)
            {
                instance.translation = transform.translation;
                instance.rotation = transform.rotation;
                instance.scale = transform.scale;
            }
            Ok(ProjectMutationReceipt::SceneObjectTransformSet { entity_id })
        },
    )?)
}

pub fn set_scene_object_renderable_transform(
    location: &ProjectLocation,
    expected_project_hash: &str,
    expected_scene_revision: u64,
    entity_id: u64,
    transform: TransformReadout,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |current, candidate| {
            apply_scene_edit(
                current,
                expected_scene_revision,
                SceneEditCommand::SetRenderableTransform {
                    id: SceneNodeId::new(entity_id),
                    transform: scene_transform(transform),
                },
            )?;
            let renderable = stored_entity_mut(candidate, entity_id)?
                .renderable
                .as_mut()
                .ok_or_else(|| {
                    reject(
                        "scene.renderableTransformRequiresAsset",
                        format!("entity {entity_id} has no renderable asset"),
                    )
                })?;
            renderable.local_transform = Some(StoredRenderableTransform {
                translation: transform.translation,
                rotation: transform.rotation,
                scale: transform.scale,
            });
            Ok(ProjectMutationReceipt::SceneObjectRenderableTransformSet { entity_id })
        },
    )?)
}

pub fn set_scene_object_appearance(
    location: &ProjectLocation,
    expected_project_hash: &str,
    expected_scene_revision: u64,
    entity_id: u64,
    appearance: StudioSceneAppearance,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |current, candidate| {
            let kind = appearance_kind(&appearance)?;
            apply_scene_edit(
                current,
                expected_scene_revision,
                SceneEditCommand::SetKind {
                    id: SceneNodeId::new(entity_id),
                    kind,
                },
            )?;
            let (mut renderable, light) = stored_appearance(appearance);
            let entity = stored_entity_mut(candidate, entity_id)?;
            if let Some(renderable) = renderable.as_mut() {
                renderable.local_transform = entity
                    .renderable
                    .as_ref()
                    .and_then(|current| current.local_transform);
            }
            entity.renderable = renderable;
            entity.light = light;
            Ok(ProjectMutationReceipt::SceneObjectAppearanceSet { entity_id })
        },
    )?)
}

pub fn set_entity_collision(
    location: &ProjectLocation,
    expected_project_hash: &str,
    entity_id: u64,
    collision: Option<StoredCollision>,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_current, candidate| {
            stored_entity_mut(candidate, entity_id)?.collision = collision;
            Ok(ProjectMutationReceipt::EntityCollisionSet {
                entity_id,
                attached: collision.is_some(),
            })
        },
    )?)
}

pub fn set_entity_kinematic(
    location: &ProjectLocation,
    expected_project_hash: &str,
    entity_id: u64,
    kinematic: Option<StoredKinematic>,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    mutation_result(publish_project_mutation(
        location,
        expected_project_hash,
        move |_current, candidate| {
            stored_entity_mut(candidate, entity_id)?.kinematic = kinematic;
            Ok(ProjectMutationReceipt::EntityKinematicSet {
                entity_id,
                attached: kinematic.is_some(),
            })
        },
    )?)
}

fn mutation_result(
    published: PublishedProject<ProjectMutationReceipt>,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    Ok((published.value, published.readout))
}

fn apply_scene_edit(
    current: &OpenedOwnerProject,
    expected_scene_revision: u64,
    command: SceneEditCommand,
) -> Result<FlatSceneDocument, AdapterRejection> {
    let mut scene = current.scene.clone();
    SceneEditService
        .apply(&mut scene, expected_scene_revision, command)
        .map_err(|error| reject(error.code(), error.to_string()))?;
    Ok(scene)
}

fn stored_entity_mut(
    project: &mut StoredProject,
    entity_id: u64,
) -> Result<&mut StoredEntityDefinition, AdapterRejection> {
    entry_scene_mut(project)
        .entities
        .iter_mut()
        .find(|entity| entity.id == entity_id)
        .ok_or_else(|| {
            reject(
                "project.missingEntity",
                format!("entry scene has no entity {entity_id}"),
            )
        })
}

fn scene_transform(transform: TransformReadout) -> SceneTransform {
    SceneTransform {
        translation: Vec3::new(
            transform.translation[0],
            transform.translation[1],
            transform.translation[2],
        ),
        rotation: Quat::new(
            transform.rotation[0],
            transform.rotation[1],
            transform.rotation[2],
            transform.rotation[3],
        ),
        scale: Vec3::new(transform.scale[0], transform.scale[1], transform.scale[2]),
    }
}

fn appearance_kind(appearance: &StudioSceneAppearance) -> Result<SceneNodeKind, AdapterRejection> {
    match appearance {
        StudioSceneAppearance::Empty => Ok(SceneNodeKind::EmptyGroup),
        StudioSceneAppearance::StaticMesh { asset, .. } => {
            let id = AssetId::parse(asset)
                .map_err(|error| reject("scene.invalidAsset", error.to_string()))?;
            if id.kind() != AssetKind::StaticMesh {
                return Err(reject(
                    "scene.wrongAssetKind",
                    format!("appearance requires a static mesh, found {}", id.kind()),
                ));
            }
            Ok(SceneNodeKind::StaticMesh(AssetReference::new(
                id,
                AssetVersionReq::Exact(1),
                None,
            )))
        }
        StudioSceneAppearance::AnimatedMesh { asset, .. } => {
            let id = AssetId::parse(asset)
                .map_err(|error| reject("scene.invalidAsset", error.to_string()))?;
            if id.kind() != AssetKind::AnimatedMesh {
                return Err(reject(
                    "scene.wrongAssetKind",
                    format!("appearance requires an animated mesh, found {}", id.kind()),
                ));
            }
            Ok(SceneNodeKind::AnimatedMesh(AssetReference::new(
                id,
                AssetVersionReq::Exact(1),
                None,
            )))
        }
        StudioSceneAppearance::Light { light } => Ok(SceneNodeKind::Light(stored_light(*light))),
    }
}

fn stored_appearance(
    appearance: StudioSceneAppearance,
) -> (Option<StoredRenderable>, Option<StoredLight>) {
    match appearance {
        StudioSceneAppearance::Empty => (None, None),
        StudioSceneAppearance::StaticMesh { asset, visible } => (
            Some(StoredRenderable {
                asset,
                visible,
                local_transform: None,
                initial_clip: None,
                visual_binding: None,
            }),
            None,
        ),
        StudioSceneAppearance::AnimatedMesh {
            asset,
            visible,
            clip,
        } => (
            Some(StoredRenderable {
                asset,
                visible,
                local_transform: None,
                initial_clip: Some(clip),
                visual_binding: None,
            }),
            None,
        ),
        StudioSceneAppearance::Light { light } => (None, Some(light)),
    }
}

pub(crate) struct PublishedProject<T> {
    pub value: T,
    pub project_hash_before: ContentHash,
    pub project_hash_after: ContentHash,
    pub content_candidate_hash: ContentHash,
    pub scene_revision_after: u64,
    pub readout: StudioProjectReadout,
}

pub(crate) fn publish_project_mutation<T>(
    location: &ProjectLocation,
    expected_project_hash: &str,
    mutate: impl FnOnce(&OpenedOwnerProject, &mut StoredProject) -> Result<T, AdapterRejection>,
) -> Result<PublishedProject<T>, AdapterRejection> {
    publish_project_mutation_with_validation(location, expected_project_hash, mutate, |_, _| Ok(()))
}

pub(crate) fn publish_project_mutation_with_validation<T>(
    location: &ProjectLocation,
    expected_project_hash: &str,
    mutate: impl FnOnce(&OpenedOwnerProject, &mut StoredProject) -> Result<T, AdapterRejection>,
    validate_staged: impl FnOnce(&T, &StudioProjectReadout) -> Result<(), AdapterRejection>,
) -> Result<PublishedProject<T>, AdapterRejection> {
    let expected_hash = ContentHash::parse(expected_project_hash)
        .map_err(|error| reject("project.invalidHash", error.to_string()))?;
    let current = OpenedOwnerProject::load(location)?;
    if current.source_hash != expected_hash {
        return Err(reject(
            "project.staleHash",
            format!(
                "expected project hash {expected_hash}, found {}",
                current.source_hash
            ),
        ));
    }
    let mut document_candidate = current.stored.document().clone();
    let value = mutate(&current, &mut document_candidate)?;
    let (stored_candidate, _) =
        admit_stored_project_with_document(document_candidate).map_err(stored_project_rejection)?;
    let candidate_bytes =
        encode_project_document(stored_candidate.document()).map_err(stored_project_rejection)?;

    let next_manifest =
        project_manifest(location.relative_project_file(), candidate_bytes.as_bytes());
    let observed_identity =
        ContentStoreIdentity::from_manifest(current.scene.revision, &current.manifest)
            .map_err(|error| reject("content.identityRejected", error.to_string()))?;
    let write_candidate = ContentWriteCandidate::build_from_observed_prior(
        observed_identity.clone(),
        &current.manifest,
        ContentWriteSetDraft {
            next_manifest: next_manifest.clone(),
            writes: vec![ContentWrite::new(
                location.relative_project_file(),
                candidate_bytes.as_bytes(),
            )],
            moves: Vec::new(),
            deletes: Vec::new(),
        },
    )
    .map_err(|error| reject("content.writeRejected", error.to_string()))?;
    let content_candidate_hash = write_candidate.candidate_hash();
    let next_content_revision = write_candidate.expected_next().revision;
    let authorized = write_candidate
        .authorize(&observed_identity)
        .map_err(|error| reject("content.writeStale", error.to_string()))?;

    let candidate_decoded = DecodedProjectDocument {
        project: stored_candidate.into_document(),
        source_schema_version: STORED_PROJECT_SCHEMA_VERSION,
    };
    let staged = OpenedOwnerProject::admit_source(
        location,
        candidate_bytes,
        candidate_decoded,
        STORED_PROJECT_SCHEMA_VERSION,
    )?;
    let staged_readout = staged.readout()?;
    validate_staged(&value, &staged_readout)?;

    let observed_next =
        ContentStoreIdentity::from_manifest(next_content_revision, &staged.manifest)
            .map_err(|error| reject("content.confirmationRejected", error.to_string()))?;
    authorized
        .confirm(&observed_next)
        .map_err(|error| reject("content.publicationMismatch", error.to_string()))?;
    location
        .revalidate()
        .map_err(|error| reject("path.changedDuringWrite", error.to_string()))?;

    let installed_hash = ProjectStore::default()
        .replace_if_unchanged(location.project_file(), &staged.stored, expected_hash)
        .map_err(project_store_rejection)?;

    Ok(PublishedProject {
        value,
        project_hash_before: expected_hash,
        project_hash_after: installed_hash,
        content_candidate_hash,
        scene_revision_after: staged.scene_revision(),
        readout: staged_readout,
    })
}

struct ProjectedVoxelObjects {
    frame: RenderFrameDiff,
}

fn project_voxel_objects(
    project: &StoredProject,
    candidate: Option<(&VoxelObjectAsset, &VoxelObjectFrameSelection)>,
    projector: &mut VoxelObjectRenderProjector,
    frame_override: Option<(&str, u32)>,
) -> Result<ProjectedVoxelObjects, AdapterRejection> {
    let frame = project_stored_voxel_objects_with(project, candidate, projector, frame_override)
        .map_err(|error| reject("projection.voxelObjectRejected", error.to_string()))?;
    Ok(ProjectedVoxelObjects { frame })
}

fn voxel_object_authoring_readout(project: &StoredProject) -> VoxelObjectAuthoringReadout {
    let assets = project
        .assets
        .iter()
        .filter_map(|asset| asset.voxel_object.as_ref())
        .map(|object| VoxelObjectAssetAuthoringReadout {
            asset_id: object.asset_id.clone(),
            content_hash: object.content_hash.clone(),
            grid: VoxelObjectGridReadout {
                coordinate_system: "rightHandedYUp",
                cell_size: object.grid.cell_size,
                chunk_size: object.grid.chunk_size,
                pivot: object.grid.pivot,
            },
            bounds: object.bounds,
            default_frame: voxel_object_frame_readout(&object.default_frame, None),
            clips: object
                .clips
                .iter()
                .map(|clip| VoxelObjectClipAuthoringReadout {
                    clip_id: clip.id.clone(),
                    name: clip.name.clone(),
                    frames_per_second: clip.frames_per_second,
                    frames: clip
                        .frames
                        .iter()
                        .map(|frame| {
                            let duration = frame
                                .duration_seconds
                                .map(|seconds| (seconds * 1_000_000.0).round().max(1.0) as u64);
                            voxel_object_frame_readout(&frame.frame, duration)
                        })
                        .collect(),
                })
                .collect(),
            default_clip: object.default_clip.clone(),
            material_palette: object.material_palette.clone(),
            material_map: object.material_map.clone(),
            provenance: VoxelObjectProvenanceReadout {
                kind: object.provenance.kind,
                source_path: object.provenance.source_path.clone(),
                source_sha256: object.provenance.source_sha256.clone(),
                source_byte_count: object.provenance.source_byte_count,
                converter: object.provenance.converter.clone(),
                settings_sha256: object.provenance.settings_sha256.clone(),
                license_path: object.provenance.license_path.clone(),
                source_clips: object.provenance.source_clips.clone(),
            },
        })
        .collect();
    let mut instances = project
        .scenes
        .iter()
        .flat_map(|scene| {
            scene
                .voxel_object_instances
                .iter()
                .cloned()
                .map(|instance| VoxelObjectInstanceReadout {
                    scene_id: scene.id.clone(),
                    owner_entity_id: instance.owner_entity_id,
                    instance: StudioVoxelObjectInstance {
                        instance_id: instance.instance_id,
                        voxel_object_asset_id: instance.voxel_object_asset_id,
                        surface_mode: instance.surface_mode,
                        frame: instance.frame,
                        translation: instance.translation,
                        rotation: instance.rotation,
                        scale: instance.scale,
                        material_overrides: instance.material_overrides,
                    },
                })
        })
        .collect::<Vec<_>>();
    instances.sort_by(|left, right| {
        left.owner_entity_id
            .cmp(&right.owner_entity_id)
            .then_with(|| left.scene_id.cmp(&right.scene_id))
            .then_with(|| left.instance.instance_id.cmp(&right.instance.instance_id))
    });
    VoxelObjectAuthoringReadout { assets, instances }
}

fn voxel_object_frame_readout(
    frame: &VoxelFrame,
    duration_microseconds: Option<u64>,
) -> VoxelObjectFrameAuthoringReadout {
    VoxelObjectFrameAuthoringReadout {
        bounds: frame.bounds,
        voxel_data_hash: frame.voxel_data_hash.clone(),
        voxel_count: frame
            .representation
            .sparse_runs
            .iter()
            .map(|run| run.length as usize)
            .sum(),
        sparse_run_count: frame.representation.sparse_runs.len(),
        duration_microseconds,
    }
}

fn complete_projection(
    project: &StoredProject,
    catalog: &AssetCatalog,
    instances: RenderFrameDiff,
    lights: RenderFrameDiff,
    voxels: RenderFrameDiff,
    voxel_objects: RenderFrameDiff,
) -> Result<RenderFrameDiff, AdapterRejection> {
    let mut operations = Vec::new();
    for material in project_material_descriptors(catalog).into_values() {
        operations.push(RenderDiff::DefineMaterial { material });
    }
    for entry in catalog
        .iter()
        .filter(|entry| entry.kind() == AssetKind::StaticMesh)
    {
        let asset = entry.id.as_str();
        let imported = project
            .assets
            .iter()
            .find(|candidate| candidate.id == asset)
            .and_then(|candidate| candidate.static_mesh.clone());
        let mesh = imported.unwrap_or_else(|| {
            let material = format!(
                "material/studio-{}",
                asset.trim_start_matches("mesh/").replace('/', "-")
            );
            operations.push(RenderDiff::DefineMaterial {
                material: studio_material(&material, asset),
            });
            StaticMeshAsset {
                asset: asset.to_string(),
                payload: cuboid_payload(studio_mesh_dimensions(asset)),
                material_slots: vec![MeshMaterialSlot { slot: 0, material }],
                collision: MeshCollisionPolicy::VisualOnly,
            }
        });
        operations.push(RenderDiff::DefineStaticMesh { asset: mesh });
    }
    for asset in project
        .assets
        .iter()
        .filter_map(|stored| stored.animated_mesh.clone())
    {
        operations.push(RenderDiff::DefineAnimatedMesh { asset });
    }
    operations.extend(voxels.ops);
    operations.extend(
        voxel_objects
            .ops
            .into_iter()
            .filter(|operation| !matches!(operation, RenderDiff::DefineMaterial { .. })),
    );
    operations.extend(lights.ops);
    operations.extend(instances.ops);
    RenderFrameDiff::try_from_ops(operations)
        .map_err(|error| reject("projection.completeFrameRejected", format!("{error:?}")))
}

fn project_material_descriptors(
    catalog: &AssetCatalog,
) -> BTreeMap<String, RenderMaterialDescriptor> {
    catalog
        .iter()
        .filter(|entry| entry.kind() == AssetKind::Material)
        .filter_map(|entry| {
            let material = entry.material.as_ref()?.render_projection();
            let id = entry.id.as_str().to_string();
            Some((
                id.clone(),
                RenderMaterialDescriptor {
                    schema_version: 1,
                    id,
                    color: [
                        material.color.r,
                        material.color.g,
                        material.color.b,
                        material.color.a,
                    ],
                    texture: material
                        .texture
                        .as_ref()
                        .map(|reference| reference.id().as_str().to_string()),
                    roughness: material.roughness,
                    texture_tint: [
                        material.texture_tint.r,
                        material.texture_tint.g,
                        material.texture_tint.b,
                        material.texture_tint.a,
                    ],
                    emission_color: [
                        material.emission_color.r,
                        material.emission_color.g,
                        material.emission_color.b,
                    ],
                    emission_intensity: material.emissive,
                    uv_strategy: match material.uv_strategy {
                        UvStrategy::Flat => MaterialUvStrategy::Flat,
                        UvStrategy::Planar => MaterialUvStrategy::Planar,
                        UvStrategy::Atlas => MaterialUvStrategy::Atlas,
                    },
                    voxel_surface: None,
                },
            ))
        })
        .collect()
}

fn project_scene_lights(scene: &FlatSceneDocument) -> Result<RenderFrameDiff, AdapterRejection> {
    let world = composed_world_transforms(scene);
    let lights = scene
        .nodes
        .iter()
        .filter_map(|node| {
            let SceneNodeKind::Light(light) = &node.kind else {
                return None;
            };
            Some(AppearanceLight {
                id: node.id.raw(),
                parent: None,
                availability: ProjectionAvailability::Both,
                light: render_light(light, world[&node.id]),
            })
        })
        .collect();
    SceneAppearanceProjector::new()
        .project(
            &AppearanceScene {
                lights,
                ..AppearanceScene::default()
            },
            ProjectionMode::AuthoredPreview,
        )
        .map(|projected| projected.frame)
        .map_err(|error| reject("projection.lightRejected", format!("{error:?}")))
}

fn render_light(light: &SceneLight, transform: SceneTransform) -> LightDescriptor {
    let shadow = |intent| match intent {
        SceneLightShadowIntent::Disabled => LightShadowIntent::Disabled,
        SceneLightShadowIntent::Requested => LightShadowIntent::Requested,
    };
    let position = [
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
    ];
    let direction = rotate_direction(transform.rotation, [0.0, -1.0, 0.0]);
    match light {
        SceneLight::Ambient {
            color,
            intensity,
            enabled,
            shadow_intent,
        } => LightDescriptor::Ambient {
            color: *color,
            intensity: *intensity,
            enabled: *enabled,
            shadow_intent: shadow(*shadow_intent),
        },
        SceneLight::Directional {
            color,
            intensity,
            enabled,
            shadow_intent,
        } => LightDescriptor::Directional {
            color: *color,
            intensity: *intensity,
            enabled: *enabled,
            direction,
            shadow_intent: shadow(*shadow_intent),
        },
        SceneLight::Point {
            color,
            intensity,
            enabled,
            range,
            decay,
            shadow_intent,
        } => LightDescriptor::Point {
            color: *color,
            intensity: *intensity,
            enabled: *enabled,
            position,
            range: *range,
            decay: *decay,
            shadow_intent: shadow(*shadow_intent),
        },
        SceneLight::Spot {
            color,
            intensity,
            enabled,
            range,
            decay,
            outer_angle_radians,
            penumbra,
            shadow_intent,
        } => LightDescriptor::Spot {
            color: *color,
            intensity: *intensity,
            enabled: *enabled,
            position,
            direction,
            range: *range,
            decay: *decay,
            outer_angle_radians: *outer_angle_radians,
            penumbra: *penumbra,
            shadow_intent: shadow(*shadow_intent),
        },
    }
}

fn rotate_direction(rotation: Quat, vector: [f32; 3]) -> [f32; 3] {
    let axis = [rotation.x, rotation.y, rotation.z];
    let cross = |left: [f32; 3], right: [f32; 3]| {
        [
            left[1] * right[2] - left[2] * right[1],
            left[2] * right[0] - left[0] * right[2],
            left[0] * right[1] - left[1] * right[0],
        ]
    };
    let first = cross(axis, vector);
    let twice = [first[0] * 2.0, first[1] * 2.0, first[2] * 2.0];
    let second = cross(axis, twice);
    [
        vector[0] + twice[0] * rotation.w + second[0],
        vector[1] + twice[1] * rotation.w + second[1],
        vector[2] + twice[2] * rotation.w + second[2],
    ]
}

fn studio_material(id: &str, asset: &str) -> RenderMaterialDescriptor {
    let seed = asset.bytes().fold(2_166_136_261_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
    });
    let channel = |shift: u32| 0.28 + (((seed >> shift) & 0xff) as f32 / 255.0) * 0.58;
    RenderMaterialDescriptor {
        schema_version: 1,
        id: id.to_string(),
        color: [channel(0), channel(8), channel(16), 1.0],
        texture: None,
        roughness: 0.78,
        texture_tint: [1.0; 4],
        emission_color: [0.0; 3],
        emission_intensity: 0.0,
        uv_strategy: MaterialUvStrategy::Flat,
        voxel_surface: None,
    }
}

fn studio_mesh_dimensions(asset: &str) -> [f32; 3] {
    if asset.contains("security-door") {
        [2.4, 3.4, 0.55]
    } else if asset.contains("extraction-beacon") {
        [0.8, 2.4, 0.8]
    } else if asset.contains("spatial-probe") {
        [0.5, 0.5, 0.5]
    } else if asset.contains("player-marker") {
        [0.7, 1.4, 0.7]
    } else if asset.contains("control-panel") {
        [1.2, 1.5, 0.8]
    } else {
        [1.1, 1.8, 1.1]
    }
}

fn cuboid_payload([width, height, depth]: [f32; 3]) -> MeshPayloadDescriptor {
    let half_width = width / 2.0;
    let half_depth = depth / 2.0;
    let positions = vec![
        -half_width,
        0.0,
        -half_depth,
        half_width,
        0.0,
        -half_depth,
        half_width,
        height,
        -half_depth,
        -half_width,
        height,
        -half_depth,
        -half_width,
        0.0,
        half_depth,
        half_width,
        0.0,
        half_depth,
        half_width,
        height,
        half_depth,
        -half_width,
        height,
        half_depth,
    ];
    MeshPayloadDescriptor {
        layout: MeshBufferLayout {
            vertex_count: 8,
            index_count: 36,
            index_width: MeshIndexWidth::U32,
            attributes: vec![
                MeshAttribute {
                    name: MeshAttributeName::Position,
                    components: 3,
                    kind: MeshAttributeKind::F32,
                },
                MeshAttribute {
                    name: MeshAttributeName::Normal,
                    components: 3,
                    kind: MeshAttributeKind::F32,
                },
            ],
        },
        groups: vec![MeshGroupDescriptor {
            material_slot: 0,
            start: 0,
            count: 36,
        }],
        bounds: MeshBoundsDescriptor {
            min: [-half_width, 0.0, -half_depth],
            max: [half_width, height, half_depth],
        },
        source: MeshPayloadSource::Inline {
            positions,
            normals: vec![
                -0.577, -0.577, -0.577, 0.577, -0.577, -0.577, 0.577, 0.577, -0.577, -0.577, 0.577,
                -0.577, -0.577, -0.577, 0.577, 0.577, -0.577, 0.577, 0.577, 0.577, 0.577, -0.577,
                0.577, 0.577,
            ],
            uvs: None,
            indices: vec![
                4, 5, 6, 4, 6, 7, 1, 0, 3, 1, 3, 2, 0, 4, 7, 0, 7, 3, 5, 1, 2, 5, 2, 6, 3, 7, 6, 3,
                6, 2, 0, 1, 5, 0, 5, 4,
            ],
        },
        provenance: MeshProvenance::Generated,
    }
}

fn scene_hierarchy(scene: &FlatSceneDocument, state: &EntityState) -> SceneHierarchyReadout {
    let tree = scene
        .to_tree()
        .expect("validated authored scene has a tree representation");
    let world = composed_world_transforms(scene);
    let child_orders = scene
        .nodes
        .iter()
        .map(|node| (node.id, node.child_order))
        .collect::<BTreeMap<_, _>>();
    let entities = state
        .entities()
        .map(|entity| (SceneNodeId::new(entity.id.raw()), entity.id))
        .collect::<BTreeMap<_, _>>();
    let mut nodes = Vec::with_capacity(state.total_count().max(scene.nodes.len()));
    for root in &tree.roots {
        append_hierarchy_node(root, None, 0, &world, &child_orders, &entities, &mut nodes);
    }
    let authored_node_ids = scene
        .nodes
        .iter()
        .map(|node| node.id.raw())
        .collect::<BTreeSet<_>>();
    let mut root_node_ids = tree
        .roots
        .iter()
        .map(|node| node.id.raw())
        .collect::<Vec<_>>();
    let mut used_root_orders = scene
        .nodes
        .iter()
        .filter(|node| node.parent.is_none())
        .map(|node| node.child_order)
        .collect::<BTreeSet<_>>();
    let mut next_child_order = 0_u32;
    for entity in state
        .entities()
        .filter(|entity| !authored_node_ids.contains(&entity.id.raw()))
    {
        while used_root_orders.contains(&next_child_order) {
            next_child_order = next_child_order
                .checked_add(1)
                .expect("bounded hierarchy cannot exhaust every u32 child order");
        }
        let transform = state
            .transform(entity.id)
            .map_or(EntityTransform::IDENTITY, |transform| transform.transform());
        let world_transform = state.world_transform(entity.id).unwrap_or(transform);
        let renderable_transform = state
            .renderable(entity.id)
            .map_or(EntityTransform::IDENTITY, |renderable| {
                renderable.local_transform
            });
        let display_order = nodes.len() as u32;
        root_node_ids.push(entity.id.raw());
        nodes.push(SceneHierarchyNodeReadout {
            node_id: entity.id.raw(),
            parent_node_id: None,
            child_order: next_child_order,
            display_order,
            depth: 0,
            node_kind: "entityInstance",
            label: entity.name.clone(),
            tags: vec!["runtime-derived".to_string()],
            asset: state
                .renderable(entity.id)
                .map(|renderable| renderable.asset.clone()),
            entity_id: Some(entity.id.raw()),
            local_transform: transform_readout(transform),
            world_transform: transform_readout(world_transform),
            renderable_transform: transform_readout(renderable_transform),
        });
        used_root_orders.insert(next_child_order);
    }
    SceneHierarchyReadout {
        scene_id: scene.id.raw(),
        revision: scene.revision,
        name: scene.metadata.name.clone(),
        root_node_ids,
        nodes,
    }
}

fn append_hierarchy_node(
    node: &SceneNode,
    parent: Option<u64>,
    depth: u32,
    world: &BTreeMap<SceneNodeId, EntityTransform>,
    child_orders: &BTreeMap<SceneNodeId, u32>,
    entities: &BTreeMap<SceneNodeId, rusty_engine::core_ids::EntityId>,
    nodes: &mut Vec<SceneHierarchyNodeReadout>,
) {
    let display_order = nodes.len() as u32;
    nodes.push(SceneHierarchyNodeReadout {
        node_id: node.id.raw(),
        parent_node_id: parent,
        child_order: child_orders[&node.id],
        display_order,
        depth,
        node_kind: node.kind.tag(),
        label: node
            .metadata
            .label
            .clone()
            .unwrap_or_else(|| format!("Node {}", node.id.raw())),
        tags: node.metadata.tags.clone(),
        asset: node
            .kind
            .asset()
            .map(|asset| asset.id().as_str().to_string()),
        entity_id: entities.get(&node.id).map(|entity| entity.raw()),
        local_transform: transform_readout(node.transform),
        world_transform: transform_readout(world[&node.id]),
        renderable_transform: transform_readout(node.renderable_transform),
    });
    for child in &node.children {
        append_hierarchy_node(
            child,
            Some(node.id.raw()),
            depth + 1,
            world,
            child_orders,
            entities,
            nodes,
        );
    }
}

fn transform_readout(transform: EntityTransform) -> TransformReadout {
    TransformReadout {
        translation: [
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
        ],
        rotation: [
            transform.rotation.x,
            transform.rotation.y,
            transform.rotation.z,
            transform.rotation.w,
        ],
        scale: [transform.scale.x, transform.scale.y, transform.scale.z],
    }
}

fn project_manifest(relative_project_file: &str, bytes: &[u8]) -> ContentManifest {
    ContentManifest::new(vec![ContentArtifact::durable(
        relative_project_file,
        ArtifactRole::Resource(PROJECT_RESOURCE_ROLE.to_string()),
        bytes,
    )])
}

fn project_catalog(project: &StoredProject) -> Result<AssetCatalog, AdapterRejection> {
    let entries = project
        .assets
        .iter()
        .map(|asset| {
            AssetId::parse(&asset.id)
                .map_err(|error| reject("catalog.invalidAsset", error.to_string()))?;
            let metadata = asset.catalog.as_ref();
            let dependencies = metadata.map_or_else(
                || {
                    asset
                        .voxel_volume
                        .iter()
                        .flat_map(|voxel| &voxel.material_palette)
                        .chain(
                            asset
                                .voxel_object
                                .iter()
                                .flat_map(|object| &object.material_palette),
                        )
                        .map(|binding| StoredAssetReference {
                            id: binding.material_asset_id.clone(),
                            version: StoredAssetVersionRequirement::Exact { value: 1 },
                            hash: None,
                        })
                        .collect()
                },
                |metadata| metadata.dependencies.clone(),
            );
            let mut unique_dependencies = Vec::with_capacity(dependencies.len());
            for dependency in dependencies {
                if !unique_dependencies.contains(&dependency) {
                    unique_dependencies.push(dependency);
                }
            }
            Ok(StoredCatalogEntry {
                id: asset.id.clone(),
                version: metadata.map_or(1, |metadata| metadata.version),
                hash: metadata.and_then(|metadata| metadata.hash.clone()),
                source_path: metadata.and_then(|metadata| metadata.source_path.clone()),
                label: metadata
                    .and_then(|metadata| metadata.label.clone())
                    .or_else(|| Some(asset.id.clone())),
                dependencies: unique_dependencies,
                material: asset.material.clone(),
                texture: None,
                voxel_atlas: None,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let stored = StoredAssetCatalog { entries };
    let encoded = serde_json::to_string(&stored)
        .map_err(|error| reject("catalog.encode", error.to_string()))?;
    let catalog = decode_catalog(&encoded)
        .map_err(|error| reject("catalog.invalidMaterial", error.to_string()))?
        .canonical();
    let validation = validate_catalog(&catalog);
    if !validation.is_ok() {
        return Err(reject(
            "catalog.rejected",
            validation
                .diagnostics()
                .into_iter()
                .map(|diagnostic| {
                    format!(
                        "{} at {}: {}",
                        diagnostic.code, diagnostic.path, diagnostic.message
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    Ok(catalog)
}

fn project_scene(
    project: &StoredProject,
    source_hash: ContentHash,
) -> Result<FlatSceneDocument, AdapterRejection> {
    let scene_index = project
        .scenes
        .iter()
        .position(|scene| scene.id == project.entry_scene)
        .expect("admitted project retains entry scene");
    let source = &project.scenes[scene_index];
    let mut dependencies = BTreeMap::<String, AssetReference>::new();
    let mut nodes = Vec::with_capacity(source.entities.len());
    for entity in &source.entities {
        let kind = match (&entity.light, &entity.renderable) {
            (Some(light), None) => SceneNodeKind::Light(stored_light(*light)),
            (None, Some(renderable)) => {
                let id = AssetId::parse(&renderable.asset)
                    .map_err(|error| reject("scene.invalidAsset", error.to_string()))?;
                let kind = id.kind();
                let reference = AssetReference::new(id, AssetVersionReq::Exact(1), None);
                dependencies.insert(renderable.asset.clone(), reference.clone());
                match kind {
                    AssetKind::StaticMesh => SceneNodeKind::StaticMesh(reference),
                    AssetKind::AnimatedMesh => SceneNodeKind::AnimatedMesh(reference),
                    _ => {
                        return Err(reject(
                            "scene.wrongAssetKind",
                            format!("renderable requires a mesh, found {kind}"),
                        ))
                    }
                }
            }
            (None, None) => SceneNodeKind::EmptyGroup,
            (Some(_), Some(_)) => unreachable!("stored-project validation rejects mixed kinds"),
        };
        nodes.push(SceneNodeRecord {
            id: SceneNodeId::new(entity.id),
            parent: entity.parent.map(SceneNodeId::new),
            child_order: entity.child_order,
            transform: SceneTransform {
                translation: entity
                    .translation
                    .map_or(Vec3::ZERO, |value| Vec3::new(value[0], value[1], value[2])),
                rotation: Quat::new(
                    entity.rotation[0],
                    entity.rotation[1],
                    entity.rotation[2],
                    entity.rotation[3],
                ),
                scale: Vec3::new(entity.scale[0], entity.scale[1], entity.scale[2]),
            },
            renderable_transform: entity
                .renderable
                .as_ref()
                .and_then(|renderable| renderable.local_transform)
                .map_or(SceneTransform::IDENTITY, |transform| SceneTransform {
                    translation: Vec3::new(
                        transform.translation[0],
                        transform.translation[1],
                        transform.translation[2],
                    ),
                    rotation: Quat::new(
                        transform.rotation[0],
                        transform.rotation[1],
                        transform.rotation[2],
                        transform.rotation[3],
                    ),
                    scale: Vec3::new(transform.scale[0], transform.scale[1], transform.scale[2]),
                }),
            kind,
            metadata: NodeMetadata {
                label: Some(entity.name.clone()),
                tags: domain_tags(entity),
            },
        });
    }

    let mut scene = FlatSceneDocument {
        id: SceneId::new((scene_index + 1) as u64),
        revision: revision_from_hash(source_hash),
        schema_version: CURRENT_SCENE_SCHEMA_VERSION,
        metadata: SceneMetadata {
            name: Some(source.name.clone()),
            authoring_format_version: CURRENT_SCENE_SCHEMA_VERSION,
        },
        dependencies: dependencies.into_values().collect(),
        nodes,
    };
    scene.canonicalize();
    Ok(scene)
}

fn stored_light(light: StoredLight) -> SceneLight {
    let shadow_intent = |enabled| {
        if enabled {
            SceneLightShadowIntent::Requested
        } else {
            SceneLightShadowIntent::Disabled
        }
    };
    match light {
        StoredLight::Ambient {
            color,
            intensity,
            enabled,
            shadows,
        } => SceneLight::Ambient {
            color,
            intensity,
            enabled,
            shadow_intent: shadow_intent(shadows),
        },
        StoredLight::Directional {
            color,
            intensity,
            enabled,
            shadows,
        } => SceneLight::Directional {
            color,
            intensity,
            enabled,
            shadow_intent: shadow_intent(shadows),
        },
        StoredLight::Point {
            color,
            intensity,
            enabled,
            range,
            decay,
            shadows,
        } => SceneLight::Point {
            color,
            intensity,
            enabled,
            range,
            decay,
            shadow_intent: shadow_intent(shadows),
        },
        StoredLight::Spot {
            color,
            intensity,
            enabled,
            range,
            decay,
            outer_angle_radians,
            penumbra,
            shadows,
        } => SceneLight::Spot {
            color,
            intensity,
            enabled,
            range,
            decay,
            outer_angle_radians,
            penumbra,
            shadow_intent: shadow_intent(shadows),
        },
    }
}

fn validate_owner_admission(
    catalog: &AssetCatalog,
    scene: &FlatSceneDocument,
) -> Result<(), AdapterRejection> {
    let validation = validate_scene(scene);
    if !validation.is_valid() {
        return Err(reject(
            "scene.rejected",
            validation
                .diagnostics()
                .into_iter()
                .map(|diagnostic| {
                    format!(
                        "{} at {}: {}",
                        diagnostic.code, diagnostic.path, diagnostic.message
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let mut resolution = SceneResolutionContext::default();
    for entry in catalog.iter() {
        resolution.available_assets.insert(
            entry.id.clone(),
            AvailableSceneAsset {
                version: entry.version,
                hash: entry.hash.clone(),
            },
        );
    }
    let plan = SceneAdmissionPlan::prepare(scene, &resolution)
        .map_err(|error| reject("scene.admissionRejected", error.to_string()))?;
    let mut state = EntityState::default();
    let state_revision = state.revision();
    plan.apply(&mut state, state_revision)
        .map_err(|error| reject("entityState.admissionRejected", error.to_string()))?;
    Ok(())
}

fn resolved_render_assets(catalog: &AssetCatalog) -> BTreeMap<String, ResolvedRenderAsset> {
    catalog
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind(),
                AssetKind::StaticMesh | AssetKind::AnimatedMesh
            )
        })
        .map(|entry| {
            (
                entry.id.as_str().to_string(),
                ResolvedRenderAsset {
                    id: entry.id.as_str().to_string(),
                    kind: match entry.kind() {
                        AssetKind::StaticMesh => RenderAssetKind::StaticMesh,
                        AssetKind::AnimatedMesh => RenderAssetKind::AnimatedMesh,
                        _ => unreachable!("render asset filter is closed"),
                    },
                    content_hash: entry.hash.as_ref().map(|hash| hash.as_str().to_string()),
                    version: entry.version,
                },
            )
        })
        .collect()
}

fn install_animation_playback(
    project: &StoredProject,
    mut frame: RenderFrameDiff,
) -> Result<RenderFrameDiff, AdapterRejection> {
    let scene = entry_scene(project);
    for operation in &mut frame.ops {
        let RenderDiff::CreateAnimatedMeshInstance { instance, .. } = operation else {
            continue;
        };
        let entity_id = instance.metadata.source_entity.ok_or_else(|| {
            reject(
                "projection.animationMissingEntity",
                "animated instance has no source entity",
            )
        })?;
        let entity = scene
            .entities
            .iter()
            .find(|entity| entity.id == entity_id)
            .ok_or_else(|| {
                reject(
                    "projection.animationMissingEntity",
                    format!("missing source entity {entity_id}"),
                )
            })?;
        let renderable = entity.renderable.as_ref().ok_or_else(|| {
            reject(
                "projection.animationMissingRenderable",
                format!("entity {entity_id} has no renderable"),
            )
        })?;
        let asset = project
            .assets
            .iter()
            .find(|asset| asset.id == renderable.asset)
            .and_then(|asset| asset.animated_mesh.as_ref())
            .ok_or_else(|| reject("projection.animationMissingAsset", &renderable.asset))?;
        let clip = renderable
            .initial_clip
            .as_ref()
            .or(asset.default_clip.as_ref())
            .ok_or_else(|| reject("projection.animationMissingClip", &renderable.asset))?;
        instance.playback = Some(AnimatedMeshPlaybackCommand::Play {
            clip: clip.clone(),
            r#loop: AnimationLoopMode::Repeat,
            speed: 1.0,
            weight: 1.0,
            restart: true,
            fade_seconds: None,
        });
    }
    Ok(frame)
}

fn animated_mesh_resources(
    project: &StoredProject,
) -> Result<Vec<AnimatedMeshResourceReadout>, AdapterRejection> {
    project
        .assets
        .iter()
        .filter_map(|stored| stored.animated_mesh.as_ref().map(|asset| (stored, asset)))
        .map(|(stored, asset)| {
            Ok(AnimatedMeshResourceReadout {
                asset: asset.asset.clone(),
                content_hash: asset
                    .content_hash
                    .clone()
                    .ok_or_else(|| reject("projection.animationMissingHash", &asset.asset))?,
                clip_ids: asset.clips.iter().map(|clip| clip.id.clone()).collect(),
                source_path: stored
                    .catalog
                    .as_ref()
                    .and_then(|metadata| metadata.source_path.clone())
                    .ok_or_else(|| reject("projection.animationMissingSource", &asset.asset))?,
            })
        })
        .collect()
}

fn entity_component_references(
    project: &StoredProject,
    hierarchy: &SceneHierarchyReadout,
) -> Result<Vec<StudioEntityComponentReference>, AdapterRejection> {
    let voxel_contract = StudioEntityInspectorContractIdentity {
        contract_id: VOXEL_OBJECT_INSPECTOR_CONTRACT_ID,
        contract_version: VOXEL_OBJECT_INSPECTOR_CONTRACT_VERSION,
    };
    let weapon_contract = StudioEntityInspectorContractIdentity {
        contract_id: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
        contract_version: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_VERSION,
    };
    let mut references = project
        .scenes
        .iter()
        .flat_map(|scene| scene.voxel_object_instances.iter())
        .map(|instance| StudioEntityComponentReference {
            owner_entity_id: instance.owner_entity_id,
            component_type_id: VOXEL_OBJECT_COMPONENT_TYPE_ID,
            inspector_contract: Some(voxel_contract),
        })
        .collect::<Vec<_>>();
    references.extend(
        loading_bay_weapon_owner_entity_ids(project)
            .map_err(|message| reject("project.entityComponents", message))?
            .into_iter()
            .map(|owner_entity_id| StudioEntityComponentReference {
                owner_entity_id,
                component_type_id: LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
                inspector_contract: Some(weapon_contract),
            }),
    );
    references.sort_by_key(|reference| (reference.owner_entity_id, reference.component_type_id));

    if references.len() > MAX_STUDIO_ENTITY_COMPONENT_REFERENCES {
        return Err(reject(
            "project.entityComponents",
            format!(
                "project has {} component references, exceeding {}",
                references.len(),
                MAX_STUDIO_ENTITY_COMPONENT_REFERENCES
            ),
        ));
    }
    let known_owners = hierarchy
        .nodes
        .iter()
        .filter_map(|node| node.entity_id)
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut per_owner = BTreeMap::<u64, usize>::new();
    for reference in &references {
        if !known_owners.contains(&reference.owner_entity_id) {
            return Err(reject(
                "project.entityComponents",
                format!(
                    "component owner {} is absent from the canonical entry-scene hierarchy",
                    reference.owner_entity_id
                ),
            ));
        }
        if !seen.insert((reference.owner_entity_id, reference.component_type_id)) {
            return Err(reject(
                "project.entityComponents",
                format!(
                    "owner {} repeats component `{}`",
                    reference.owner_entity_id, reference.component_type_id
                ),
            ));
        }
        let count = per_owner.entry(reference.owner_entity_id).or_default();
        *count += 1;
        if *count > MAX_STUDIO_ENTITY_COMPONENTS_PER_OWNER {
            return Err(reject(
                "project.entityComponents",
                format!(
                    "owner {} exceeds {} component references",
                    reference.owner_entity_id, MAX_STUDIO_ENTITY_COMPONENTS_PER_OWNER
                ),
            ));
        }
    }
    Ok(references)
}

fn domain_tags(entity: &StoredEntityDefinition) -> Vec<String> {
    let mut tags = BTreeSet::new();
    if entity.door.is_some() {
        tags.insert("door".to_string());
    }
    if entity.switch.is_some() {
        tags.insert("switch".to_string());
    }
    if entity.enemy {
        tags.insert("enemy".to_string());
    }
    if entity.encounter.is_some() {
        tags.insert("encounter".to_string());
    }
    if entity.extraction_beacon.is_some() {
        tags.insert("extraction-beacon".to_string());
    }
    if entity.navigation.is_some() {
        tags.insert("navigation".to_string());
    }
    if entity.player_controller.is_some() {
        tags.insert("player-controller".to_string());
    }
    if entity.weapon.is_some() {
        tags.insert("weapon".to_string());
    }
    tags.into_iter().collect()
}

fn entry_scene(project: &StoredProject) -> &StoredScene {
    project
        .scenes
        .iter()
        .find(|scene| scene.id == project.entry_scene)
        .expect("admitted project retains entry scene")
}

fn entry_scene_mut(project: &mut StoredProject) -> &mut StoredScene {
    project
        .scenes
        .iter_mut()
        .find(|scene| scene.id == project.entry_scene)
        .expect("admitted project retains entry scene")
}

fn revision_from_hash(hash: ContentHash) -> u64 {
    let bytes = hash.as_bytes();
    let value = u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]) & JSON_SAFE_U64_MASK;
    value.max(1)
}

fn projection_diagnostic(diagnostic: EntityProjectionDiagnostic) -> ProjectionDiagnosticReadout {
    match diagnostic {
        EntityProjectionDiagnostic::MissingAsset { entity, asset } => ProjectionDiagnosticReadout {
            code: "projection.missingAsset",
            entity_id: entity.raw(),
            asset,
            asset_kind: None,
        },
        EntityProjectionDiagnostic::UnsupportedAppearance {
            entity,
            asset,
            kind,
        } => ProjectionDiagnosticReadout {
            code: "projection.unsupportedAppearance",
            entity_id: entity.raw(),
            asset,
            asset_kind: Some(render_asset_kind(kind).to_string()),
        },
    }
}

const fn render_asset_kind(kind: RenderAssetKind) -> &'static str {
    match kind {
        RenderAssetKind::Material => "material",
        RenderAssetKind::Texture => "texture",
        RenderAssetKind::Sprite => "sprite",
        RenderAssetKind::SpriteAtlas => "spriteAtlas",
        RenderAssetKind::StaticMesh => "staticMesh",
        RenderAssetKind::AnimatedMesh => "animatedMesh",
        RenderAssetKind::VoxelObject => "voxelObject",
        RenderAssetKind::Audio => "audio",
        RenderAssetKind::Font => "font",
    }
}

fn project_store_rejection(error: crate::ProjectStoreError) -> AdapterRejection {
    if let crate::ProjectStoreError::Codec(codec) = &error {
        return stored_project_rejection(codec.clone());
    }
    let code = match error {
        crate::ProjectStoreError::StaleSource { .. } => "project.staleHash",
        crate::ProjectStoreError::TooLarge { .. } => "project.tooLarge",
        crate::ProjectStoreError::InvalidUtf8 { .. } => "project.invalidUtf8",
        _ => "project.storageRejected",
    };
    reject(code, error.to_string())
}

fn stored_project_rejection(error: crate::StoredProjectError) -> AdapterRejection {
    AdapterRejection::new(error.diagnostic().code, error.diagnostic().message.clone())
        .at_path(error.diagnostic().path.clone())
}

fn reject(code: impl Into<String>, message: impl Into<String>) -> AdapterRejection {
    AdapterRejection::new(code, message)
}
