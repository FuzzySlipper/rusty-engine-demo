use std::collections::{BTreeMap, BTreeSet};

use asset_catalog::{
    decode_catalog, encode_catalog, validate_catalog, AssetCatalog, StoredAssetCatalog,
    StoredAssetReference, StoredAssetVersionRequirement, StoredCatalogEntry,
};
use authored_scene::{
    composed_world_transforms, encode_scene, validate_scene, AvailableSceneAsset,
    FlatSceneDocument, NodeMetadata, SceneAdmissionPlan, SceneEditCommand, SceneEditService,
    SceneMetadata, SceneNode, SceneNodeKind, SceneNodeRecord, SceneResolutionContext,
    SceneTransform, CURRENT_SCENE_SCHEMA_VERSION,
};
use content_store::{
    admit_source_batch, encode_manifest, ArtifactRole, ContentArtifact, ContentBody, ContentHash,
    ContentManifest, ContentSourceBatch, ContentStoreIdentity, ContentWrite, ContentWriteCandidate,
    ContentWriteSetDraft,
};
use core_assets::{AssetId, AssetKind, AssetReference, AssetVersionReq};
use core_ids::{SceneId, SceneNodeId};
use core_math::Vec3;
use engine_inspector::{
    inspect_catalog, inspect_content_manifest, inspect_entity_state, inspect_scene,
    inspect_voxel_state,
};
use entity_state::{encode_durable_snapshot, EntityState, EntityTransform};
use render_model::{
    MaterialUvStrategy, MeshAttribute, MeshAttributeKind, MeshAttributeName, MeshBoundsDescriptor,
    MeshBufferLayout, MeshCollisionPolicy, MeshGroupDescriptor, MeshIndexWidth, MeshMaterialSlot,
    MeshPayloadDescriptor, MeshPayloadSource, MeshProvenance, RenderAssetKind, RenderDiff,
    RenderFrameDiff, RenderMaterialDescriptor, ResolvedRenderAsset, StaticMeshAsset,
};
use render_projection::{EntityProjectionDiagnostic, EntityRenderProjector};

use crate::{
    admit_stored_project_with_document, encode_project_document, AdmittedProject,
    AdmittedStoredProject, DecodedProjectDocument, ProjectStore, StoredEntityDefinition,
    StoredProject, StoredScene, StoredVoxelEnvironment, STORED_PROJECT_SCHEMA_VERSION,
};

use super::path::ProjectLocation;
use super::protocol::{
    AdapterRejection, CanonicalOwnerContent, EntityTranslationReceipt, LoadingBayDomainReadout,
    OwnerInspections, ProjectionDiagnosticReadout, ProjectionReadout, SceneHierarchyNodeReadout,
    SceneHierarchyReadout, StudioProjectIdentity, StudioProjectReadout, TransformReadout,
};
use super::voxel::{project_voxel_authoring, voxel_authoring_readout};

const PROJECT_RESOURCE_ROLE: &str = "resource:loading-bay-project";
const JSON_SAFE_U64_MASK: u64 = (1_u64 << 53) - 1;

pub struct OpenedOwnerProject {
    source_schema_version: u32,
    source_bytes: String,
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
            location.relative_project_file(),
            source.source_bytes().to_string(),
            source.decoded,
            source_schema_version,
        )
    }

    fn admit_source(
        relative_project_file: &str,
        source_bytes: String,
        decoded: DecodedProjectDocument,
        source_schema_version: u32,
    ) -> Result<Self, AdapterRejection> {
        let source_hash = ContentHash::of(source_bytes.as_bytes());
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
            source_schema_version,
            source_bytes,
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
        let project = self.stored.document();
        let entry_scene = entry_scene(project);
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

        let voxel_projected = project_voxel_authoring(project, &self.catalog)?;
        let projection =
            complete_projection(&self.catalog, projected.frame, voxel_projected.frame)?;

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
                catalog: inspect_catalog(&self.catalog, None),
                scene: inspect_scene(&self.scene, Some(&self.catalog)),
                entity_state: inspect_entity_state(self.admitted.session.entities()),
                persistence: inspect_content_manifest(&self.manifest),
            },
            scene_hierarchy: scene_hierarchy(&self.scene, self.admitted.session.entities()),
            voxel: self
                .admitted
                .collision_scene
                .as_ref()
                .map(inspect_voxel_state),
            voxel_authoring: voxel_authoring_readout(project)?,
            loading_bay: loading_bay_readout(entry_scene),
            projection,
            projection_readout: ProjectionReadout {
                frame_kind: "complete",
                source_revision: projected.readout.source_revision,
                retained_entities: projected.readout.retained_entities,
                retained_voxel_instances: voxel_projected.instance_count,
                retained_voxel_chunks: voxel_projected.chunk_count,
                diagnostics,
            },
        })
    }
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
        location.relative_project_file(),
        candidate_bytes,
        candidate_decoded,
        STORED_PROJECT_SCHEMA_VERSION,
    )?;
    let staged_readout = staged.readout()?;

    let installed_hash = ProjectStore::default()
        .replace_if_unchanged(location.project_file(), &staged.stored, expected_hash)
        .map_err(project_store_rejection)?;
    location
        .revalidate()
        .map_err(|error| reject("path.changedDuringWrite", error.to_string()))?;
    let reread = OpenedOwnerProject::load(location)?;
    if reread.source_hash != installed_hash || reread.source_bytes != staged.source_bytes {
        return Err(reject(
            "project.canonicalRereadMismatch",
            "atomic replacement did not reread as the admitted canonical candidate",
        ));
    }
    let observed_next =
        ContentStoreIdentity::from_manifest(next_content_revision, &reread.manifest)
            .map_err(|error| reject("content.confirmationRejected", error.to_string()))?;
    authorized
        .confirm(&observed_next)
        .map_err(|error| reject("content.publicationMismatch", error.to_string()))?;

    Ok(PublishedProject {
        value,
        project_hash_before: expected_hash,
        project_hash_after: installed_hash,
        content_candidate_hash,
        scene_revision_after: staged.scene_revision(),
        readout: staged_readout,
    })
}

fn complete_projection(
    catalog: &AssetCatalog,
    instances: RenderFrameDiff,
    voxels: RenderFrameDiff,
) -> Result<RenderFrameDiff, AdapterRejection> {
    let mut operations = Vec::new();
    for entry in catalog
        .iter()
        .filter(|entry| entry.kind() == AssetKind::StaticMesh)
    {
        let asset = entry.id.as_str();
        let material = format!(
            "material/studio-{}",
            asset.trim_start_matches("mesh/").replace('/', "-")
        );
        operations.push(RenderDiff::DefineMaterial {
            material: studio_material(&material, asset),
        });
        operations.push(RenderDiff::DefineStaticMesh {
            asset: StaticMeshAsset {
                asset: asset.to_string(),
                payload: cuboid_payload(studio_mesh_dimensions(asset)),
                material_slots: vec![MeshMaterialSlot { slot: 0, material }],
                collision: MeshCollisionPolicy::VisualOnly,
            },
        });
    }
    operations.extend(voxels.ops);
    operations.extend(instances.ops);
    RenderFrameDiff::try_from_ops(operations)
        .map_err(|error| reject("projection.completeFrameRejected", format!("{error:?}")))
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
    let mut nodes = Vec::with_capacity(scene.nodes.len());
    for root in &tree.roots {
        append_hierarchy_node(root, None, 0, &world, &child_orders, &entities, &mut nodes);
    }
    SceneHierarchyReadout {
        scene_id: scene.id.raw(),
        revision: scene.revision,
        name: scene.metadata.name.clone(),
        root_node_ids: tree.roots.iter().map(|node| node.id.raw()).collect(),
        nodes,
    }
}

fn append_hierarchy_node(
    node: &SceneNode,
    parent: Option<u64>,
    depth: u32,
    world: &BTreeMap<SceneNodeId, EntityTransform>,
    child_orders: &BTreeMap<SceneNodeId, u32>,
    entities: &BTreeMap<SceneNodeId, core_ids::EntityId>,
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
            let dependencies = asset
                .voxel_volume
                .iter()
                .flat_map(|voxel| &voxel.material_palette)
                .map(|binding| StoredAssetReference {
                    id: binding.material_asset_id.clone(),
                    version: StoredAssetVersionRequirement::Exact { value: 1 },
                    hash: None,
                })
                .collect();
            Ok(StoredCatalogEntry {
                id: asset.id.clone(),
                version: 1,
                hash: None,
                source_path: None,
                label: Some(asset.id.clone()),
                dependencies,
                material: asset.material.clone(),
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
    for (index, entity) in source.entities.iter().enumerate() {
        let kind = match &entity.renderable {
            Some(renderable) => {
                let id = AssetId::parse(&renderable.asset)
                    .map_err(|error| reject("scene.invalidAsset", error.to_string()))?;
                let reference = AssetReference::new(id, AssetVersionReq::Exact(1), None);
                dependencies.insert(renderable.asset.clone(), reference.clone());
                SceneNodeKind::StaticMesh(reference)
            }
            None => SceneNodeKind::EmptyGroup,
        };
        nodes.push(SceneNodeRecord {
            id: SceneNodeId::new(entity.id),
            parent: None,
            child_order: index as u32,
            transform: SceneTransform::at(
                entity
                    .translation
                    .map_or(Vec3::ZERO, |value| Vec3::new(value[0], value[1], value[2])),
            ),
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
        .filter(|entry| entry.kind() == AssetKind::StaticMesh)
        .map(|entry| {
            (
                entry.id.as_str().to_string(),
                ResolvedRenderAsset {
                    id: entry.id.as_str().to_string(),
                    kind: RenderAssetKind::StaticMesh,
                    content_hash: entry.hash.as_ref().map(|hash| hash.as_str().to_string()),
                    version: entry.version,
                },
            )
        })
        .collect()
}

fn loading_bay_readout(scene: &StoredScene) -> LoadingBayDomainReadout {
    LoadingBayDomainReadout {
        scene_name: scene.name.clone(),
        entity_count: scene.entities.len(),
        door_count: count_where(scene, |entity| entity.door.is_some()),
        switch_count: count_where(scene, |entity| entity.switch.is_some()),
        enemy_count: count_where(scene, |entity| entity.enemy),
        encounter_count: count_where(scene, |entity| entity.encounter.is_some()),
        extraction_beacon_count: count_where(scene, |entity| entity.extraction_beacon.is_some()),
        navigator_count: count_where(scene, |entity| entity.navigation.is_some()),
        player_controller_count: count_where(scene, |entity| entity.player_controller.is_some()),
        weapon_count: count_where(scene, |entity| entity.weapon.is_some()),
        voxel_environment: match scene.voxel_environment {
            Some(StoredVoxelEnvironment::Solid(_)) => "solid",
            Some(StoredVoxelEnvironment::Material(_)) => "material",
            Some(StoredVoxelEnvironment::GeneratedRoom(_)) => "generatedRoom",
            None => "none",
        },
    }
}

fn count_where(scene: &StoredScene, predicate: impl Fn(&StoredEntityDefinition) -> bool) -> usize {
    scene
        .entities
        .iter()
        .filter(|entity| predicate(entity))
        .count()
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
        RenderAssetKind::Audio => "audio",
        RenderAssetKind::Font => "font",
    }
}

fn project_store_rejection(error: crate::ProjectStoreError) -> AdapterRejection {
    let code = match error {
        crate::ProjectStoreError::StaleSource { .. } => "project.staleHash",
        crate::ProjectStoreError::TooLarge { .. } => "project.tooLarge",
        crate::ProjectStoreError::InvalidUtf8 { .. } => "project.invalidUtf8",
        crate::ProjectStoreError::Codec(_) => "project.decodeRejected",
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
