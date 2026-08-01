use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use asset_catalog::StoredAssetCatalog;
use asset_import::{
    admit_gltf_source, decode_import_manifest, decode_sidecar, encode_import_manifest,
    encode_sidecar, gltf_relative_resource_uris, import_animated_glb_asset, import_text,
    init_metadata, plan_animated_glb_import, plan_animated_gltf_import, plan_import, reconcile,
    reconcile_source_hash, GltfResource, GltfSourceClosure, ImportContext, ImportMode,
    ImportSettings, ImportedAnimatedGlb, ImportedAssets, SidecarMetadata, SourceUri,
    IMPORTER_VERSION, MAX_GLTF_RESOURCE_BYTES, MAX_GLTF_RESOURCE_COUNT,
    MAX_GLTF_TOTAL_RESOURCE_BYTES, MAX_SOURCE_BYTES,
};
use voxel_convert::source_sha256;

use crate::{
    StoredAsset, StoredAssetCatalogMetadata, StoredAssetImport, StoredImportSource, StoredProject,
};

use super::host_file::{read_host_file, write_host_file_atomic};
use super::project::publish_project_mutation;
use super::protocol::{
    AdapterRejection, AssetImportArtifactReadout, AssetImportDiagnosticReadout,
    AssetImportPlanReadout, ProjectMutationReceipt, StudioAssetImportSettings, StudioFileSelection,
    StudioProjectReadout,
};
use super::voxel::load_expected;
use super::ProjectLocation;

pub(crate) struct PreparedAssetImport {
    pub readout: AssetImportPlanReadout,
    assets: Option<PreparedImportedAssets>,
    manifest_json: Option<String>,
    sidecar_json: Option<String>,
    source: StoredImportSource,
    prior_generated_asset_ids: Vec<String>,
    runtime_resource: Option<PreparedRuntimeResource>,
}

#[derive(Clone)]
struct PreparedRuntimeResource {
    relative_path: String,
    bytes: Vec<u8>,
}

#[derive(Clone)]
enum PreparedImportedAssets {
    Static(ImportedAssets),
    Animated(ImportedAnimatedGlb),
}

impl PreparedImportedAssets {
    fn catalog(&self) -> &asset_catalog::AssetCatalog {
        match self {
            Self::Static(imported) => &imported.catalog,
            Self::Animated(imported) => &imported.catalog,
        }
    }

    fn asset_id(&self) -> &str {
        match self {
            Self::Static(imported) => &imported.static_mesh.asset,
            Self::Animated(imported) => &imported.animated_mesh.asset,
        }
    }
}

pub(crate) fn prepare_asset_import(
    location: &ProjectLocation,
    expected_project_hash: &str,
    source: StudioFileSelection,
    settings: StudioAssetImportSettings,
) -> Result<PreparedAssetImport, AdapterRejection> {
    let project = load_expected(location, expected_project_hash)?;
    let loaded = read_selection(location, &source)?;
    let importer_settings = ImportSettings {
        scale: settings.scale,
        generate_collision: settings.generate_collision,
        material_namespace: settings.material_namespace.clone(),
    };
    let sidecar = init_metadata(
        loaded.uri.clone(),
        &loaded.bytes,
        "mesh",
        IMPORTER_VERSION,
        importer_settings.clone(),
        &project.document().project_id,
    );
    prepare(
        expected_project_hash,
        source,
        loaded,
        importer_settings,
        settings,
        None,
        sidecar,
        Vec::new(),
    )
}

pub(crate) fn prepare_asset_reimport(
    location: &ProjectLocation,
    expected_project_hash: &str,
    asset_id: &str,
) -> Result<PreparedAssetImport, AdapterRejection> {
    let project = load_expected(location, expected_project_hash)?;
    let stored = project
        .document()
        .assets
        .iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| {
            reject(
                "assetImport.assetMissing",
                format!("project has no asset `{asset_id}`"),
            )
        })?;
    let import = stored.import.as_ref().ok_or_else(|| {
        reject(
            "assetImport.notImported",
            format!("asset `{asset_id}` has no retained import provenance"),
        )
    })?;
    let prior = decode_import_manifest(&import.manifest_json)
        .map_err(|error| reject("assetImport.invalidManifest", error.to_string()))?;
    let sidecar = decode_sidecar(&import.sidecar_json)
        .map_err(|error| reject("assetImport.invalidSidecar", error.to_string()))?;
    let source = selection_from_stored(&import.source);
    let loaded = read_selection(location, &source)?;
    let settings = StudioAssetImportSettings {
        scale: sidecar.import_settings.scale,
        generate_collision: sidecar.import_settings.generate_collision,
        material_namespace: sidecar.import_settings.material_namespace.clone(),
    };
    prepare(
        expected_project_hash,
        source,
        loaded,
        sidecar.import_settings.clone(),
        settings,
        Some(prior),
        sidecar,
        import.generated_asset_ids.clone(),
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    expected_project_hash: &str,
    source: StudioFileSelection,
    loaded: LoadedImportSource,
    importer_settings: ImportSettings,
    settings: StudioAssetImportSettings,
    prior: Option<asset_import::ImportManifest>,
    sidecar: SidecarMetadata,
    prior_generated_asset_ids: Vec<String>,
) -> Result<PreparedAssetImport, AdapterRejection> {
    let context = ImportContext {
        available_textures: None,
        settings: importer_settings,
    };
    let animated = is_animated_source(&loaded.uri);
    let gltf = loaded.gltf.as_ref();
    if animated && !matches!(&loaded.stored, StoredImportSource::Project { .. }) {
        return Err(reject(
            "assetImport.animatedHostSourceUnsupported",
            "animated GLB import requires a project-relative source so the retained runtime resource remains durable",
        ));
    }
    let source_text = if animated {
        None
    } else {
        Some(
            std::str::from_utf8(&loaded.bytes)
                .map_err(|error| reject("assetImport.sourceNotUtf8", error.to_string()))?,
        )
    };
    let plan = if let Some(gltf) = gltf {
        plan_animated_gltf_import(
            &loaded.uri,
            gltf,
            &context,
            ImportMode::DryRun,
            prior.as_ref(),
            Some(&sidecar),
        )
    } else if animated {
        plan_animated_glb_import(
            &loaded.uri,
            &loaded.bytes,
            &context,
            ImportMode::DryRun,
            prior.as_ref(),
            Some(&sidecar),
        )
    } else {
        plan_import(
            &loaded.uri,
            source_text.expect("non-GLB import has UTF-8 source"),
            &context,
            ImportMode::DryRun,
            prior.as_ref(),
            Some(&sidecar),
        )
    };
    let packed_gltf = if gltf.is_some() && !plan.has_errors {
        plan.files
            .iter()
            .find(|artifact| artifact.relative_path.ends_with(".glb"))
            .map(|artifact| artifact.bytes.clone())
    } else {
        None
    };
    let assets = if plan.has_errors {
        None
    } else if animated {
        let import_uri = if gltf.is_some() {
            SourceUri::RelativePath(format!(
                "{}.glb",
                loaded
                    .uri
                    .value()
                    .strip_suffix(".gltf")
                    .or_else(|| loaded.uri.value().strip_suffix(".GLTF"))
                    .unwrap_or(loaded.uri.value())
            ))
        } else {
            loaded.uri.clone()
        };
        import_animated_glb_asset(
            &import_uri,
            packed_gltf.as_deref().unwrap_or(&loaded.bytes),
            &context,
        )
        .assets
        .map(PreparedImportedAssets::Animated)
    } else {
        import_text(
            source_text.expect("non-GLB import has UTF-8 source"),
            loaded.uri.value(),
            &context,
        )
        .assets
        .map(PreparedImportedAssets::Static)
    };
    let manifest_json = plan
        .manifest
        .as_ref()
        .map(encode_import_manifest)
        .transpose()
        .map_err(|error| reject("assetImport.manifestEncode", error.to_string()))?;
    let sidecar_json = plan
        .sidecar_update
        .as_ref()
        .map(encode_sidecar)
        .transpose()
        .map_err(|error| reject("assetImport.sidecarEncode", error.to_string()))?;
    let generated_asset_ids = assets
        .as_ref()
        .map(|assets| {
            assets
                .catalog()
                .canonical()
                .entries
                .into_iter()
                .map(|entry| entry.id.as_str().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let generated_artifacts = plan
        .files
        .iter()
        .map(|artifact| AssetImportArtifactReadout {
            relative_path: artifact.relative_path.clone(),
            byte_count: artifact.bytes.len() as u64,
        })
        .collect::<Vec<_>>();
    let diagnostics = plan
        .diagnostics
        .iter()
        .map(|diagnostic| AssetImportDiagnosticReadout {
            severity: diagnostic.severity.label().to_string(),
            code: diagnostic.code.label().to_string(),
            locus: diagnostic.locus.clone(),
            message: diagnostic.message.clone(),
            remedy: diagnostic.remedy.clone(),
        })
        .collect::<Vec<_>>();
    let mesh_asset_id = plan
        .manifest
        .as_ref()
        .map(|manifest| manifest.mesh_asset_id.clone());
    let reimport_kind = plan
        .reimport
        .as_ref()
        .map(|reimport| reimport.label().to_string());
    let source_hash = plan.manifest.as_ref().map_or_else(
        || source_sha256(&loaded.bytes),
        |manifest| manifest.source_hash.clone(),
    );
    let source_byte_count = loaded.source_byte_count();
    let runtime_resource = packed_gltf
        .map(|bytes| {
            Ok(PreparedRuntimeResource {
                relative_path: packed_runtime_path(&source, &source_hash)?,
                bytes,
            })
        })
        .transpose()?;
    let seed = serde_json::to_vec(&serde_json::json!({
        "expectedProjectHash": expected_project_hash,
        "source": &source,
        "sourceHash": &source_hash,
        "sourceByteCount": source_byte_count,
        "settings": &settings,
        "manifest": &manifest_json,
        "generatedAssetIds": &generated_asset_ids,
        "hasErrors": plan.has_errors,
    }))
    .expect("closed asset-import plan seed serializes");
    let plan_hash = source_sha256(&seed);
    let plan_id = format!("asset-import-{}", &plan_hash[..16]);
    let readout = AssetImportPlanReadout {
        plan_id,
        plan_hash,
        expected_project_hash: expected_project_hash.to_string(),
        source,
        source_hash,
        source_byte_count,
        mesh_asset_id,
        reimport_kind,
        has_errors: plan.has_errors,
        diagnostics,
        generated_artifacts,
        generated_asset_ids,
        settings,
    };
    Ok(PreparedAssetImport {
        readout,
        assets,
        manifest_json,
        sidecar_json,
        source: loaded.stored,
        prior_generated_asset_ids,
        runtime_resource,
    })
}

pub(crate) fn apply_prepared_asset_import(
    location: &ProjectLocation,
    expected_project_hash: &str,
    prepared: &PreparedAssetImport,
    plan_id: &str,
    expected_plan_hash: &str,
) -> Result<(ProjectMutationReceipt, StudioProjectReadout), AdapterRejection> {
    if prepared.readout.plan_id != plan_id || prepared.readout.plan_hash != expected_plan_hash {
        return Err(reject(
            "assetImport.planMismatch",
            "prepared asset-import plan identity or hash does not match",
        ));
    }
    if prepared.readout.expected_project_hash != expected_project_hash {
        return Err(reject(
            "assetImport.stalePlan",
            "asset-import plan was prepared for a different project hash",
        ));
    }
    let assets = prepared.assets.clone().ok_or_else(|| {
        reject(
            "assetImport.planHasErrors",
            "asset-import plan has errors and has no publication candidate",
        )
    })?;
    let manifest_json = prepared.manifest_json.clone().ok_or_else(|| {
        reject(
            "assetImport.planIncomplete",
            "asset-import plan has no canonical manifest",
        )
    })?;
    let sidecar_json = prepared.sidecar_json.clone().ok_or_else(|| {
        reject(
            "assetImport.planIncomplete",
            "asset-import plan has no canonical sidecar",
        )
    })?;
    let mut stored_catalog = StoredAssetCatalog::from_catalog(assets.catalog())
        .map_err(|error| reject("assetImport.catalogEncode", error.to_string()))?;
    let mesh_asset_id = assets.asset_id().to_string();
    if matches!(&assets, PreparedImportedAssets::Animated(_)) {
        let StoredImportSource::Project { path } = &prepared.source else {
            return Err(reject(
                "assetImport.animatedHostSourceUnsupported",
                "animated GLB import requires a durable project-relative source",
            ));
        };
        let Some(entry) = stored_catalog
            .entries
            .iter_mut()
            .find(|entry| entry.id == mesh_asset_id)
        else {
            return Err(reject(
                "assetImport.catalogEncode",
                "animated import catalog omitted its mesh identity",
            ));
        };
        entry.source_path = Some(path.clone());
    }
    let generated_asset_ids = stored_catalog
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    let source = prepared.source.clone();
    let source_path = source.path().to_string();
    let source_hash = prepared.readout.source_hash.clone();
    let source_byte_count = prepared.readout.source_byte_count;
    let reimport_kind = prepared
        .readout
        .reimport_kind
        .clone()
        .unwrap_or_else(|| "structuralReload".to_string());
    let prior_generated = prepared
        .prior_generated_asset_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let receipt_plan_id = plan_id.to_string();
    let receipt_plan_hash = expected_plan_hash.to_string();
    let runtime_source_path = prepared
        .runtime_resource
        .as_ref()
        .map(|runtime| runtime.relative_path.clone());
    let installed_runtime = prepared
        .runtime_resource
        .as_ref()
        .map(|runtime| install_runtime_resource(location, runtime))
        .transpose()?;
    let publication =
        publish_project_mutation(location, expected_project_hash, move |_, project| {
            install_imported_assets(
                project,
                assets,
                stored_catalog,
                StoredAssetImport {
                    source,
                    source_hash,
                    source_byte_count,
                    importer_version: IMPORTER_VERSION,
                    manifest_json,
                    sidecar_json,
                    generated_asset_ids: generated_asset_ids.clone(),
                },
                &prior_generated,
                runtime_source_path.as_deref(),
            )?;
            Ok(ProjectMutationReceipt::AssetImportApplied {
                plan_id: receipt_plan_id,
                plan_hash: receipt_plan_hash,
                asset_id: mesh_asset_id,
                source_path,
                reimport_kind,
                generated_asset_ids,
            })
        });
    let published = match publication {
        Ok(published) => published,
        Err(error) => {
            if installed_runtime == Some(true) {
                if let Some(runtime) = &prepared.runtime_resource {
                    let _ = fs::remove_file(location.root().join(&runtime.relative_path));
                }
            }
            return Err(error);
        }
    };
    Ok((published.value, published.readout))
}

fn install_imported_assets(
    project: &mut StoredProject,
    imported: PreparedImportedAssets,
    stored_catalog: StoredAssetCatalog,
    import: StoredAssetImport,
    prior_generated: &BTreeSet<String>,
    runtime_source_path: Option<&str>,
) -> Result<(), AdapterRejection> {
    let generated = stored_catalog
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(collision) = project
        .assets
        .iter()
        .find(|asset| generated.contains(asset.id.as_str()) && !prior_generated.contains(&asset.id))
    {
        return Err(reject(
            "assetImport.assetCollision",
            format!(
                "generated asset `{}` collides with an unrelated project asset",
                collision.id
            ),
        ));
    }
    project
        .assets
        .retain(|asset| !prior_generated.contains(&asset.id));
    let mesh_id = imported.asset_id().to_string();
    for mut entry in stored_catalog.entries {
        let is_mesh = entry.id == mesh_id;
        if is_mesh {
            if let Some(runtime_source_path) = runtime_source_path {
                entry.source_path = Some(runtime_source_path.to_owned());
            }
        }
        let (static_mesh, animated_mesh) = match &imported {
            PreparedImportedAssets::Static(imported) => {
                (is_mesh.then(|| imported.static_mesh.clone()), None)
            }
            PreparedImportedAssets::Animated(imported) => {
                (None, is_mesh.then(|| imported.animated_mesh.clone()))
            }
        };
        project.assets.push(StoredAsset {
            id: entry.id,
            catalog: Some(StoredAssetCatalogMetadata {
                version: entry.version,
                hash: entry.hash,
                source_path: entry.source_path,
                label: entry.label,
                dependencies: entry.dependencies,
            }),
            static_mesh,
            animated_mesh,
            import: is_mesh.then(|| import.clone()),
            voxel_volume: None,
            voxel_object: None,
            voxel_edit_history: None,
            voxel_annotations: Vec::new(),
            material: entry.material,
        });
    }
    project.assets.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(())
}

struct LoadedImportSource {
    stored: StoredImportSource,
    uri: SourceUri,
    bytes: Vec<u8>,
    gltf: Option<GltfSourceClosure>,
}

impl LoadedImportSource {
    fn source_byte_count(&self) -> u64 {
        self.gltf
            .as_ref()
            .map_or(self.bytes.len() as u64, |source| {
                source
                    .resources
                    .iter()
                    .fold(source.root_json.len() as u64, |total, resource| {
                        total.saturating_add(resource.bytes.len() as u64)
                    })
            })
    }
}

fn is_animated_source(uri: &SourceUri) -> bool {
    let value = uri.value().to_ascii_lowercase();
    value.ends_with(".glb") || value.ends_with(".gltf")
}

fn read_selection(
    location: &ProjectLocation,
    selection: &StudioFileSelection,
) -> Result<LoadedImportSource, AdapterRejection> {
    match selection {
        StudioFileSelection::Project { path } => {
            let bytes = location
                .read_relative_file(path, MAX_SOURCE_BYTES as u64)
                .map_err(|error| reject("assetImport.projectFileRejected", error.to_string()))?;
            let gltf = path
                .to_ascii_lowercase()
                .ends_with(".gltf")
                .then(|| load_project_gltf_source(location, path, bytes.clone()))
                .transpose()?;
            Ok(LoadedImportSource {
                stored: StoredImportSource::Project { path: path.clone() },
                uri: SourceUri::RelativePath(path.clone()),
                bytes,
                gltf,
            })
        }
        StudioFileSelection::Host { path } => {
            let source = read_host_file(path, MAX_SOURCE_BYTES).map_err(|error| {
                reject(
                    "assetImport.hostFileRejected",
                    format!("{}: {}", error.code, error.message),
                )
                .at_path(path.clone())
            })?;
            let normalized = source.path.display().to_string();
            if normalized.to_ascii_lowercase().ends_with(".gltf") {
                return Err(reject(
                    "assetImport.animatedHostSourceUnsupported",
                    "JSON glTF import requires a project-relative source so its external closure and packed runtime GLB remain durable",
                ));
            }
            Ok(LoadedImportSource {
                stored: StoredImportSource::Host {
                    path: normalized.clone(),
                },
                uri: SourceUri::AbsolutePath(normalized),
                bytes: source.bytes,
                gltf: None,
            })
        }
    }
}

fn load_project_gltf_source(
    location: &ProjectLocation,
    source_path: &str,
    root_json: Vec<u8>,
) -> Result<GltfSourceClosure, AdapterRejection> {
    let uris = gltf_relative_resource_uris(&root_json)
        .map_err(|diagnostic| reject("assetImport.gltfClosureRejected", diagnostic.render()))?;
    if uris.len() > MAX_GLTF_RESOURCE_COUNT {
        return Err(reject(
            "assetImport.gltfResourceLimit",
            format!(
                "glTF references {} resources; limit is {MAX_GLTF_RESOURCE_COUNT}",
                uris.len()
            ),
        ));
    }
    let parent = Path::new(source_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let mut retained_bytes = 0usize;
    let mut resources = Vec::with_capacity(uris.len());
    for uri in uris {
        let remaining = MAX_GLTF_TOTAL_RESOURCE_BYTES
            .checked_sub(retained_bytes)
            .ok_or_else(|| {
                reject(
                    "assetImport.gltfResourceLimit",
                    "glTF resource total overflowed",
                )
            })?;
        let read_limit = MAX_GLTF_RESOURCE_BYTES.min(remaining);
        if read_limit == 0 {
            return Err(reject(
                "assetImport.gltfResourceLimit",
                format!(
                    "glTF external resources exceed the {MAX_GLTF_TOTAL_RESOURCE_BYTES}-byte aggregate limit"
                ),
            ));
        }
        let relative = parent.join(&uri);
        let relative = relative.to_str().ok_or_else(|| {
            reject(
                "assetImport.gltfResourcePathRejected",
                format!("glTF resource `{uri}` is not a UTF-8 project path"),
            )
        })?;
        let bytes = location
            .read_relative_file(relative, read_limit as u64)
            .map_err(|error| {
                reject(
                    "assetImport.gltfResourceRejected",
                    format!("glTF resource `{uri}` was rejected: {error}"),
                )
                .at_path(relative.to_owned())
            })?;
        retained_bytes = retained_bytes.checked_add(bytes.len()).ok_or_else(|| {
            reject(
                "assetImport.gltfResourceLimit",
                "glTF resource total overflowed",
            )
        })?;
        resources.push(GltfResource { uri, bytes });
    }
    Ok(GltfSourceClosure {
        root_json,
        resources,
    })
}

fn packed_runtime_path(
    source: &StudioFileSelection,
    source_hash: &str,
) -> Result<String, AdapterRejection> {
    let StudioFileSelection::Project { path } = source else {
        return Err(reject(
            "assetImport.animatedHostSourceUnsupported",
            "packed glTF runtime resources require a project-relative source",
        ));
    };
    let Some(stem) = path
        .strip_suffix(".gltf")
        .or_else(|| path.strip_suffix(".GLTF"))
    else {
        return Err(reject(
            "assetImport.invalidSourceExtension",
            "packed glTF runtime resources require a .gltf source",
        ));
    };
    let hash = source_hash.strip_prefix("sha256:").unwrap_or(source_hash);
    if hash.len() < 16 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(reject(
            "assetImport.invalidSourceHash",
            "glTF import did not produce a canonical source hash",
        ));
    }
    Ok(format!("{stem}.rusty-import-{}.glb", &hash[..16]))
}

fn install_runtime_resource(
    location: &ProjectLocation,
    runtime: &PreparedRuntimeResource,
) -> Result<bool, AdapterRejection> {
    let target = location.root().join(&runtime.relative_path);
    if target.exists() {
        let existing = location
            .read_relative_file(&runtime.relative_path, MAX_SOURCE_BYTES as u64)
            .map_err(|error| reject("assetImport.runtimeResourceRejected", error.to_string()))?;
        if existing == runtime.bytes {
            return Ok(false);
        }
        return Err(reject(
            "assetImport.runtimeResourceCollision",
            format!(
                "packed runtime resource `{}` already exists with different bytes",
                runtime.relative_path
            ),
        ));
    }
    write_host_file_atomic(
        target.to_str().ok_or_else(|| {
            reject(
                "assetImport.runtimeResourceRejected",
                "packed runtime target is not UTF-8",
            )
        })?,
        &runtime.bytes,
        None,
    )
    .map_err(|error| {
        reject(
            "assetImport.runtimeResourceRejected",
            format!("{}: {}", error.code, error.message),
        )
    })?;
    Ok(true)
}

pub(crate) fn import_source_status(
    location: &ProjectLocation,
    import: &StoredAssetImport,
) -> &'static str {
    let uri = match &import.source {
        StoredImportSource::Project { path } => SourceUri::RelativePath(path.clone()),
        StoredImportSource::Host { path } => SourceUri::AbsolutePath(path.clone()),
    };
    let Ok(sidecar) = decode_sidecar(&import.sidecar_json) else {
        return "metadataInvalid";
    };
    if let StoredImportSource::Project { path } = &import.source {
        if path.to_ascii_lowercase().ends_with(".gltf") {
            let Ok(root) = location.read_relative_file(path, MAX_SOURCE_BYTES as u64) else {
                return "unavailable";
            };
            let Ok(source) = load_project_gltf_source(location, path, root) else {
                return "unavailable";
            };
            let Ok(packed) = admit_gltf_source(&source) else {
                return "unavailable";
            };
            return reconcile_source_hash(Some(&sidecar), &uri, packed.source_hash).label();
        }
    }
    let bytes = match &import.source {
        StoredImportSource::Project { path } => {
            match location.read_relative_file(path, MAX_SOURCE_BYTES as u64) {
                Ok(bytes) => bytes,
                Err(_) => return "unavailable",
            }
        }
        StoredImportSource::Host { path } => match read_host_file(path, MAX_SOURCE_BYTES) {
            Ok(source) => source.bytes,
            Err(_) => return "unavailable",
        },
    };
    reconcile(Some(&sidecar), &uri, &bytes).label()
}

fn selection_from_stored(source: &StoredImportSource) -> StudioFileSelection {
    match source {
        StoredImportSource::Project { path } => StudioFileSelection::Project { path: path.clone() },
        StoredImportSource::Host { path } => StudioFileSelection::Host { path: path.clone() },
    }
}

fn reject(code: &str, message: impl Into<String>) -> AdapterRejection {
    AdapterRejection::new(code, message)
}
