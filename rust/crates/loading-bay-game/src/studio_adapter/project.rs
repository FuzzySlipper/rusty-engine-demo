use std::collections::{BTreeMap, BTreeSet};

use asset_catalog::{encode_catalog, validate_catalog, AssetCatalog, CatalogEntry};
use authored_scene::{
    encode_scene, validate_scene, AvailableSceneAsset, FlatSceneDocument, NodeMetadata,
    SceneAdmissionPlan, SceneEditCommand, SceneEditService, SceneMetadata, SceneNodeKind,
    SceneNodeRecord, SceneResolutionContext, SceneTransform, CURRENT_SCENE_SCHEMA_VERSION,
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
use entity_state::{encode_durable_snapshot, EntityState};
use render_model::{RenderAssetKind, ResolvedRenderAsset};
use render_projection::{EntityProjectionDiagnostic, EntityRenderProjector};

use crate::{
    admit_stored_project_with_document, encode_project_document, AdmittedProject,
    AdmittedStoredProject, DecodedProjectDocument, ProjectStore, StoredEntityDefinition,
    StoredProject, StoredScene, StoredVoxelEnvironment, STORED_PROJECT_SCHEMA_VERSION,
};

use super::path::ProjectLocation;
use super::protocol::{
    AdapterRejection, CanonicalOwnerContent, EntityTranslationReceipt, LoadingBayDomainReadout,
    OwnerInspections, ProjectionDiagnosticReadout, ProjectionReadout, StudioProjectIdentity,
    StudioProjectReadout,
};

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

    pub fn readout(
        &self,
        projector: &mut EntityRenderProjector,
    ) -> Result<StudioProjectReadout, AdapterRejection> {
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
        let projected = projector
            .project(self.admitted.session.entities(), &render_assets)
            .map_err(|error| reject("projection.rejected", format!("{error:?}")))?;
        let diagnostics = projected
            .diagnostics
            .into_iter()
            .map(projection_diagnostic)
            .collect();

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
            voxel: self
                .admitted
                .collision_scene
                .as_ref()
                .map(inspect_voxel_state),
            loading_bay: loading_bay_readout(entry_scene),
            projection: projected.frame,
            projection_readout: ProjectionReadout {
                source_revision: projected.readout.source_revision,
                retained_entities: projected.readout.retained_entities,
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
    projector: &mut EntityRenderProjector,
) -> Result<(EntityTranslationReceipt, StudioProjectReadout), AdapterRejection> {
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
    let Some(source_node) = current.scene.nodes.iter().find(|node| node.id == node_id) else {
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

    let mut document_candidate = current.stored.document().clone();
    let scene = entry_scene_mut(&mut document_candidate);
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
    let candidate_hash = write_candidate.candidate_hash();
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
    let mut staged_projector = projector.clone();
    let staged_readout = staged.readout(&mut staged_projector)?;

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

    *projector = staged_projector;
    let receipt = EntityTranslationReceipt {
        entity_id,
        translation,
        project_hash_before: expected_hash.to_hex(),
        project_hash_after: installed_hash.to_hex(),
        scene_revision_before: expected_scene_revision,
        scene_revision_after: staged.scene_revision(),
        content_candidate_hash: candidate_hash.to_hex(),
    };
    Ok((receipt, staged_readout))
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
                .map(|id| CatalogEntry::new(id, 1).with_label(asset.id.clone()))
                .map_err(|error| reject("catalog.invalidAsset", error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let catalog = AssetCatalog::from_entries(entries).canonical();
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
