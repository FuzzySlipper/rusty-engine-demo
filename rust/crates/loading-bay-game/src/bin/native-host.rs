use std::{
    collections::BTreeSet,
    env,
    io::{self, Write},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use loading_bay_game::{
    decode_game_snapshot, decode_project_document, doom_texture_projection, encode_game_snapshot,
    externalize_frame_meshes, project_stored_voxel_volume, GameLoopEdgeCommand,
    GameLoopEdgeCommandKind, GameRuntime, LoadingBayGameLoop, PlayerInputCommand,
    PlayerInputIntent, ResolvedPlayerAction, RuntimeError,
};
use rusty_engine::{
    core_ids::EntityId,
    engine_spatial::VoxelCollisionScene,
    render_host_contracts::{
        RendererCameraPose, RendererCameraProjection, RendererCompositionCamera,
        RendererCompositionTarget, RendererPhysicalInputReadout, RendererPickFilter,
        RendererPickRay, RendererPickRequest, RendererTargetColor, RendererTargetDepth,
        RendererTargetSampling, RendererViewComposition, RendererViewTarget, RendererViewport,
        RENDERER_VIEW_COMPOSITION_SCHEMA_VERSION,
    },
    render_model::{
        pack_mesh_resources, Geometry, Material, MaterialUvStrategy, MeshAttribute,
        MeshAttributeKind, MeshAttributeName, MeshBoundsDescriptor, MeshBufferLayout,
        MeshCollisionPolicy, MeshGroupDescriptor, MeshIndexWidth, MeshMaterialSlot,
        MeshPayloadDescriptor, MeshPayloadSource, MeshProvenance, PackedMeshResource, RenderDiff,
        RenderFrameDiff, RenderHandle, RenderLayer, RenderMaterialDescriptor, RenderMetadata,
        RenderNode, StaticMeshAsset, StaticMeshInstanceDescriptor, Transform,
        MAX_MESH_RESOURCE_BYTES,
    },
    renderer_webview_host::{
        RendererResource, RendererWebviewAdapter, RendererWebviewBounds,
        RendererWebviewObservation, RendererWebviewOptions,
    },
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const PLAYER: EntityId = EntityId::new(1);
const INTERLOCK: EntityId = EntityId::new(6);
const PRODUCT_GLB: &[u8] = include_bytes!("../../../../../content/assets/brush-kit/vent-panel.glb");
const MESH_ID: &str = "mesh/vent-panel";
const MATERIAL_ID: &str = "material/brush-kit/vent-panel";

#[derive(Debug, Clone, Copy)]
struct Options {
    proof: bool,
    corrupt_resource: bool,
    doom_e1m1: bool,
}

impl Options {
    fn parse() -> Result<Self> {
        let mut proof = false;
        let mut corrupt_resource = false;
        let mut doom_e1m1 = false;
        for argument in env::args().skip(1) {
            match argument.as_str() {
                "--proof" => proof = true,
                "--proof-corrupt-resource" => {
                    proof = true;
                    corrupt_resource = true;
                }
                "--proof-doom-e1m1" => {
                    proof = true;
                    doom_e1m1 = true;
                }
                "--doom-e1m1" => doom_e1m1 = true,
                _ => bail!("unknown argument {argument}"),
            }
        }
        Ok(Self {
            proof,
            corrupt_resource,
            doom_e1m1,
        })
    }
}

#[derive(Debug, Default)]
struct Proof {
    frame: bool,
    views: bool,
    camera: bool,
    resize: bool,
    resource_rendered: bool,
    input_authority: bool,
    input_noop: bool,
    pick_authority: bool,
    pick_miss: bool,
    state: bool,
    render: bool,
    save_round_trip: bool,
}

impl Proof {
    fn complete(&self) -> bool {
        self.frame
            && self.views
            && self.camera
            && self.resize
            && self.resource_rendered
            && self.input_authority
            && self.input_noop
            && self.pick_authority
            && self.pick_miss
            && self.state
            && self.render
            && self.save_round_trip
    }
}

#[derive(Debug, Clone, Copy)]
enum PickKind {
    Miss,
    Interlock,
}

#[derive(Debug, Clone, Copy)]
struct PendingPick {
    request_id: u64,
    kind: PickKind,
    revision_before: u64,
}

struct NativeApplication {
    options: Options,
    runtime: LoadingBayGameLoop,
    input_sequence: u64,
    last_loop_advance: Instant,
    mesh: StaticMeshAsset,
    resources: Vec<RendererResource>,
    doom_texture_frame: Option<RenderFrameDiff>,
    doom_frame: Option<RenderFrameDiff>,
    doom_scene_submitted: bool,
    doom_texture_request: Option<u64>,
    doom_scene_request: Option<u64>,
    doom_horizontal_surfaces: bool,
    doom_vertical_surfaces: bool,
    doom_texture_count: usize,
    window: Option<Window>,
    renderer: Option<RendererWebviewAdapter>,
    pressed_codes: BTreeSet<String>,
    pointer_buttons: u16,
    pending_input: Option<u64>,
    pending_pick: Option<PendingPick>,
    dispose_request: Option<u64>,
    next_input_poll: Instant,
    started_at: Instant,
    ready: bool,
    proof: Proof,
    failure: Option<String>,
}

impl NativeApplication {
    fn new(options: Options) -> Result<Self> {
        let project = if options.doom_e1m1 {
            include_str!("../../../../../content/projects/doom-e1m1.project.json")
        } else {
            include_str!("../../../../../content/projects/loading-bay.project.json")
        };
        let runtime = GameRuntime::from_stored_project(project)
            .context("admit the checked-in native product project")?;
        let (mesh, resource) = prepare_product_mesh(PRODUCT_GLB)?;
        let (
            resources,
            doom_texture_frame,
            doom_frame,
            doom_horizontal_surfaces,
            doom_vertical_surfaces,
            doom_texture_count,
        ) = if options.doom_e1m1 {
            let stored = decode_project_document(project)?.project;
            let admitted_scene = runtime
                .collision_scene()
                .context("Doom project has no admitted voxel environment")?;
            let route_scene = VoxelCollisionScene::from_material_voxels(
                admitted_scene.voxel_size(),
                admitted_scene.chunk_size(),
                admitted_scene.material_voxels().to_vec(),
            )
            .map_err(|error| anyhow::anyhow!("admit Doom rendered volume: {error:?}"))?;
            let doom_horizontal_surfaces = route_scene.mesh_chunks().iter().any(|chunk| {
                chunk
                    .normals
                    .chunks_exact(3)
                    .any(|normal| normal[1].abs() > 0.9)
            });
            let doom_vertical_surfaces = route_scene.mesh_chunks().iter().any(|chunk| {
                chunk
                    .normals
                    .chunks_exact(3)
                    .any(|normal| normal[0].abs() > 0.9 || normal[2].abs() > 0.9)
            });
            if !doom_horizontal_surfaces || !doom_vertical_surfaces {
                bail!("Doom route capture does not contain both horizontal and vertical surfaces");
            }
            let frame = project_stored_voxel_volume(&stored, &route_scene)?;
            let (frame, mut resources) = externalize_frame_meshes(frame)?;
            let (texture_resources, texture_ops) = doom_texture_projection(&stored)?;
            let doom_texture_count = texture_resources.len();
            if doom_texture_count != 54 {
                bail!("Doom native proof requires 54 textures, found {doom_texture_count}");
            }
            resources.extend(texture_resources);
            let texture_frame = RenderFrameDiff::try_from_ops(texture_ops)
                .map_err(|error| anyhow::anyhow!("build Doom texture frame: {error:?}"))?;
            (
                resources,
                Some(texture_frame),
                Some(frame),
                doom_horizontal_surfaces,
                doom_vertical_surfaces,
                doom_texture_count,
            )
        } else {
            (
                vec![RendererResource {
                    identity: resource.resource.clone(),
                    content_hash: resource.content_hash.clone(),
                    media_type: "application/vnd.rusty-engine.mesh-resource".to_owned(),
                    bytes: resource.bytes.clone(),
                }],
                None,
                None,
                false,
                false,
                0,
            )
        };
        let mut runtime = LoadingBayGameLoop::new(runtime, PLAYER)
            .context("create the native product game loop")?;
        runtime.start_connection();
        Ok(Self {
            options,
            runtime,
            input_sequence: 0,
            last_loop_advance: Instant::now(),
            mesh,
            resources,
            doom_texture_frame,
            doom_frame,
            doom_scene_submitted: false,
            doom_texture_request: None,
            doom_scene_request: None,
            doom_horizontal_surfaces,
            doom_vertical_surfaces,
            doom_texture_count,
            window: None,
            renderer: None,
            pressed_codes: BTreeSet::new(),
            pointer_buttons: 0,
            pending_input: None,
            pending_pick: None,
            dispose_request: None,
            next_input_poll: Instant::now(),
            started_at: Instant::now(),
            ready: false,
            proof: Proof::default(),
            failure: None,
        })
    }

    fn mount(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title(if self.options.doom_e1m1 {
                        "Doom E1M1 — Rust-native Engine renderer"
                    } else {
                        "Loading Bay — Rust-native Engine renderer"
                    })
                    .with_inner_size(winit::dpi::LogicalSize::new(960, 640)),
            )
            .context("create Loading Bay product window")?;
        let mut resources = self.resources.clone();
        if self.options.corrupt_resource {
            *resources[0]
                .bytes
                .last_mut()
                .context("packed product resource is empty")? ^= 0xff;
        }
        let renderer = RendererWebviewAdapter::mount(
            &window,
            RendererWebviewOptions {
                auto_start: true,
                bounds: window_bounds(&window),
                clear_color: Some(0x101820),
                pixel_ratio: window.scale_factor(),
                resources,
            },
        )
        .map_err(|error| anyhow::anyhow!("mount Engine-owned renderer: {error:?}"))?;
        self.window = Some(window);
        self.renderer = Some(renderer);
        Ok(())
    }

    fn initialize_renderer(&mut self) -> Result<()> {
        let camera_pose = if self.options.doom_e1m1 {
            self.doom_camera_pose()?
        } else {
            RendererCameraPose {
                position: [0.0, 6.0, 9.0],
                pitch_degrees: -25.0,
                yaw_degrees: 0.0,
            }
        };
        let renderer = self.renderer.as_mut().context("renderer unavailable")?;
        let frame = if self.options.doom_e1m1 {
            self.doom_texture_frame
                .as_ref()
                .or(self.doom_frame.as_ref())
                .context("Doom native frame is missing")?
                .clone()
        } else {
            product_frame(&self.mesh)?
        };
        let frame_request = renderer.submit_frame(&frame)?;
        if self.options.doom_e1m1 {
            if self.doom_texture_frame.is_some() {
                self.doom_texture_request = Some(frame_request);
            } else {
                self.doom_scene_request = Some(frame_request);
                self.doom_scene_submitted = true;
            }
        }
        renderer.configure_views(&product_views(1))?;
        renderer.set_camera_pose(camera_pose, None)?;
        renderer.read_state()?;
        renderer.render_once(None)?;
        let bounds = window_bounds(self.window.as_ref().context("window unavailable")?);
        renderer.resize(
            RendererWebviewBounds {
                width: bounds.width.saturating_sub(48).max(1),
                height: bounds.height.saturating_sub(32).max(1),
                ..bounds
            },
            self.window
                .as_ref()
                .context("window unavailable")?
                .scale_factor(),
        )?;
        if !self.options.doom_e1m1 {
            self.request_input()?;
        }
        Ok(())
    }

    fn request_input(&mut self) -> Result<()> {
        if self.pending_input.is_none() {
            self.pending_input = Some(
                self.renderer
                    .as_mut()
                    .context("renderer unavailable")?
                    .read_physical_input()?,
            );
        }
        Ok(())
    }

    fn apply_input(&mut self, input: &RendererPhysicalInputReadout) -> Result<()> {
        let pressed = input.pressed_codes.iter().cloned().collect::<BTreeSet<_>>();
        let revision_before = self.runtime.runtime().readout().entity_revision;
        if pressed.is_empty() && input.pointer.buttons == 0 {
            self.proof.input_noop |=
                self.runtime.runtime().readout().entity_revision == revision_before;
        }
        if !self.options.doom_e1m1
            && input.pointer.buttons & 1 != 0
            && self.pointer_buttons & 1 == 0
        {
            let before = self
                .runtime
                .runtime()
                .session()
                .player_controller(PLAYER)
                .context("project player controller is missing")?
                .state;
            self.runtime.runtime_mut().apply_player_action(
                PLAYER,
                ResolvedPlayerAction::Look {
                    yaw_delta: 0.25,
                    pitch_delta: 0.0,
                },
            )?;
            let after = self
                .runtime
                .runtime()
                .session()
                .player_controller(PLAYER)
                .context("project player controller disappeared")?
                .state;
            self.proof.input_authority = after != before;
            self.request_pick(PickKind::Miss)?;
        }
        if self.options.doom_e1m1 {
            let bindings = self
                .runtime
                .runtime()
                .session()
                .player_controller(PLAYER)
                .context("Doom player controller is missing")?
                .config
                .bindings
                .clone();
            let forward = f32::from(pressed.contains(&bindings.move_forward))
                - f32::from(pressed.contains(&bindings.move_backward));
            let right = f32::from(pressed.contains(&bindings.move_right))
                - f32::from(pressed.contains(&bindings.move_left));
            let yaw_delta = f32::from(pressed.contains("ArrowRight"))
                - f32::from(pressed.contains("ArrowLeft"));
            let pitch_delta =
                f32::from(pressed.contains("ArrowDown")) - f32::from(pressed.contains("ArrowUp"));
            self.input_sequence = self.input_sequence.saturating_add(1);
            self.runtime
                .submit_input(PlayerInputCommand {
                    connection_generation: self.runtime.input_session().connection_generation,
                    sequence: self.input_sequence,
                    intent: PlayerInputIntent {
                        movement: [forward, right],
                        look_delta: [yaw_delta * 0.12, pitch_delta * 0.12],
                        primary_fire_held: input.pointer.buttons & 1 != 0,
                    },
                })
                .map_err(|error| anyhow::anyhow!("submit native semantic input: {error}"))?;
            if bindings
                .jump
                .as_ref()
                .is_some_and(|jump| pressed.contains(jump) && !self.pressed_codes.contains(jump))
            {
                self.input_sequence = self.input_sequence.saturating_add(1);
                self.runtime
                    .submit_edge_command(GameLoopEdgeCommand {
                        connection_generation: self.runtime.input_session().connection_generation,
                        sequence: self.input_sequence,
                        command: GameLoopEdgeCommandKind::Jump,
                    })
                    .map_err(|error| anyhow::anyhow!("submit native jump input: {error}"))?;
            }
        } else if let Some(code) = pressed.difference(&self.pressed_codes).next() {
            let revision_before = self.runtime.runtime().readout().entity_revision;
            if code == "Enter" {
                let before = self
                    .runtime
                    .runtime()
                    .session()
                    .player_controller(PLAYER)
                    .context("project player controller is missing")?
                    .state;
                self.runtime.runtime_mut().apply_player_action(
                    PLAYER,
                    ResolvedPlayerAction::Look {
                        yaw_delta: 0.25,
                        pitch_delta: 0.0,
                    },
                )?;
                let after = self
                    .runtime
                    .runtime()
                    .session()
                    .player_controller(PLAYER)
                    .context("project player controller disappeared")?
                    .state;
                self.proof.input_authority = after != before;
                if self.pending_pick.is_none() && !self.proof.pick_miss {
                    self.request_pick(PickKind::Miss)?;
                }
            } else if code == "Escape" {
                self.proof.input_noop =
                    self.runtime.runtime().readout().entity_revision == revision_before;
            }
        }
        self.pressed_codes = pressed;
        self.pointer_buttons = input.pointer.buttons;
        Ok(())
    }

    fn sync_doom_camera(&mut self) -> Result<()> {
        let camera_pose = self.doom_camera_pose()?;
        self.renderer
            .as_mut()
            .context("renderer unavailable")?
            .set_camera_pose(camera_pose, None)?;
        Ok(())
    }

    fn advance_doom_game_loop(&mut self) -> Result<()> {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_loop_advance);
        self.last_loop_advance = now;
        if self.options.proof || !self.options.doom_e1m1 || !self.ready {
            return Ok(());
        }
        let receipt = self.runtime.advance_elapsed(elapsed)?;
        if !receipt.fixed_ticks.is_empty() {
            self.sync_doom_camera()?;
        }
        Ok(())
    }

    fn doom_camera_pose(&self) -> Result<RendererCameraPose> {
        let player = self
            .runtime
            .runtime()
            .session()
            .player_controller(PLAYER)
            .context("Doom player controller is missing")?;
        let position = player
            .entity_view
            .world_transform
            .context("Doom player world transform is missing")?
            .translation;
        Ok(RendererCameraPose {
            position: [
                f64::from(position.x),
                f64::from(position.y + player.config.traversal.eye_height),
                f64::from(position.z),
            ],
            pitch_degrees: f64::from(player.state.pitch_degrees),
            yaw_degrees: f64::from(player.state.yaw_degrees),
        })
    }

    fn request_pick(&mut self, kind: PickKind) -> Result<()> {
        let ray = match kind {
            PickKind::Miss => RendererPickRay::WorldRay {
                origin: [1000.0, 10.0, 1000.0],
                direction: [0.0, -1.0, 0.0],
            },
            PickKind::Interlock => RendererPickRay::WorldRay {
                origin: [0.0, 10.0, 0.0],
                direction: [0.0, -1.0, 0.0],
            },
        };
        let request_id = self
            .renderer
            .as_mut()
            .context("renderer unavailable")?
            .pick(&RendererPickRequest {
                filter: Some(RendererPickFilter {
                    layers: vec![RenderLayer::Scene],
                    tags: vec!["loading-bay-interlock".to_owned()],
                    ..RendererPickFilter::default()
                }),
                max_distance: Some(32.0),
                ray,
            })?;
        self.pending_pick = Some(PendingPick {
            request_id,
            kind,
            revision_before: self.runtime.runtime().readout().entity_revision,
        });
        Ok(())
    }

    fn apply_pick(
        &mut self,
        request_id: u64,
        receipt: rusty_engine::render_host_contracts::RendererPickReceipt,
    ) -> Result<()> {
        let pending = self
            .pending_pick
            .take()
            .context("unexpected pick receipt")?;
        if pending.request_id != request_id {
            bail!(
                "pick request mismatch: received {request_id}, expected {}",
                pending.request_id
            );
        }
        match pending.kind {
            PickKind::Miss => {
                if receipt.hint.is_some()
                    || self.runtime.runtime().readout().entity_revision != pending.revision_before
                {
                    bail!("miss pick changed Loading Bay authority");
                }
                self.proof.pick_miss = true;
                self.request_pick(PickKind::Interlock)?;
            }
            PickKind::Interlock => {
                let entity = receipt
                    .hint
                    .and_then(|hint| hint.source_trace)
                    .map(|trace| trace.entity)
                    .context("interlock pick returned no entity trace")?;
                if entity != INTERLOCK.raw() {
                    bail!("interlock pick returned entity {entity}");
                }
                let before = self
                    .runtime
                    .runtime()
                    .session()
                    .switch(INTERLOCK)
                    .context("authored generator interlock is missing")?
                    .activation_count;
                let interaction = self.runtime.runtime_mut().interact(PLAYER, INTERLOCK);
                let after = self
                    .runtime
                    .runtime()
                    .session()
                    .switch(INTERLOCK)
                    .context("authored generator interlock disappeared")?
                    .activation_count;
                self.proof.pick_authority = matches!(
                    interaction,
                    Err(RuntimeError::SwitchOutOfRange {
                        actor: PLAYER,
                        switch: INTERLOCK,
                        ..
                    })
                ) && after == before
                    && self.runtime.runtime().readout().entity_revision == pending.revision_before;
                let saved = encode_game_snapshot(self.runtime.runtime())?;
                let restored = decode_game_snapshot(&saved)?;
                self.proof.save_round_trip = restored
                    .session()
                    .player_controller(PLAYER)
                    .map(|view| view.state)
                    == self
                        .runtime
                        .runtime()
                        .session()
                        .player_controller(PLAYER)
                        .map(|view| view.state)
                    && restored
                        .session()
                        .switch(INTERLOCK)
                        .map(|view| view.activation_count)
                        == self
                            .runtime
                            .runtime()
                            .session()
                            .switch(INTERLOCK)
                            .map(|view| view.activation_count);
            }
        }
        Ok(())
    }

    fn handle_observation(
        &mut self,
        observation: RendererWebviewObservation,
        event_loop: &ActiveEventLoop,
    ) -> Result<()> {
        match observation {
            RendererWebviewObservation::Ready(_) => {
                if self.options.corrupt_resource {
                    bail!("corrupt resource unexpectedly reached ready state");
                }
                if self.options.proof {
                    println!(
                        "{}",
                        if self.options.doom_e1m1 {
                            "DOOM_E1M1_NATIVE_READY_FOR_CAPTURE"
                        } else {
                            "LOADING_BAY_NATIVE_READY_FOR_INPUT"
                        }
                    );
                    io::stdout().flush()?;
                }
                self.ready = true;
                self.initialize_renderer()?;
            }
            RendererWebviewObservation::FrameApplied {
                request_id,
                receipt,
            } => {
                if !receipt.applied {
                    bail!("renderer rejected product frame: {:?}", receipt.diagnostics);
                }
                if self.options.doom_e1m1
                    && self.doom_texture_request == Some(request_id)
                    && !self.doom_scene_submitted
                {
                    let scene_request = self
                        .renderer
                        .as_mut()
                        .context("renderer unavailable after Doom texture frame")?
                        .submit_frame(
                            self.doom_frame
                                .as_ref()
                                .context("Doom scene frame is missing")?,
                        )?;
                    self.doom_scene_request = Some(scene_request);
                    self.doom_scene_submitted = true;
                } else if !self.options.doom_e1m1 || self.doom_scene_request == Some(request_id) {
                    self.proof.frame = true;
                    self.proof.resource_rendered = true;
                }
            }
            RendererWebviewObservation::ViewsConfigured { receipt, .. } => {
                if !receipt.applied {
                    bail!("renderer rejected product views: {:?}", receipt.diagnostics);
                }
                self.proof.views = true;
            }
            RendererWebviewObservation::CameraUpdated { .. } => self.proof.camera = true,
            RendererWebviewObservation::PhysicalInputRead {
                request_id,
                readout,
            } if self.pending_input == Some(request_id) => {
                self.pending_input = None;
                self.apply_input(&readout)?;
            }
            RendererWebviewObservation::PickCompleted {
                request_id,
                receipt,
            } => {
                self.apply_pick(request_id, receipt)?;
            }
            RendererWebviewObservation::StateRead { .. } => self.proof.state = true,
            RendererWebviewObservation::FrameRendered { .. } => self.proof.render = true,
            RendererWebviewObservation::Resized { .. } => self.proof.resize = true,
            RendererWebviewObservation::Disposed { request_id }
                if self.dispose_request == Some(request_id) =>
            {
                if self.options.doom_e1m1 {
                    println!(
                        "DOOM_E1M1_NATIVE_PROOF_OK frame={} views={} camera={} resize={} resource_rendered={} input_authority={} input_noop={} pick_authority={} pick_miss={} state={} render={} save_round_trip={} textures={} horizontal_surfaces={} vertical_surfaces={} lifecycle=disposed",
                        self.proof.frame, self.proof.views, self.proof.camera, self.proof.resize,
                        self.proof.resource_rendered, self.proof.input_authority,
                        self.proof.input_noop, self.proof.pick_authority, self.proof.pick_miss,
                        self.proof.state, self.proof.render, self.proof.save_round_trip,
                        self.doom_texture_count, self.doom_horizontal_surfaces,
                        self.doom_vertical_surfaces,
                    );
                } else {
                    println!(
                        "LOADING_BAY_NATIVE_PROOF_OK frame={} views={} camera={} resize={} resource_rendered={} input_authority={} input_noop={} pick_authority={} pick_miss={} state={} render={} save_round_trip={} lifecycle=disposed",
                        self.proof.frame, self.proof.views, self.proof.camera, self.proof.resize,
                        self.proof.resource_rendered, self.proof.input_authority,
                        self.proof.input_noop, self.proof.pick_authority, self.proof.pick_miss,
                        self.proof.state, self.proof.render, self.proof.save_round_trip,
                    );
                }
                event_loop.exit();
            }
            RendererWebviewObservation::MountFailed { message } => {
                self.renderer = None;
                if self.options.corrupt_resource && message.contains("content hash mismatch") {
                    println!(
                        "LOADING_BAY_RESOURCE_REJECTION_OK lifecycle=transactional message={message}"
                    );
                    event_loop.exit();
                } else {
                    bail!("renderer mount failed transactionally: {message}");
                }
            }
            RendererWebviewObservation::OperationFailed {
                request_id,
                operation,
                message,
            } => bail!("renderer operation {operation:?} request {request_id} failed: {message}"),
            _ => {}
        }
        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.renderer = None;
        self.failure = Some(error.to_string());
        event_loop.exit();
    }
}

impl ApplicationHandler for NativeApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(error) = self.mount(event_loop) {
                if self.options.corrupt_resource
                    && error
                        .to_string()
                        .contains("resource bytes do not match the declared SHA-256 identity")
                {
                    println!(
                        "LOADING_BAY_RESOURCE_REJECTION_OK lifecycle=transactional message={error}"
                    );
                    event_loop.exit();
                } else {
                    self.fail(event_loop, error);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested) && self.dispose_request.is_none() {
            match self.renderer.as_mut().map(RendererWebviewAdapter::dispose) {
                Some(Ok(request_id)) => self.dispose_request = Some(request_id),
                Some(Err(error)) => self.fail(event_loop, error),
                None => event_loop.exit(),
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if self.options.proof && self.started_at.elapsed() > Duration::from_secs(30) {
            self.fail(
                event_loop,
                format!("native renderer proof timed out: {:?}", self.proof),
            );
            return;
        }
        let observations = self
            .renderer
            .as_mut()
            .map(RendererWebviewAdapter::drain_observations)
            .unwrap_or_default();
        for observation in observations {
            let result = observation
                .map_err(anyhow::Error::from)
                .and_then(|observation| self.handle_observation(observation, event_loop));
            if let Err(error) = result {
                self.fail(event_loop, error);
                return;
            }
        }
        if self.failure.is_some() || self.dispose_request.is_some() {
            return;
        }
        if let Err(error) = self.advance_doom_game_loop() {
            self.fail(event_loop, error);
            return;
        }
        if (!self.options.doom_e1m1 || !self.options.proof)
            && self.ready
            && self.renderer.is_some()
            && Instant::now() >= self.next_input_poll
        {
            if let Err(error) = self.request_input() {
                self.fail(event_loop, error);
                return;
            }
            self.next_input_poll = Instant::now() + Duration::from_millis(16);
        }
        let proof_complete = if self.options.doom_e1m1 {
            self.proof.frame
                && self.proof.views
                && self.proof.camera
                && self.proof.resize
                && self.proof.resource_rendered
                && self.proof.state
                && self.proof.render
                && self.started_at.elapsed() >= Duration::from_secs(4)
        } else {
            self.proof.complete()
        };
        if self.options.proof && proof_complete {
            match self.renderer.as_mut().map(RendererWebviewAdapter::dispose) {
                Some(Ok(request_id)) => self.dispose_request = Some(request_id),
                Some(Err(error)) => self.fail(event_loop, error),
                None => self.fail(event_loop, "renderer disappeared before disposal"),
            }
        }
    }
}

fn prepare_product_mesh(glb: &[u8]) -> Result<(StaticMeshAsset, PackedMeshResource)> {
    let imported = rusty_engine::voxel_convert::import_static_glb(glb)
        .map_err(|error| anyhow::anyhow!("import checked-in product GLB: {error:?}"))?;
    if imported.positions.is_empty() || imported.triangles.is_empty() {
        bail!("checked-in product GLB has no renderable triangles");
    }
    let positions = imported
        .positions
        .iter()
        .flat_map(|position| position.iter().map(|value| *value as f32))
        .collect::<Vec<_>>();
    let indices = imported
        .triangles
        .iter()
        .flat_map(|triangle| triangle.indices)
        .collect::<Vec<_>>();
    let payload = MeshPayloadDescriptor {
        layout: MeshBufferLayout {
            vertex_count: u32::try_from(imported.positions.len())?,
            index_count: u32::try_from(indices.len())?,
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
            count: u32::try_from(indices.len())?,
        }],
        bounds: mesh_bounds(&imported.positions)?,
        source: MeshPayloadSource::Inline {
            positions,
            normals: vertex_normals(&imported.positions, &indices),
            uvs: None,
            indices,
        },
        provenance: MeshProvenance::StaticAsset,
    };
    let packed = pack_mesh_resources(&[payload], MAX_MESH_RESOURCE_BYTES)
        .map_err(|error| anyhow::anyhow!("pack checked-in product GLB: {error:?}"))?;
    let resource = packed
        .resources
        .into_iter()
        .next()
        .context("missing packed resource")?;
    let mesh = StaticMeshAsset {
        asset: MESH_ID.to_owned(),
        payload: packed
            .payloads
            .into_iter()
            .next()
            .context("missing packed payload")?,
        material_slots: vec![MeshMaterialSlot {
            slot: 0,
            material: MATERIAL_ID.to_owned(),
        }],
        collision: MeshCollisionPolicy::VisualOnly,
    };
    mesh.validate()
        .map_err(|error| anyhow::anyhow!("validate checked-in product mesh: {error:?}"))?;
    Ok((mesh, resource))
}

fn product_frame(mesh: &StaticMeshAsset) -> Result<RenderFrameDiff> {
    let ops = vec![
        RenderDiff::DefineMaterial {
            material: RenderMaterialDescriptor {
                schema_version: 2,
                id: MATERIAL_ID.to_owned(),
                color: [0.2, 0.72, 0.55, 1.0],
                texture: None,
                roughness: 0.58,
                texture_tint: [1.0; 4],
                emission_color: [0.08, 0.45, 0.3],
                emission_intensity: 0.22,
                uv_strategy: MaterialUvStrategy::Flat,
                voxel_surface: None,
            },
        },
        RenderDiff::DefineStaticMesh {
            asset: mesh.clone(),
        },
        RenderDiff::Create {
            handle: RenderHandle::new(1),
            parent: None,
            node: RenderNode {
                geometry: Geometry::Cube,
                material: Material {
                    color: [0.16, 0.2, 0.24, 1.0],
                    wireframe: false,
                },
                transform: Transform {
                    translation: [0.0, -0.2, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [12.0, 0.4, 12.0],
                },
                visible: true,
                layer: RenderLayer::Scene,
                metadata: RenderMetadata {
                    source_entity: None,
                    source_scene_node: None,
                    tags: vec!["loading-bay-floor".to_owned()],
                    label: Some("Loading Bay floor".to_owned()),
                },
            },
        },
        RenderDiff::CreateStaticMeshInstance {
            handle: RenderHandle::new(5),
            parent: None,
            instance: StaticMeshInstanceDescriptor {
                asset: mesh.asset.clone(),
                transform: Transform {
                    translation: [-2.0, 1.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.0, 1.0, 1.0],
                },
                visible: true,
                material_overrides: Vec::new(),
                metadata: RenderMetadata {
                    source_entity: None,
                    source_scene_node: None,
                    tags: vec![
                        "checked-product-resource".to_owned(),
                        "loading-bay-prop".to_owned(),
                    ],
                    label: Some("Authored vent panel".to_owned()),
                },
            },
        },
        RenderDiff::Create {
            handle: RenderHandle::new(6),
            parent: None,
            node: RenderNode {
                geometry: Geometry::Cube,
                material: Material {
                    color: [0.85, 0.42, 0.16, 1.0],
                    wireframe: false,
                },
                transform: Transform {
                    translation: [0.0, 1.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    scale: [1.25, 1.25, 1.25],
                },
                visible: true,
                layer: RenderLayer::Scene,
                metadata: RenderMetadata {
                    source_entity: Some(INTERLOCK.raw()),
                    source_scene_node: Some(INTERLOCK.raw()),
                    tags: vec!["loading-bay-interlock".to_owned()],
                    label: Some("Generator interlock control".to_owned()),
                },
            },
        },
    ];
    RenderFrameDiff::try_from_ops(ops)
        .map_err(|error| anyhow::anyhow!("build Loading Bay product frame: {error:?}"))
}

fn product_views(target_revision: u64) -> RendererViewComposition {
    RendererViewComposition {
        schema_version: RENDERER_VIEW_COMPOSITION_SCHEMA_VERSION,
        cameras: vec![RendererCompositionCamera {
            id: "camera.loading-bay".to_owned(),
            pose: RendererCameraPose {
                position: [0.0, 6.0, 9.0],
                pitch_degrees: -25.0,
                yaw_degrees: 0.0,
            },
            projection: RendererCameraProjection::Perspective {
                fov_y_degrees: 55.0,
                near: 0.1,
                far: 100.0,
            },
        }],
        targets: vec![RendererCompositionTarget {
            id: "target.loading-bay".to_owned(),
            revision: target_revision,
            width: 256,
            height: 256,
            color: RendererTargetColor::Rgba8Srgb,
            depth: RendererTargetDepth::Depth24,
            sampling: RendererTargetSampling::Linear,
        }],
        views: vec![
            rusty_engine::render_host_contracts::RendererCompositionView {
                id: "view.loading-bay".to_owned(),
                camera_id: "camera.loading-bay".to_owned(),
                target: RendererViewTarget::Offscreen {
                    target_id: "target.loading-bay".to_owned(),
                    target_revision,
                },
                viewport: RendererViewport {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                order: 10,
            },
        ],
        presentations: Vec::new(),
    }
}

fn vertex_normals(positions: &[[f64; 3]], indices: &[u32]) -> Vec<f32> {
    let mut accumulated = vec![[0.0_f64; 3]; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [
            positions[triangle[0] as usize],
            positions[triangle[1] as usize],
            positions[triangle[2] as usize],
        ];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        for index in triangle {
            for axis in 0..3 {
                accumulated[*index as usize][axis] += normal[axis];
            }
        }
    }
    accumulated
        .into_iter()
        .flat_map(|normal| {
            let length = (normal[0].powi(2) + normal[1].powi(2) + normal[2].powi(2)).sqrt();
            if length > f64::EPSILON {
                normal.map(|value| (value / length) as f32)
            } else {
                [0.0, 1.0, 0.0]
            }
        })
        .collect()
}

fn mesh_bounds(positions: &[[f64; 3]]) -> Result<MeshBoundsDescriptor> {
    let first = positions.first().context("product mesh has no positions")?;
    let mut min = *first;
    let mut max = *first;
    for position in &positions[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    Ok(MeshBoundsDescriptor {
        min: min.map(|value| value as f32),
        max: max.map(|value| value as f32),
    })
}

fn window_bounds(window: &Window) -> RendererWebviewBounds {
    let size = window.inner_size();
    let scale = window.scale_factor();
    RendererWebviewBounds {
        x: 0,
        y: 0,
        width: ((f64::from(size.width) / scale).round() as u32).max(1),
        height: ((f64::from(size.height) / scale).round() as u32).max(1),
    }
}

fn main() -> Result<()> {
    #[cfg(target_os = "linux")]
    gtk::init().context("initialize GTK for native renderer host")?;
    let options = Options::parse()?;
    let event_loop = EventLoop::new().context("create Loading Bay event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut application = NativeApplication::new(options)?;
    event_loop
        .run_app(&mut application)
        .context("run Loading Bay native product")?;
    if let Some(failure) = application.failure {
        bail!(failure);
    }
    Ok(())
}
