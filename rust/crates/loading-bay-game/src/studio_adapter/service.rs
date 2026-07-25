use voxel_convert::PreparedVoxelConversion;

use crate::STORED_PROJECT_SCHEMA_VERSION;

use super::path::ProjectLocation;
use super::project::{apply_entity_translation, OpenedOwnerProject};
use super::protocol::{
    AdapterDescription, AdapterRejection, ProjectMutationReceipt, StudioAdapterRequest,
    StudioAdapterResponse, StudioProjectReadout, VoxelReadout, MAX_REQUEST_ID_BYTES,
    MAX_STUDIO_ADAPTER_REQUEST_BYTES, MAX_STUDIO_ADAPTER_RESPONSE_BYTES,
    STUDIO_ADAPTER_PROTOCOL_VERSION,
};
use super::voxel::{
    apply_brush, apply_prepared_conversion, apply_prepared_history_revert, apply_primitive,
    attach_voxel_instance, create_annotation_layer, duplicate_voxel_asset, edit_annotation,
    export_annotation, export_voxel_asset_file, import_voxel_asset_file, initialize_voxel_asset,
    initialize_voxel_template, materialize_project_environment, prepare_conversion,
    prepare_history_revert, query_annotation, query_history, query_model, redo_edit,
    remove_voxel_instance, replace_palette, revert_history, set_voxel_instance_transform,
    undo_edit, upsert_material, validate_pick, PreparedProjectHistoryRevert,
};

struct OpenProject {
    location: ProjectLocation,
    prepared_conversion: Option<PreparedVoxelConversion>,
    prepared_history_revert: Option<(String, PreparedProjectHistoryRevert)>,
    next_history_preview_id: u64,
}

#[derive(Default)]
pub struct StudioAdapterService {
    open: Option<OpenProject>,
}

impl StudioAdapterService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_json(&mut self, input: &str) -> String {
        if input.len() > MAX_STUDIO_ADAPTER_REQUEST_BYTES {
            return encode_response(StudioAdapterResponse::rejected(
                None,
                AdapterRejection::new(
                    "protocol.requestTooLarge",
                    format!(
                        "request is {} bytes, exceeding the {}-byte bound",
                        input.len(),
                        MAX_STUDIO_ADAPTER_REQUEST_BYTES
                    ),
                ),
            ));
        }
        let request = match serde_json::from_str::<StudioAdapterRequest>(input.trim_end()) {
            Ok(request) => request,
            Err(error) => {
                return encode_response(StudioAdapterResponse::rejected(
                    None,
                    AdapterRejection::new("protocol.malformedRequest", error.to_string()),
                ));
            }
        };
        encode_response(self.handle(request))
    }

    pub fn handle(&mut self, request: StudioAdapterRequest) -> StudioAdapterResponse {
        let request_id = request.request_id().to_string();
        if request_id.trim().is_empty() || request_id.len() > MAX_REQUEST_ID_BYTES {
            return StudioAdapterResponse::rejected(
                None,
                AdapterRejection::new(
                    "protocol.invalidRequestId",
                    "requestId must be nonblank and within the byte bound",
                ),
            );
        }
        if request.protocol_version() != STUDIO_ADAPTER_PROTOCOL_VERSION {
            return StudioAdapterResponse::rejected(
                Some(request_id),
                AdapterRejection::new(
                    "protocol.unsupportedVersion",
                    format!(
                        "protocol version {} is unsupported; expected {}",
                        request.protocol_version(),
                        STUDIO_ADAPTER_PROTOCOL_VERSION
                    ),
                ),
            );
        }

        match request {
            StudioAdapterRequest::Describe { .. } => StudioAdapterResponse::Described {
                protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                request_id,
                adapter: AdapterDescription {
                    adapter_id: "rusty-engine-demo.loading-bay",
                    adapter_version: 4,
                    protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                    project_kind: "loadingBayProject",
                    project_schema_version: STORED_PROJECT_SCHEMA_VERSION,
                    operations: vec![
                        "describe",
                        "openProject",
                        "readProject",
                        "setEntityTranslation",
                        "upsertMaterial",
                        "initializeVoxelAsset",
                        "duplicateVoxelAsset",
                        "attachVoxelInstance",
                        "setVoxelInstanceTransform",
                        "removeVoxelInstance",
                        "replaceVoxelPalette",
                        "validateVoxelPick",
                        "applyVoxelBrush",
                        "applyVoxelPrimitive",
                        "initializeVoxelTemplate",
                        "importVoxelAssetFile",
                        "exportVoxelAssetFile",
                        "materializeEnvironment",
                        "undoVoxelEdit",
                        "redoVoxelEdit",
                        "revertVoxelHistory",
                        "queryVoxelHistory",
                        "prepareVoxelHistoryRevert",
                        "applyVoxelHistoryRevert",
                        "discardVoxelHistoryRevert",
                        "createVoxelAnnotationLayer",
                        "editVoxelAnnotation",
                        "queryVoxelAnnotation",
                        "exportVoxelAnnotation",
                        "queryVoxelModel",
                        "prepareVoxelConversion",
                        "applyVoxelConversion",
                        "discardVoxelConversion",
                        "closeProject",
                    ],
                },
            },
            StudioAdapterRequest::OpenProject {
                root, project_file, ..
            } => self.open_project(request_id, &root, &project_file),
            StudioAdapterRequest::ReadProject { .. } => self.read_project(request_id),
            StudioAdapterRequest::SetEntityTranslation {
                expected_project_hash,
                expected_scene_revision,
                entity_id,
                translation,
                ..
            } => {
                let Some(open) = self.open.as_ref() else {
                    return not_open(request_id);
                };
                match apply_entity_translation(
                    &open.location,
                    &expected_project_hash,
                    expected_scene_revision,
                    entity_id,
                    translation,
                ) {
                    Ok((receipt, project)) => StudioAdapterResponse::EntityTranslationApplied {
                        protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                        request_id,
                        receipt,
                        project,
                    },
                    Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
                }
            }
            StudioAdapterRequest::UpsertMaterial {
                expected_project_hash,
                asset_id,
                definition,
                ..
            } => self.mutate(request_id, |location| {
                upsert_material(location, &expected_project_hash, asset_id, definition)
            }),
            StudioAdapterRequest::InitializeVoxelAsset {
                expected_project_hash,
                asset_id,
                cell_size,
                chunk_size,
                origin,
                bounds,
                material_palette,
                initial_material_slot,
                ..
            } => self.mutate(request_id, |location| {
                initialize_voxel_asset(
                    location,
                    &expected_project_hash,
                    asset_id,
                    cell_size,
                    chunk_size,
                    origin,
                    bounds,
                    material_palette,
                    initial_material_slot,
                )
            }),
            StudioAdapterRequest::DuplicateVoxelAsset {
                expected_project_hash,
                source_asset_id,
                expected_source_content_hash,
                target_asset_id,
                ..
            } => self.mutate(request_id, |location| {
                duplicate_voxel_asset(
                    location,
                    &expected_project_hash,
                    source_asset_id,
                    expected_source_content_hash,
                    target_asset_id,
                )
            }),
            StudioAdapterRequest::AttachVoxelInstance {
                expected_project_hash,
                scene_id,
                instance,
                ..
            } => self.mutate(request_id, |location| {
                attach_voxel_instance(location, &expected_project_hash, scene_id, instance)
            }),
            StudioAdapterRequest::SetVoxelInstanceTransform {
                expected_project_hash,
                scene_id,
                instance_id,
                translation,
                rotation,
                scale,
                ..
            } => self.mutate(request_id, |location| {
                set_voxel_instance_transform(
                    location,
                    &expected_project_hash,
                    scene_id,
                    instance_id,
                    translation,
                    rotation,
                    scale,
                )
            }),
            StudioAdapterRequest::RemoveVoxelInstance {
                expected_project_hash,
                scene_id,
                instance_id,
                ..
            } => self.mutate(request_id, |location| {
                remove_voxel_instance(location, &expected_project_hash, scene_id, instance_id)
            }),
            StudioAdapterRequest::ReplaceVoxelPalette {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                expected_voxel_data_hash,
                replacement,
                ..
            } => self.mutate(request_id, |location| {
                replace_palette(
                    location,
                    &expected_project_hash,
                    asset_id,
                    expected_asset_content_hash,
                    expected_voxel_data_hash,
                    replacement,
                )
            }),
            StudioAdapterRequest::ValidateVoxelPick {
                expected_project_hash,
                scene_id,
                instance_id,
                origin,
                direction,
                max_distance,
                claimed_voxel,
                claimed_face,
                ..
            } => {
                let Some(open) = self.open.as_ref() else {
                    return not_open(request_id);
                };
                match validate_pick(
                    &open.location,
                    &expected_project_hash,
                    &scene_id,
                    &instance_id,
                    origin,
                    direction,
                    max_distance,
                    claimed_voxel,
                    claimed_face,
                ) {
                    Ok(anchor) => StudioAdapterResponse::VoxelPickValidated {
                        protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                        request_id,
                        anchor,
                    },
                    Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
                }
            }
            StudioAdapterRequest::ApplyVoxelBrush {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                center,
                radius,
                mode,
                material_slot,
                ..
            } => self.mutate(request_id, |location| {
                apply_brush(
                    location,
                    &expected_project_hash,
                    asset_id,
                    expected_asset_content_hash,
                    center,
                    radius,
                    mode,
                    material_slot,
                )
            }),
            StudioAdapterRequest::ApplyVoxelPrimitive {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                request,
                ..
            } => self.mutate(request_id, |location| {
                apply_primitive(
                    location,
                    &expected_project_hash,
                    asset_id,
                    expected_asset_content_hash,
                    request,
                )
            }),
            StudioAdapterRequest::InitializeVoxelTemplate {
                expected_project_hash,
                asset_id,
                cell_size,
                chunk_size,
                material_palette,
                request,
                ..
            } => self.mutate(request_id, |location| {
                initialize_voxel_template(
                    location,
                    &expected_project_hash,
                    asset_id,
                    cell_size,
                    chunk_size,
                    material_palette,
                    request,
                )
            }),
            StudioAdapterRequest::ImportVoxelAssetFile {
                expected_project_hash,
                source_path,
                target_asset_id,
                ..
            } => self.mutate(request_id, |location| {
                import_voxel_asset_file(
                    location,
                    &expected_project_hash,
                    source_path,
                    target_asset_id,
                )
            }),
            StudioAdapterRequest::ExportVoxelAssetFile {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                target_path,
                expected_target_sha256,
                ..
            } => {
                let Some(open) = self.open.as_ref() else {
                    return not_open(request_id);
                };
                match export_voxel_asset_file(
                    &open.location,
                    &expected_project_hash,
                    &asset_id,
                    &expected_asset_content_hash,
                    &target_path,
                    expected_target_sha256.as_deref(),
                ) {
                    Ok(receipt) => StudioAdapterResponse::VoxelAssetFileExported {
                        protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                        request_id,
                        asset_id,
                        target_path: receipt.path.display().to_string(),
                        byte_count: receipt.byte_count,
                        sha256: receipt.sha256,
                        replaced_existing: receipt.replaced_existing,
                    },
                    Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
                }
            }
            StudioAdapterRequest::MaterializeEnvironment {
                expected_project_hash,
                expected_scene_revision,
                scene_id,
                preset,
                seed,
                voxel_asset_id,
                voxel_instance_id,
                voxel_translation,
                player_entity_id,
                exit_entity_id,
                wall_material,
                floor_material,
                accent_material,
                material_palette,
                ..
            } => self.mutate(request_id, |location| {
                materialize_project_environment(
                    location,
                    &expected_project_hash,
                    expected_scene_revision,
                    scene_id,
                    preset,
                    seed,
                    voxel_asset_id,
                    voxel_instance_id,
                    voxel_translation,
                    player_entity_id,
                    exit_entity_id,
                    wall_material,
                    floor_material,
                    accent_material,
                    material_palette,
                )
            }),
            StudioAdapterRequest::UndoVoxelEdit {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                ..
            } => self.mutate(request_id, |location| {
                undo_edit(
                    location,
                    &expected_project_hash,
                    asset_id,
                    expected_asset_content_hash,
                )
            }),
            StudioAdapterRequest::RedoVoxelEdit {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                ..
            } => self.mutate(request_id, |location| {
                redo_edit(
                    location,
                    &expected_project_hash,
                    asset_id,
                    expected_asset_content_hash,
                )
            }),
            StudioAdapterRequest::RevertVoxelHistory {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                target_cursor,
                ..
            } => self.mutate(request_id, |location| {
                revert_history(
                    location,
                    &expected_project_hash,
                    asset_id,
                    expected_asset_content_hash,
                    target_cursor,
                )
            }),
            StudioAdapterRequest::QueryVoxelHistory {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                max_entries,
                max_deltas_per_entry,
                ..
            } => self.read_voxel(request_id, |location| {
                query_history(
                    location,
                    &expected_project_hash,
                    &asset_id,
                    &expected_asset_content_hash,
                    max_entries,
                    max_deltas_per_entry,
                )
            }),
            StudioAdapterRequest::PrepareVoxelHistoryRevert {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                target_cursor,
                max_samples,
                ..
            } => {
                let Some(open) = self.open.as_mut() else {
                    return not_open(request_id);
                };
                let preview_id = format!("history-preview-{}", open.next_history_preview_id);
                open.next_history_preview_id = open.next_history_preview_id.saturating_add(1);
                match prepare_history_revert(
                    &open.location,
                    preview_id.clone(),
                    expected_project_hash,
                    asset_id,
                    expected_asset_content_hash,
                    target_cursor,
                    max_samples,
                ) {
                    Ok((prepared, preview)) => {
                        open.prepared_history_revert = Some((preview_id, prepared));
                        StudioAdapterResponse::VoxelHistoryRevertPrepared {
                            protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                            request_id,
                            preview,
                        }
                    }
                    Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
                }
            }
            StudioAdapterRequest::ApplyVoxelHistoryRevert {
                expected_project_hash,
                preview_id,
                ..
            } => {
                let Some(open) = self.open.as_mut() else {
                    return not_open(request_id);
                };
                let Some((retained_id, prepared)) = open.prepared_history_revert.take() else {
                    return StudioAdapterResponse::rejected(
                        Some(request_id),
                        AdapterRejection::new(
                            "voxel.historyPreviewMissing",
                            format!("no prepared history preview `{preview_id}`"),
                        ),
                    );
                };
                if retained_id != preview_id {
                    open.prepared_history_revert = Some((retained_id, prepared));
                    return StudioAdapterResponse::rejected(
                        Some(request_id),
                        AdapterRejection::new(
                            "voxel.historyPreviewMissing",
                            format!("no prepared history preview `{preview_id}`"),
                        ),
                    );
                }
                match apply_prepared_history_revert(
                    &open.location,
                    &expected_project_hash,
                    prepared,
                ) {
                    Ok((receipt, project)) => mutation_response(request_id, receipt, project),
                    Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
                }
            }
            StudioAdapterRequest::DiscardVoxelHistoryRevert { preview_id, .. } => {
                let Some(open) = self.open.as_mut() else {
                    return not_open(request_id);
                };
                if open
                    .prepared_history_revert
                    .as_ref()
                    .is_none_or(|(retained_id, _)| retained_id != &preview_id)
                {
                    return StudioAdapterResponse::rejected(
                        Some(request_id),
                        AdapterRejection::new(
                            "voxel.historyPreviewMissing",
                            format!("no prepared history preview `{preview_id}`"),
                        ),
                    );
                }
                open.prepared_history_revert = None;
                StudioAdapterResponse::VoxelHistoryRevertDiscarded {
                    protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                    request_id,
                    preview_id,
                }
            }
            StudioAdapterRequest::CreateVoxelAnnotationLayer {
                expected_project_hash,
                asset_id,
                draft,
                ..
            } => self.mutate(request_id, |location| {
                create_annotation_layer(location, &expected_project_hash, asset_id, draft)
            }),
            StudioAdapterRequest::EditVoxelAnnotation {
                expected_project_hash,
                asset_id,
                layer_id,
                transaction,
                ..
            } => self.mutate(request_id, |location| {
                edit_annotation(
                    location,
                    &expected_project_hash,
                    asset_id,
                    layer_id,
                    transaction,
                )
            }),
            StudioAdapterRequest::QueryVoxelAnnotation {
                expected_project_hash,
                asset_id,
                layer_id,
                query,
                ..
            } => self.read_voxel(request_id, |location| {
                query_annotation(
                    location,
                    &expected_project_hash,
                    &asset_id,
                    &layer_id,
                    query,
                )
            }),
            StudioAdapterRequest::ExportVoxelAnnotation {
                expected_project_hash,
                asset_id,
                layer_id,
                expected_layer_hash,
                ..
            } => self.read_voxel(request_id, |location| {
                export_annotation(
                    location,
                    &expected_project_hash,
                    &asset_id,
                    &layer_id,
                    &expected_layer_hash,
                )
            }),
            StudioAdapterRequest::QueryVoxelModel {
                expected_project_hash,
                asset_id,
                expected_asset_content_hash,
                window,
                ..
            } => self.read_voxel(request_id, |location| {
                query_model(
                    location,
                    &expected_project_hash,
                    &asset_id,
                    &expected_asset_content_hash,
                    window,
                )
            }),
            StudioAdapterRequest::PrepareVoxelConversion {
                expected_project_hash,
                source_asset_id,
                source,
                target_asset_id,
                license,
                mesh_primitive,
                settings,
                max_preview_samples,
                ..
            } => {
                let Some(open) = self.open.as_mut() else {
                    return not_open(request_id);
                };
                match prepare_conversion(
                    &open.location,
                    &expected_project_hash,
                    source_asset_id,
                    source,
                    target_asset_id,
                    license,
                    mesh_primitive,
                    *settings,
                    max_preview_samples,
                ) {
                    Ok((prepared, plan, preview)) => {
                        open.prepared_conversion = Some(prepared);
                        StudioAdapterResponse::VoxelConversionPrepared {
                            protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                            request_id,
                            plan,
                            preview,
                        }
                    }
                    Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
                }
            }
            StudioAdapterRequest::ApplyVoxelConversion {
                expected_project_hash,
                plan_id,
                expected_plan_hash,
                expected_output_hash,
                ..
            } => {
                let Some(open) = self.open.as_mut() else {
                    return not_open(request_id);
                };
                let Some(prepared) = open
                    .prepared_conversion
                    .as_ref()
                    .filter(|prepared| prepared.plan().plan_id == plan_id)
                    .cloned()
                else {
                    return StudioAdapterResponse::rejected(
                        Some(request_id),
                        AdapterRejection::new(
                            "conversion.planMissing",
                            format!("no prepared conversion `{plan_id}`"),
                        ),
                    );
                };
                match apply_prepared_conversion(
                    &open.location,
                    &expected_project_hash,
                    &prepared,
                    plan_id.clone(),
                    expected_plan_hash,
                    expected_output_hash,
                ) {
                    Ok((receipt, project)) => {
                        open.prepared_conversion = None;
                        mutation_response(request_id, receipt, project)
                    }
                    Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
                }
            }
            StudioAdapterRequest::DiscardVoxelConversion { plan_id, .. } => {
                let Some(open) = self.open.as_mut() else {
                    return not_open(request_id);
                };
                if open
                    .prepared_conversion
                    .as_ref()
                    .is_none_or(|prepared| prepared.plan().plan_id != plan_id)
                {
                    return StudioAdapterResponse::rejected(
                        Some(request_id),
                        AdapterRejection::new(
                            "conversion.planMissing",
                            format!("no prepared conversion `{plan_id}`"),
                        ),
                    );
                }
                open.prepared_conversion = None;
                StudioAdapterResponse::VoxelConversionDiscarded {
                    protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                    request_id,
                    plan_id,
                }
            }
            StudioAdapterRequest::CloseProject { .. } => {
                self.open = None;
                StudioAdapterResponse::ProjectClosed {
                    protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                    request_id,
                }
            }
        }
    }

    fn open_project(
        &mut self,
        request_id: String,
        root: &str,
        project_file: &str,
    ) -> StudioAdapterResponse {
        let result = (|| {
            let location = ProjectLocation::resolve(root, project_file)
                .map_err(|error| AdapterRejection::new("path.rejected", error.to_string()))?;
            let project = OpenedOwnerProject::load(&location)?;
            let readout = project.readout()?;
            Ok::<_, AdapterRejection>((location, readout))
        })();
        match result {
            Ok((location, project)) => {
                self.open = Some(OpenProject {
                    location,
                    prepared_conversion: None,
                    prepared_history_revert: None,
                    next_history_preview_id: 1,
                });
                StudioAdapterResponse::ProjectOpened {
                    protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                    request_id,
                    project,
                }
            }
            Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
        }
    }

    fn read_project(&mut self, request_id: String) -> StudioAdapterResponse {
        let Some(open) = &mut self.open else {
            return not_open(request_id);
        };
        let result = (|| {
            let project = OpenedOwnerProject::load(&open.location)?;
            project.readout()
        })();
        match result {
            Ok(project) => StudioAdapterResponse::ProjectRead {
                protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                request_id,
                project,
            },
            Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
        }
    }

    fn mutate(
        &self,
        request_id: String,
        operation: impl FnOnce(
            &ProjectLocation,
        ) -> Result<
            (ProjectMutationReceipt, StudioProjectReadout),
            AdapterRejection,
        >,
    ) -> StudioAdapterResponse {
        let Some(open) = self.open.as_ref() else {
            return not_open(request_id);
        };
        match operation(&open.location) {
            Ok((receipt, project)) => mutation_response(request_id, receipt, project),
            Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
        }
    }

    fn read_voxel(
        &self,
        request_id: String,
        operation: impl FnOnce(&ProjectLocation) -> Result<VoxelReadout, AdapterRejection>,
    ) -> StudioAdapterResponse {
        let Some(open) = self.open.as_ref() else {
            return not_open(request_id);
        };
        match operation(&open.location) {
            Ok(readout) => StudioAdapterResponse::VoxelRead {
                protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                request_id,
                readout,
            },
            Err(error) => StudioAdapterResponse::rejected(Some(request_id), error),
        }
    }
}

fn mutation_response(
    request_id: String,
    receipt: ProjectMutationReceipt,
    project: StudioProjectReadout,
) -> StudioAdapterResponse {
    StudioAdapterResponse::ProjectMutationApplied {
        protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
        request_id,
        receipt,
        project,
    }
}

fn not_open(request_id: String) -> StudioAdapterResponse {
    StudioAdapterResponse::rejected(
        Some(request_id),
        AdapterRejection::new("project.notOpen", "no external project is open"),
    )
}

fn encode_response(response: StudioAdapterResponse) -> String {
    let encoded = serde_json::to_string(&response)
        .expect("closed Studio adapter responses contain serializable values");
    if encoded.len() <= MAX_STUDIO_ADAPTER_RESPONSE_BYTES {
        return encoded;
    }
    serde_json::to_string(&StudioAdapterResponse::rejected(
        None,
        AdapterRejection::new(
            "protocol.responseTooLarge",
            format!(
                "response exceeds the {}-byte bound",
                MAX_STUDIO_ADAPTER_RESPONSE_BYTES
            ),
        ),
    ))
    .expect("bounded rejection response serializes")
}
