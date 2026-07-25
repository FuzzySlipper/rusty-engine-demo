use crate::STORED_PROJECT_SCHEMA_VERSION;

use super::path::ProjectLocation;
use super::project::{apply_entity_translation, OpenedOwnerProject};
use super::protocol::{
    AdapterDescription, AdapterRejection, StudioAdapterRequest, StudioAdapterResponse,
    MAX_REQUEST_ID_BYTES, MAX_STUDIO_ADAPTER_REQUEST_BYTES, MAX_STUDIO_ADAPTER_RESPONSE_BYTES,
    STUDIO_ADAPTER_PROTOCOL_VERSION,
};

struct OpenProject {
    location: ProjectLocation,
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
                    adapter_version: 2,
                    protocol_version: STUDIO_ADAPTER_PROTOCOL_VERSION,
                    project_kind: "loadingBayProject",
                    project_schema_version: STORED_PROJECT_SCHEMA_VERSION,
                    operations: [
                        "describe",
                        "openProject",
                        "readProject",
                        "setEntityTranslation",
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
            } => self.set_entity_translation(
                request_id,
                &expected_project_hash,
                expected_scene_revision,
                entity_id,
                translation,
            ),
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
                self.open = Some(OpenProject { location });
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
            return StudioAdapterResponse::rejected(
                Some(request_id),
                AdapterRejection::new("project.notOpen", "no external project is open"),
            );
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

    fn set_entity_translation(
        &mut self,
        request_id: String,
        expected_project_hash: &str,
        expected_scene_revision: u64,
        entity_id: u64,
        translation: [f32; 3],
    ) -> StudioAdapterResponse {
        let Some(open) = &mut self.open else {
            return StudioAdapterResponse::rejected(
                Some(request_id),
                AdapterRejection::new("project.notOpen", "no external project is open"),
            );
        };
        match apply_entity_translation(
            &open.location,
            expected_project_hash,
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
