//! Loading Bay's product-owned developer-command composition.
//!
//! Engine owns the common envelope, discovery, correlation preflight, command
//! history, and standard command markers. This module composes those public
//! mechanisms with Loading Bay's bounded queue, safe-point borrowed dispatch,
//! product play marker, host DTO mappings, delayed ordinary outcome bridge,
//! and deterministic generation retirement. `LoadingBayProductService` keeps
//! the live gameplay owner and calls these hooks at explicit lifecycle points.

use rusty_engine::developer_command::{
    map_command_response, CommandBindings, CommandDescriptor, CommandId, CommandLane,
    CommandProfile, CommandRequest, CommandResponse, DeveloperCommand, DispatchError,
    DispatchFacts, HandlerResult, HostCommandDiscovery, HostCommandOutcome, HostCommandRequest,
    HostCommandResponse, HostDecimalU64, HostErrorBody, HostErrorCode, HostErrorMessage,
    HostReceiptRefs, HostResponseContext, ParameterDescriptor, ProfileId, RuntimeInstanceId,
    TypeDescriptor,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::product_service::{
    command_identity, LoadingBayProductService, LoadingBayProjectReadout,
};
use crate::{LoadingBayServiceCommand, LoadingBayServiceError};

pub const DEVELOPER_COMMAND_PROFILE: &str = "loading-bay.developer";
pub const DEVELOPER_COMMAND_CATALOG_EPOCH: u64 = 1;
pub const DEVELOPER_COMMAND_HISTORY_CAPACITY: usize = 128;
pub(crate) const MAX_PENDING_DEVELOPER_COMMANDS: usize = 64;
const MAX_DEVELOPER_COMMAND_RESULTS: usize = 128;

pub type LoadingBayDeveloperCommandRequest = HostCommandRequest<Value>;
pub type LoadingBayDeveloperCommandResponse = HostCommandResponse<Value, Value>;

#[derive(Debug, Clone)]
pub(crate) struct PendingDeveloperCommand {
    generation: u64,
    request: CommandRequest<Value>,
    response_context: HostResponseContext,
}

#[derive(Debug)]
pub(crate) struct PendingDeveloperPlay {
    generation: u64,
    sequence: u64,
    response: CommandResponse<LoadingBayPlayAdmission, LoadingBayDeveloperOwnerError>,
    response_context: HostResponseContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayPlayAdmission {
    pub connection_generation: u64,
    pub command_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadingBayPlayCompletion {
    pub kind: String,
    pub connection_generation: u64,
    pub command_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostTrackSetReceipt {
    pub entity: String,
    pub track: String,
    pub before: i64,
    pub after: i64,
    pub committed_tracks_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadingBayDeveloperOwnerError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LoadingBayPlayServiceCommand;

impl DeveloperCommand for LoadingBayPlayServiceCommand {
    type Request = LoadingBayServiceCommand;
    type Reply = LoadingBayPlayAdmission;
    type Error = LoadingBayDeveloperOwnerError;

    fn descriptor() -> CommandDescriptor {
        CommandDescriptor::new(
            CommandId::parse("loading-bay.play.service-command")
                .expect("fixed Loading Bay developer command identity"),
            Vec::new(),
            CommandLane::Play,
            "Queue one existing Loading Bay semantic service command for ordinary fixed-step consumption.",
            vec![ParameterDescriptor::new(
                "command",
                "A Loading Bay service command admitted by the ordinary product owner.",
                true,
                TypeDescriptor::Record { fields: Vec::new() },
            )],
            TypeDescriptor::Record { fields: Vec::new() },
            TypeDescriptor::Record { fields: Vec::new() },
        )
        .expect("fixed Loading Bay developer command descriptor")
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostEntityRequest {
    entity: String,
}

impl HostEntityRequest {
    pub fn into_entity(self) -> Result<rusty_engine::core_ids::EntityId, String> {
        self.entity
            .parse::<u64>()
            .map(rusty_engine::core_ids::EntityId::new)
            .map_err(|_| "developer inspect entity must be a decimal u64".to_owned())
    }
}

pub fn create_bindings(
    runtime: RuntimeInstanceId,
    revision: u64,
) -> Result<CommandBindings, LoadingBayServiceError> {
    let profile = CommandProfile::new(
        ProfileId::parse(DEVELOPER_COMMAND_PROFILE).expect("fixed developer profile identity"),
        [CommandLane::Inspect, CommandLane::Play, CommandLane::Admin],
    )
    .expect("fixed developer profile lanes");
    let mut bindings = CommandBindings::new(
        profile,
        DispatchFacts {
            runtime,
            revision,
            catalog_epoch: DEVELOPER_COMMAND_CATALOG_EPOCH,
        },
        DEVELOPER_COMMAND_HISTORY_CAPACITY,
    )
    .map_err(|error| owner_error("developerBindingsInvalid", error.to_string()))?;
    bindings
        .expose_borrowed::<rusty_engine::developer_command_standard::InspectEntity>()
        .and_then(|()| {
            bindings.expose_borrowed::<rusty_engine::developer_command_standard::InspectMechanics>()
        })
        .and_then(|()| {
            bindings.expose_borrowed::<rusty_engine::developer_command_standard::AdminSetTrack>()
        })
        .and_then(|()| bindings.expose_borrowed::<LoadingBayPlayServiceCommand>())
        .map_err(|error| owner_error("developerBindingsInvalid", error.to_string()))?;
    Ok(bindings)
}

pub fn decode_payload<T: DeserializeOwned>(
    request: CommandRequest<Value>,
) -> Result<CommandRequest<T>, String> {
    let payload = serde_json::from_value(request.payload).map_err(|error| error.to_string())?;
    Ok(CommandRequest {
        protocol_version: request.protocol_version,
        command: request.command,
        correlation: request.correlation,
        runtime: request.runtime,
        expected: request.expected,
        cancelled: request.cancelled,
        timed_out: request.timed_out,
        payload,
    })
}

pub fn decode_entity_payload(
    request: CommandRequest<Value>,
) -> Result<CommandRequest<rusty_engine::core_ids::EntityId>, String> {
    let request = decode_payload::<HostEntityRequest>(request)?;
    let entity = request.payload.into_entity()?;
    Ok(CommandRequest {
        protocol_version: request.protocol_version,
        command: request.command,
        correlation: request.correlation,
        runtime: request.runtime,
        expected: request.expected,
        cancelled: request.cancelled,
        timed_out: request.timed_out,
        payload: entity,
    })
}

pub fn erase_host_response<R: Serialize, E: Serialize>(
    response: HostCommandResponse<R, E>,
) -> LoadingBayDeveloperCommandResponse {
    let outcome = match response.outcome {
        HostCommandOutcome::Success {
            value,
            receipt_refs,
        } => HostCommandOutcome::Success {
            value: serde_json::to_value(value).expect("admitted developer reply must serialize"),
            receipt_refs,
        },
        HostCommandOutcome::Error {
            code,
            message,
            details,
        } => HostCommandOutcome::Error {
            code,
            message,
            details: details.map(|details| {
                serde_json::to_value(details).expect("admitted developer error must serialize")
            }),
        },
    };
    HostCommandResponse {
        correlation: response.correlation,
        runtime: response.runtime,
        profile: response.profile,
        revision: response.revision,
        catalog_epoch: response.catalog_epoch,
        outcome,
    }
}

pub fn project_track_response(
    response: CommandResponse<
        rusty_engine::gameplay_mechanics::TrackSetReceipt,
        rusty_engine::gameplay_mechanics::MechanicsError,
    >,
) -> CommandResponse<HostTrackSetReceipt, rusty_engine::gameplay_mechanics::MechanicsError> {
    CommandResponse {
        protocol_version: response.protocol_version,
        provenance: response.provenance,
        facts: response.facts,
        result: match response.result {
            HandlerResult::Success(receipt) => HandlerResult::Success(HostTrackSetReceipt {
                entity: receipt.entity.raw().to_string(),
                track: receipt.track.as_str().to_owned(),
                before: receipt.before.get(),
                after: receipt.after.get(),
                committed_tracks_revision: receipt.committed_tracks_revision.to_string(),
            }),
            HandlerResult::Rejected(error) => HandlerResult::Rejected(error),
        },
    }
}

pub fn host_error(
    context: HostResponseContext,
    facts: &DispatchFacts,
    code: &'static str,
    message: impl Into<String>,
) -> LoadingBayDeveloperCommandResponse {
    HostCommandResponse {
        correlation: context.correlation().clone(),
        runtime: facts.runtime.clone(),
        profile: context.profile().clone(),
        revision: HostDecimalU64::new(facts.revision),
        catalog_epoch: HostDecimalU64::new(facts.catalog_epoch),
        outcome: HostCommandOutcome::Error {
            code: HostErrorCode::parse(code).expect("fixed product error identity"),
            message: bounded_message(message),
            details: None,
        },
    }
}

pub fn mapped_owner_error(error: LoadingBayDeveloperOwnerError) -> HostErrorBody<Value> {
    HostErrorBody {
        code: HostErrorCode::parse(error.code).expect("fixed product error identity"),
        message: bounded_message(error.message),
        details: None,
    }
}

pub fn mechanics_error(
    error: rusty_engine::gameplay_mechanics::MechanicsError,
) -> HostErrorBody<Value> {
    HostErrorBody {
        code: HostErrorCode::parse("mechanics.rejected").expect("fixed mechanics error identity"),
        message: bounded_message(error.to_string()),
        details: None,
    }
}

pub fn infallible_error(error: std::convert::Infallible) -> HostErrorBody<Value> {
    match error {}
}

pub fn receipt_refs(values: &[String]) -> HostReceiptRefs {
    HostReceiptRefs::new(
        values
            .iter()
            .map(|value| {
                rusty_engine::developer_command::HostReceiptRef::parse(value.clone())
                    .expect("product receipt identity")
            })
            .collect(),
    )
    .expect("bounded product receipt identities")
}

pub fn owner_error(code: &'static str, message: impl Into<String>) -> LoadingBayServiceError {
    LoadingBayServiceError {
        code,
        message: message.into(),
    }
}

fn bounded_message(message: impl Into<String>) -> HostErrorMessage {
    let mut message = message.into();
    while message.len() > rusty_engine::developer_command::MAX_HOST_ERROR_MESSAGE_BYTES {
        message.pop();
    }
    HostErrorMessage::new(message).expect("message was truncated to the public host bound")
}

pub(crate) fn developer_runtime_identity(project: &LoadingBayProjectReadout) -> RuntimeInstanceId {
    RuntimeInstanceId::parse(format!("loading-bay-{}", project.content_hash))
        .expect("content hash yields a fixed runtime identity")
}

fn developer_contract_fingerprint(project: &LoadingBayProjectReadout) -> CommandId {
    CommandId::parse(format!("loading-bay.{}", project.content_hash))
        .expect("content hash yields a fixed contract fingerprint")
}

fn developer_error(code: &'static str, message: impl Into<String>) -> LoadingBayServiceError {
    LoadingBayServiceError {
        code,
        message: message.into(),
    }
}

impl LoadingBayProductService {
    /// Discovers only commands this product actually binds at its safe point.
    /// A separate developer transport never creates or owns a gameplay session.
    pub fn discover_developer_commands(
        &self,
    ) -> Result<HostCommandDiscovery, LoadingBayServiceError> {
        self.developer_context()?;
        let bindings = self
            .developer_bindings
            .as_ref()
            .expect("developer bindings are restored after safe-point dispatch");
        Ok(HostCommandDiscovery::from_bindings(
            bindings,
            developer_contract_fingerprint(&self.project),
        ))
    }

    /// Accepts a bounded request but never executes it inline. A result is
    /// available only after `advance` reaches the product safe point.
    pub fn submit_developer_command(
        &mut self,
        request: LoadingBayDeveloperCommandRequest,
    ) -> Result<(), LoadingBayServiceError> {
        let generation = self.developer_context()?;
        if self.pending_developer_commands.len() >= MAX_PENDING_DEVELOPER_COMMANDS {
            return Err(developer_error(
                "queueSaturated",
                "developer command queue is full",
            ));
        }
        let (request, response_context) = request
            .into_command_parts()
            .map_err(|error| developer_error("invalidHostEnvelope", error.to_string()))?;
        self.pending_developer_commands
            .push_back(PendingDeveloperCommand {
                generation,
                request,
                response_context,
            });
        Ok(())
    }

    pub fn poll_developer_command(
        &mut self,
        correlation: &str,
    ) -> Option<LoadingBayDeveloperCommandResponse> {
        let index = self
            .developer_results
            .iter()
            .position(|result| result.correlation.as_str() == correlation)?;
        self.developer_results.remove(index)
    }

    /// Cancelling drops only work that has not reached the safe point; it
    /// never attempts to roll back a named service that already committed.
    pub fn cancel_developer_command(&mut self, correlation: &str) -> bool {
        let Some(index) = self
            .pending_developer_commands
            .iter()
            .position(|pending| pending.request.correlation.as_str() == correlation)
        else {
            return false;
        };
        self.pending_developer_commands.remove(index);
        true
    }

    fn developer_context(&self) -> Result<u64, LoadingBayServiceError> {
        let generation = self.developer_generation.ok_or_else(|| {
            developer_error(
                "gameplayUnavailable",
                "developer commands require an active Loading Bay gameplay session",
            )
        })?;
        if generation != self.runtime.input_session().connection_generation {
            return Err(developer_error(
                "retiredGeneration",
                "developer command context belongs to a retired gameplay session",
            ));
        }
        Ok(generation)
    }

    pub(crate) fn consume_developer_commands(&mut self) {
        let active = self.runtime.input_session().connection_generation;
        if self.developer_generation.is_none() {
            return;
        }
        if self.developer_generation != Some(active) {
            self.retire_developer_commands(
                "developer request was retired by a gameplay session replacement",
            );
            self.developer_generation = Some(active);
            return;
        }
        while let Some(pending) = self.pending_developer_commands.pop_front() {
            let response = if pending.generation != active {
                Some(host_error(
                    pending.response_context,
                    self.developer_facts(),
                    "retired-generation",
                    "developer request belongs to a retired gameplay session",
                ))
            } else {
                self.execute_developer_command(pending.request, pending.response_context)
            };
            if let Some(response) = response {
                self.push_developer_result(response);
            }
        }
    }

    pub(crate) fn retire_developer_commands(&mut self, message: &'static str) {
        while let Some(pending) = self.pending_developer_commands.pop_front() {
            let response = host_error(
                pending.response_context,
                self.developer_facts(),
                "retired-generation",
                message,
            );
            self.push_developer_result(response);
        }
        let correlations = self
            .pending_developer_plays
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for correlation in correlations {
            let pending = self
                .pending_developer_plays
                .remove(&correlation)
                .expect("collected pending developer play");
            let response = host_error(
                pending.response_context,
                self.developer_facts(),
                "retired-generation",
                message,
            );
            self.push_developer_result(response);
        }
    }

    pub(crate) fn resolve_developer_plays(&mut self) {
        let settled = self
            .pending_developer_plays
            .iter()
            .filter_map(|(correlation, pending)| {
                self.pending_outcomes
                    .iter()
                    .find(|outcome| {
                        outcome.command_sequence == Some(pending.sequence)
                            && outcome.connection_generation == pending.generation
                    })
                    .map(|outcome| (correlation.clone(), outcome.clone()))
            })
            .collect::<Vec<_>>();
        for (correlation, outcome) in settled {
            let pending = self
                .pending_developer_plays
                .remove(&correlation)
                .expect("settled pending developer play");
            let completion = if outcome.kind == "CommandConsumed" {
                CommandResponse {
                    protocol_version: pending.response.protocol_version,
                    provenance: pending.response.provenance,
                    facts: pending.response.facts,
                    result: HandlerResult::Success(LoadingBayPlayCompletion {
                        kind: outcome.kind,
                        connection_generation: outcome.connection_generation,
                        command_sequence: pending.sequence,
                    }),
                }
            } else {
                CommandResponse {
                    protocol_version: pending.response.protocol_version,
                    provenance: pending.response.provenance,
                    facts: pending.response.facts,
                    result: HandlerResult::Rejected(DispatchError::Command(
                        LoadingBayDeveloperOwnerError {
                            code: "service-command-rejected",
                            message: outcome.message.unwrap_or_else(|| {
                                "ordinary service command did not complete".to_owned()
                            }),
                        },
                    )),
                }
            };
            let refs = if matches!(completion.result, HandlerResult::Success(_)) {
                receipt_refs(&[format!("loading-bay.service.{}", pending.sequence)])
            } else {
                HostReceiptRefs::empty()
            };
            let mapped = map_command_response(
                completion,
                pending.response_context,
                refs,
                mapped_owner_error,
            );
            self.push_developer_result(erase_host_response(mapped.wire));
        }
    }

    fn execute_developer_command(
        &mut self,
        request: CommandRequest<Value>,
        response_context: HostResponseContext,
    ) -> Option<LoadingBayDeveloperCommandResponse> {
        match request.command.as_str() {
            "standard.inspect.entity" => {
                let request = match decode_entity_payload(request) {
                    Ok(request) => request,
                    Err(message) => {
                        return Some(host_error(
                            response_context,
                            self.developer_facts(),
                            "invalid-payload",
                            message,
                        ));
                    }
                };
                let mut bindings = self.take_developer_bindings();
                let response = bindings.dispatch_borrowed::<
                    rusty_engine::developer_command_standard::InspectEntity,
                    _,
                >(request, &mut |_context, entity| {
                    Ok::<_, std::convert::Infallible>(
                        self.runtime
                            .runtime()
                            .session()
                            .developer_inspect_entity(entity),
                    )
                });
                self.developer_bindings = Some(bindings);
                let mapped = map_command_response(
                    response,
                    response_context,
                    HostReceiptRefs::empty(),
                    infallible_error,
                );
                Some(erase_host_response(mapped.wire))
            }
            "standard.inspect.mechanics" => {
                let request = match decode_entity_payload(request) {
                    Ok(request) => request,
                    Err(message) => {
                        return Some(host_error(
                            response_context,
                            self.developer_facts(),
                            "invalid-payload",
                            message,
                        ));
                    }
                };
                let mut bindings = self.take_developer_bindings();
                let response = bindings.dispatch_borrowed::<
                    rusty_engine::developer_command_standard::InspectMechanics,
                    _,
                >(request, &mut |_context, entity| {
                    self.runtime
                        .runtime()
                        .session()
                        .developer_inspect_mechanics(entity)
                });
                self.developer_bindings = Some(bindings);
                let mapped = map_command_response(
                    response,
                    response_context,
                    HostReceiptRefs::empty(),
                    mechanics_error,
                );
                Some(erase_host_response(mapped.wire))
            }
            "standard.admin.track.set" => {
                let request = match decode_payload::<
                    rusty_engine::developer_command_standard::HostTrackSetRequest,
                >(request)
                .and_then(|request| {
                    let payload = self
                        .runtime
                        .runtime()
                        .session()
                        .developer_map_track_set(request.payload)?;
                    Ok(CommandRequest {
                        protocol_version: request.protocol_version,
                        command: request.command,
                        correlation: request.correlation,
                        runtime: request.runtime,
                        expected: request.expected,
                        cancelled: request.cancelled,
                        timed_out: request.timed_out,
                        payload,
                    })
                }) {
                    Ok(request) => request,
                    Err(message) => {
                        return Some(host_error(
                            response_context,
                            self.developer_facts(),
                            "invalid-payload",
                            message,
                        ));
                    }
                };
                let mut bindings = self.take_developer_bindings();
                let response = bindings.dispatch_borrowed::<
                    rusty_engine::developer_command_standard::AdminSetTrack,
                    _,
                >(request, &mut |_context, request| {
                    self.runtime
                        .runtime_mut()
                        .session_mut()
                        .developer_set_track(request)
                });
                self.developer_bindings = Some(bindings);
                let mapped = map_command_response(
                    project_track_response(response),
                    response_context,
                    receipt_refs(&["standard.admin.track.set".to_owned()]),
                    mechanics_error,
                );
                Some(erase_host_response(mapped.wire))
            }
            "loading-bay.play.service-command" => {
                let request = match decode_payload::<LoadingBayServiceCommand>(request) {
                    Ok(request) => request,
                    Err(message) => {
                        return Some(host_error(
                            response_context,
                            self.developer_facts(),
                            "invalid-payload",
                            message,
                        ));
                    }
                };
                let correlation = request.correlation.to_string();
                let mut bindings = self.take_developer_bindings();
                let response = bindings.dispatch_borrowed::<LoadingBayPlayServiceCommand, _>(
                    request,
                    &mut |_context, command| {
                        let (generation, sequence, _) = command_identity(&command);
                        self.submit(command)
                            .map(|_| LoadingBayPlayAdmission {
                                connection_generation: generation,
                                command_sequence: sequence,
                            })
                            .map_err(|error| LoadingBayDeveloperOwnerError {
                                code: "service-command-rejected",
                                message: format!("{}: {}", error.code, error.message),
                            })
                    },
                );
                self.developer_bindings = Some(bindings);
                if let HandlerResult::Success(admission) = &response.result {
                    self.pending_developer_plays.insert(
                        correlation,
                        PendingDeveloperPlay {
                            generation: admission.connection_generation,
                            sequence: admission.command_sequence,
                            response,
                            response_context,
                        },
                    );
                    None
                } else {
                    let mapped = map_command_response(
                        response,
                        response_context,
                        HostReceiptRefs::empty(),
                        mapped_owner_error,
                    );
                    Some(erase_host_response(mapped.wire))
                }
            }
            _ => Some(host_error(
                response_context,
                self.developer_facts(),
                "command-unavailable",
                "developer command is not exposed by Loading Bay",
            )),
        }
    }

    pub(crate) fn refresh_developer_facts(&mut self) {
        let facts = DispatchFacts {
            runtime: developer_runtime_identity(&self.project),
            revision: self
                .developer_generation
                .unwrap_or(self.runtime.input_session().connection_generation),
            catalog_epoch: DEVELOPER_COMMAND_CATALOG_EPOCH,
        };
        self.developer_bindings
            .as_mut()
            .expect("developer bindings are restored outside dispatch")
            .set_facts(facts);
    }

    fn developer_facts(&self) -> &DispatchFacts {
        self.developer_bindings
            .as_ref()
            .expect("developer bindings are restored outside dispatch")
            .facts()
    }

    fn take_developer_bindings(&mut self) -> CommandBindings {
        self.developer_bindings
            .take()
            .expect("developer bindings are not recursively dispatched")
    }

    fn push_developer_result(&mut self, result: LoadingBayDeveloperCommandResponse) {
        self.developer_results.push_back(result);
        while self.developer_results.len() > MAX_DEVELOPER_COMMAND_RESULTS {
            self.developer_results.pop_front();
        }
    }
}
