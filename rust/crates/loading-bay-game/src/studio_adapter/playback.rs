use rusty_engine::render_projection::VoxelObjectRenderProjector;
use rusty_engine::voxel_object_runtime::{
    admit_voxel_object, AdmittedVoxelObject, VoxelObjectLoopMode, VoxelObjectPlaybackRate,
    VoxelObjectPlaybackStatus, VoxelObjectPlayer, VoxelObjectRuntimeLimits,
};

use crate::{StoredVoxelObjectFrameSelection, StoredVoxelObjectInstance};

use super::project::OpenedOwnerProject;
use super::protocol::{
    AdapterRejection, ProjectionReadout, VoxelObjectInstancePlaybackReadout,
    VoxelObjectPlaybackCommand,
};

#[derive(Default)]
pub(crate) struct StudioVoxelObjectPlayback {
    session: Option<PlaybackSession>,
}

struct PlaybackSession {
    scene_id: String,
    instance_id: String,
    voxel_object_asset_id: String,
    player: VoxelObjectPlayer,
}

pub(crate) struct PlaybackPresentation {
    pub readout: VoxelObjectInstancePlaybackReadout,
    pub projection: rusty_engine::render_model::RenderFrameDiff,
    pub projection_readout: ProjectionReadout,
}

impl StudioVoxelObjectPlayback {
    pub(crate) fn clear(&mut self) {
        self.session = None;
    }

    pub(crate) fn present(
        &mut self,
        project: &OpenedOwnerProject,
        projector: &mut VoxelObjectRenderProjector,
        scene_id: &str,
        instance_id: &str,
        now_microseconds: u64,
        command: &VoxelObjectPlaybackCommand,
    ) -> Result<PlaybackPresentation, AdapterRejection> {
        let (instance, object) = playback_target(project, scene_id, instance_id)?;
        let durable_runtime_frame = resolve_frame(&object, &instance.frame)?;

        match command {
            VoxelObjectPlaybackCommand::Scrub {
                clip_id,
                clip_frame,
                loop_mode,
            } => {
                let mut player = VoxelObjectPlayer::new();
                player
                    .scrub(&object, clip_id, *clip_frame, *loop_mode)
                    .map_err(player_rejection)?;
                self.session = Some(PlaybackSession {
                    scene_id: scene_id.to_owned(),
                    instance_id: instance_id.to_owned(),
                    voxel_object_asset_id: instance.voxel_object_asset_id.clone(),
                    player,
                });
            }
            VoxelObjectPlaybackCommand::Play => {
                let session = self.require_target_mut(scene_id, instance_id)?;
                let current = session
                    .player
                    .sample_at(&object, now_microseconds)
                    .map_err(player_rejection)?;
                if current.ended && current.loop_mode == VoxelObjectLoopMode::Once {
                    let clip = current.clip.map(str::to_owned).ok_or_else(|| {
                        reject(
                            "voxelObject.playbackRejected",
                            "ended voxel-object playback has no selected clip",
                        )
                    })?;
                    session
                        .player
                        .scrub(&object, &clip, 0, VoxelObjectLoopMode::Once)
                        .map_err(player_rejection)?;
                }
                session
                    .player
                    .resume(now_microseconds)
                    .map_err(player_rejection)?;
            }
            VoxelObjectPlaybackCommand::Pause => self
                .require_target_mut(scene_id, instance_id)?
                .player
                .pause(now_microseconds)
                .map_err(player_rejection)?,
            VoxelObjectPlaybackCommand::Sample => {
                self.ensure_target(scene_id, instance_id)?;
            }
            VoxelObjectPlaybackCommand::Stop => {
                self.ensure_target(scene_id, instance_id)?;
                self.clear();
            }
        }

        let (readout, runtime_frame) = if let Some(session) = self.session.as_mut() {
            let should_finish_once = {
                let sample = session
                    .player
                    .sample_at(&object, now_microseconds)
                    .map_err(player_rejection)?;
                sample.status == VoxelObjectPlaybackStatus::Playing
                    && sample.loop_mode == VoxelObjectLoopMode::Once
                    && sample.ended
            };
            if should_finish_once {
                session
                    .player
                    .pause(now_microseconds)
                    .map_err(player_rejection)?;
            }
            let sample = session
                .player
                .sample_at(&object, now_microseconds)
                .map_err(player_rejection)?;
            (
                VoxelObjectInstancePlaybackReadout {
                    scene_id: session.scene_id.clone(),
                    instance_id: session.instance_id.clone(),
                    voxel_object_asset_id: session.voxel_object_asset_id.clone(),
                    project_hash: project.source_hash().to_hex(),
                    object_content_hash: object.content_hash().to_string(),
                    durable_frame: instance.frame.clone(),
                    status: playback_status(sample.status),
                    clip_id: sample.clip.map(str::to_owned),
                    loop_mode: sample.loop_mode,
                    rate: sample.rate,
                    elapsed_microseconds: sample.elapsed_micros,
                    runtime_frame: sample.frame,
                    clip_frame: sample.clip_frame,
                    ended: sample.ended,
                },
                sample.frame,
            )
        } else {
            (
                VoxelObjectInstancePlaybackReadout {
                    scene_id: scene_id.to_owned(),
                    instance_id: instance_id.to_owned(),
                    voxel_object_asset_id: instance.voxel_object_asset_id.clone(),
                    project_hash: project.source_hash().to_hex(),
                    object_content_hash: object.content_hash().to_string(),
                    durable_frame: instance.frame.clone(),
                    status: "stopped",
                    clip_id: None,
                    loop_mode: VoxelObjectLoopMode::Once,
                    rate: VoxelObjectPlaybackRate::NORMAL,
                    elapsed_microseconds: 0,
                    runtime_frame: durable_runtime_frame,
                    clip_frame: None,
                    ended: false,
                },
                durable_runtime_frame,
            )
        };
        let (projection, projection_readout) =
            project.project_voxel_object_frame(projector, instance_id, runtime_frame)?;
        Ok(PlaybackPresentation {
            readout,
            projection,
            projection_readout,
        })
    }

    fn ensure_target(&self, scene_id: &str, instance_id: &str) -> Result<(), AdapterRejection> {
        let session = self.session.as_ref().ok_or_else(|| {
            reject(
                "voxelObject.playbackNotSelected",
                "scrub an applied voxel-object instance before controlling playback",
            )
        })?;
        if session.scene_id != scene_id || session.instance_id != instance_id {
            return Err(reject(
                "voxelObject.playbackTargetMismatch",
                format!(
                    "playback targets `{}/{}`, not `{scene_id}/{instance_id}`",
                    session.scene_id, session.instance_id
                ),
            ));
        }
        Ok(())
    }

    fn require_target_mut(
        &mut self,
        scene_id: &str,
        instance_id: &str,
    ) -> Result<&mut PlaybackSession, AdapterRejection> {
        self.ensure_target(scene_id, instance_id)?;
        Ok(self.session.as_mut().expect("playback target was checked"))
    }
}

fn playback_target(
    project: &OpenedOwnerProject,
    scene_id: &str,
    instance_id: &str,
) -> Result<(StoredVoxelObjectInstance, AdmittedVoxelObject), AdapterRejection> {
    let scene = project
        .document()
        .scenes
        .iter()
        .find(|scene| scene.id == scene_id)
        .ok_or_else(|| {
            reject(
                "project.missingScene",
                format!("project has no scene `{scene_id}`"),
            )
        })?;
    let instance = scene
        .voxel_object_instances
        .iter()
        .find(|instance| instance.instance_id == instance_id)
        .cloned()
        .ok_or_else(|| {
            reject(
                "voxelObject.instanceMissing",
                format!("scene `{scene_id}` has no voxel-object instance `{instance_id}`"),
            )
        })?;
    let source = project
        .document()
        .assets
        .iter()
        .find(|asset| asset.id == instance.voxel_object_asset_id)
        .and_then(|asset| asset.voxel_object.as_ref())
        .ok_or_else(|| {
            reject(
                "voxelObject.assetMissing",
                format!(
                    "voxel-object asset `{}` is unavailable",
                    instance.voxel_object_asset_id
                ),
            )
        })?;
    let object = admit_voxel_object(source, VoxelObjectRuntimeLimits::default())
        .map_err(|error| reject("voxelObject.admissionRejected", error.to_string()))?;
    Ok((instance, object))
}

fn resolve_frame(
    object: &AdmittedVoxelObject,
    selection: &StoredVoxelObjectFrameSelection,
) -> Result<u32, AdapterRejection> {
    match selection {
        StoredVoxelObjectFrameSelection::Default => Ok(0),
        StoredVoxelObjectFrameSelection::Clip {
            clip_id,
            frame_index,
        } => object
            .clip(clip_id)
            .and_then(|clip| clip.frame_indices.get(*frame_index as usize))
            .copied()
            .ok_or_else(|| {
                reject(
                    "voxelObject.frameRejected",
                    format!("clip `{clip_id}` has no frame {frame_index}"),
                )
            }),
    }
}

fn playback_status(status: VoxelObjectPlaybackStatus) -> &'static str {
    match status {
        VoxelObjectPlaybackStatus::Stopped => "stopped",
        VoxelObjectPlaybackStatus::Playing => "playing",
        VoxelObjectPlaybackStatus::Paused => "paused",
    }
}

fn player_rejection(error: impl std::fmt::Display) -> AdapterRejection {
    reject("voxelObject.playbackRejected", error.to_string())
}

fn reject(code: impl Into<String>, message: impl Into<String>) -> AdapterRejection {
    AdapterRejection::new(code, message)
}
