use std::collections::BTreeSet;

use asset_catalog::StoredAssetCatalog;
use asset_import::{
    decode_import_manifest, decode_sidecar, encode_import_manifest, encode_sidecar, import_text,
    init_metadata, plan_import, ImportContext, ImportMode, ImportSettings, ImportedAssets,
    SidecarMetadata, SourceUri, IMPORTER_VERSION, MAX_SOURCE_BYTES,
};
use voxel_convert::source_sha256;

use crate::{
    StoredAsset, StoredAssetCatalogMetadata, StoredAssetImport, StoredImportSource, StoredProject,
};

use super::host_file::read_host_file;
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
    assets: Option<ImportedAssets>,
    manifest_json: Option<String>,
    sidecar_json: Option<String>,
    source: StoredImportSource,
    prior_generated_asset_ids: Vec<String>,
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
    let source_text = std::str::from_utf8(&loaded.bytes)
        .map_err(|error| reject("assetImport.sourceNotUtf8", error.to_string()))?;
    let context = ImportContext {
        available_textures: None,
        settings: importer_settings,
    };
    let plan = plan_import(
        &loaded.uri,
        source_text,
        &context,
        ImportMode::DryRun,
        prior.as_ref(),
        Some(&sidecar),
    );
    let assets = if plan.has_errors {
        None
    } else {
        import_text(source_text, loaded.uri.value(), &context).assets
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
                .catalog
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
    let seed = serde_json::to_vec(&serde_json::json!({
        "expectedProjectHash": expected_project_hash,
        "source": &source,
        "sourceHash": &source_hash,
        "sourceByteCount": loaded.bytes.len(),
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
        source_byte_count: loaded.bytes.len() as u64,
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
    let stored_catalog = StoredAssetCatalog::from_catalog(&assets.catalog)
        .map_err(|error| reject("assetImport.catalogEncode", error.to_string()))?;
    let mesh_asset_id = assets.static_mesh.asset.clone();
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
    let published =
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
            )?;
            Ok(ProjectMutationReceipt::AssetImportApplied {
                plan_id: receipt_plan_id,
                plan_hash: receipt_plan_hash,
                asset_id: mesh_asset_id,
                source_path,
                reimport_kind,
                generated_asset_ids,
            })
        })?;
    Ok((published.value, published.readout))
}

fn install_imported_assets(
    project: &mut StoredProject,
    imported: ImportedAssets,
    stored_catalog: StoredAssetCatalog,
    import: StoredAssetImport,
    prior_generated: &BTreeSet<String>,
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
    let mesh_id = imported.static_mesh.asset.clone();
    for entry in stored_catalog.entries {
        let is_mesh = entry.id == mesh_id;
        project.assets.push(StoredAsset {
            id: entry.id,
            catalog: Some(StoredAssetCatalogMetadata {
                version: entry.version,
                hash: entry.hash,
                source_path: entry.source_path,
                label: entry.label,
                dependencies: entry.dependencies,
            }),
            static_mesh: is_mesh.then(|| imported.static_mesh.clone()),
            animated_mesh: None,
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
            Ok(LoadedImportSource {
                stored: StoredImportSource::Project { path: path.clone() },
                uri: SourceUri::RelativePath(path.clone()),
                bytes,
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
            Ok(LoadedImportSource {
                stored: StoredImportSource::Host {
                    path: normalized.clone(),
                },
                uri: SourceUri::AbsolutePath(normalized),
                bytes: source.bytes,
            })
        }
    }
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
