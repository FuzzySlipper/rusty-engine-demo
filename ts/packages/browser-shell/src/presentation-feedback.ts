import {
  billboardHandle,
  telemetryOverlayHandle,
  type BillboardHandle,
  type PresentationFrameDiff,
  type PresentationOp,
} from "@rusty-engine/render-contracts";
import {
  RendererAudioHost,
  RendererBillboardHost,
  RendererDomParticleBillboardSink,
  RendererDomTelemetryOverlaySink,
  RendererLiveTelemetryCollector,
  RendererParticleHost,
  RendererPresentationHostSet,
  RendererTelemetryOverlayHost,
  type RendererSurface,
  type RendererSurfaceTelemetrySample,
  type RendererSurfaceTimingSample,
} from "@rusty-engine/renderer-host";

import type {
  RuntimeAnimationState,
  RuntimeBrowserState,
  RuntimeFeedbackCue,
} from "./projection.js";
import type {
  WeaponViewmodelAdapter,
  WeaponViewmodelPlan,
} from "./weapon-viewmodel.js";

export type FeedbackParticleKind =
  | "movement"
  | "blocked"
  | "dry"
  | "muzzle"
  | "impact"
  | "defeat"
  | "door"
  | "beacon"
  | "pickup";
export type FeedbackSoundKind =
  | "step"
  | "blocked"
  | "shot"
  | "sidearmShot"
  | "spreadShot"
  | "automaticShot"
  | "dryFire"
  | "hit"
  | "defeat"
  | "doorOpen"
  | "doorClose"
  | "beacon"
  | "pickup";

export interface FeedbackAnchor {
  readonly entity: number;
  readonly position: readonly [number, number, number];
}

export interface FeedbackApplicationReceipt {
  readonly cueCount: number;
  readonly failedOperations: number;
  readonly scheduledSounds: number;
  readonly viewmodelOperations: number;
}

export interface ProjectedPresentationFeedback {
  readonly animationPulses: readonly string[];
  readonly animationStates: readonly RuntimeAnimationState[];
  readonly billboardHandles: readonly BillboardHandle[];
  readonly billboardValues: readonly string[];
  readonly cueCount: number;
  readonly frame: PresentationFrameDiff;
  readonly particleKinds: readonly FeedbackParticleKind[];
  readonly soundKinds: readonly FeedbackSoundKind[];
}

export function captureRendererTelemetry(
  surface: Pick<RendererSurface, "timing">,
  state: RuntimeBrowserState,
  renderDiffCount: number,
): RendererSurfaceTelemetrySample {
  return {
    sourceTick: state.tick,
    timing: surface.timing(),
    counters: {
      entityCount: state.projection.length,
      residentChunkCount: state.voxelMeshes.length,
      renderDiffCount,
    },
  };
}

interface CueFeedback {
  readonly particle: FeedbackParticleKind;
  readonly pulse: string;
  readonly sound: FeedbackSoundKind;
  readonly billboard?: {
    readonly text: string;
    readonly tone: "neutral" | "warning" | "success";
  };
}

/**
 * Game-specific semantic mapping only. Shared Rusty Engine hosts own every
 * browser renderer, resource, simulation, and cleanup mechanism.
 */
export class PresentationFeedbackAdapter {
  #generation = 0;
  #nextBillboardHandle = 1;
  #telemetryCreated = false;

  reset(): void {
    this.#generation = 0;
    this.#nextBillboardHandle = 1;
    this.#telemetryCreated = false;
  }

  project(
    state: RuntimeBrowserState,
    audioLevel = 1,
    flashIntensity = 1,
  ): ProjectedPresentationFeedback {
    this.#generation += 1;
    const operations: PresentationOp[] = [];
    const animationPulses: string[] = [];
    const billboardHandles: BillboardHandle[] = [];
    const billboardValues: string[] = [];
    const particleKinds: FeedbackParticleKind[] = [];
    const soundKinds: FeedbackSoundKind[] = [];

    if (!this.#telemetryCreated) {
      operations.push(telemetryCreate(operations.length));
      this.#telemetryCreated = true;
    }

    state.presentation.cues.forEach((cue, index) => {
      const anchor = cueAnchor(state, cue);
      const feedback = cueFeedback(state, cue);
      const signalStem = [
        "loading-bay",
        String(this.#generation),
        String(state.tick),
        String(index),
        cue.kind,
      ].join(":");
      animationPulses.push(feedback.pulse);
      particleKinds.push(feedback.particle);
      soundKinds.push(feedback.sound);
      operations.push(
        particleEmit(
          operations.length,
          `${signalStem}:particle`,
          feedback.particle,
          anchor,
          state.tick + this.#generation + index,
          flashIntensity,
        ),
      );
      operations.push(
        audioEmit(
          operations.length,
          `${signalStem}:audio`,
          feedback.sound,
          anchor,
          audioLevel,
        ),
      );
      if (feedback.billboard !== undefined) {
        const handle = billboardHandle(this.#nextBillboardHandle);
        this.#nextBillboardHandle += 1;
        billboardHandles.push(handle);
        billboardValues.push(feedback.billboard.text);
        operations.push(
          billboardCreate(
            operations.length,
            handle,
            feedback.billboard.text,
            feedback.billboard.tone,
            anchor,
          ),
        );
      }
    });

    return {
      animationPulses,
      animationStates: state.presentation.animationStates,
      billboardHandles,
      billboardValues,
      cueCount: state.presentation.cues.length,
      frame: { schemaVersion: 1, ops: operations },
      particleKinds,
      soundKinds,
    };
  }
}

export interface BrowserPresentationFeedbackOptions {
  readonly audioStatus: HTMLElement;
  readonly layer: HTMLElement;
  readonly readState: () => RuntimeBrowserState;
  readonly surface: RendererSurface;
  readonly telemetryLayer: HTMLElement;
  readonly viewmodel: WeaponViewmodelAdapter;
}

interface SharedPresentationHosts {
  readonly audio: RendererAudioHost;
  readonly billboard: RendererBillboardHost;
  readonly particle: RendererParticleHost;
  readonly particleSink: RendererDomParticleBillboardSink;
  readonly set: RendererPresentationHostSet;
  readonly telemetry: RendererTelemetryOverlayHost;
  readonly telemetrySink: RendererDomTelemetryOverlaySink;
}

/** Browser composition over the shared renderer-host package. */
export class BrowserPresentationFeedback {
  readonly #adapter = new PresentationFeedbackAdapter();
  readonly #viewmodel: WeaponViewmodelAdapter;
  readonly #audioStatus: HTMLElement;
  readonly #layer: HTMLElement;
  readonly #readState: () => RuntimeBrowserState;
  readonly #surface: RendererSurface;
  readonly #telemetryLayer: HTMLElement;
  readonly #pulseTargets = new Set<HTMLElement>();
  readonly #timeouts = new Set<ReturnType<typeof globalThis.setTimeout>>();
  #tail: Promise<void> = Promise.resolve();
  #hosts: SharedPresentationHosts;
  #activeSoundWindows = 0;
  #soundAttempts = 0;
  #scheduledSounds = 0;
  #audioLevel = 1;
  #flashIntensity = 1;
  #viewmodelImpulseGeneration = 0;
  #disposed = false;

  constructor(options: BrowserPresentationFeedbackOptions) {
    this.#audioStatus = options.audioStatus;
    this.#layer = options.layer;
    this.#readState = options.readState;
    this.#surface = options.surface;
    this.#telemetryLayer = options.telemetryLayer;
    this.#viewmodel = options.viewmodel;
    this.#hosts = this.#createHosts();
    this.#surface.setPresentationHosts(this.#hosts.set);
    this.#setAudioStatus("inactive");
    this.#audioStatus.dataset.activeSounds = "0";
    this.setAudioLevel(1);
    this.#layer.dataset.activeEffects = "0";
    this.#layer.dataset.pendingTimers = "0";
    this.#layer.dataset.maxActiveEffects = String(MAX_ACTIVE_EFFECTS);
    this.#layer.dataset.sharedRendererHosts =
      "audio,billboard,particle,telemetry";
    this.#layer.dataset.viewmodelOwner = "shared-renderer";
  }

  async activateAudio(): Promise<"running" | "blocked" | "unavailable"> {
    try {
      const diagnostics = await this.#hosts.audio.resume();
      const status = diagnostics.length === 0 ? "running" : "blocked";
      this.#setAudioStatus(status);
      return status;
    } catch {
      this.#setAudioStatus("unavailable");
      return "unavailable";
    }
  }

  setAudioLevel(level: number): void {
    this.#audioLevel = Number.isFinite(level)
      ? Math.min(1, Math.max(0, level))
      : 1;
    this.#audioStatus.dataset.volume = this.#audioLevel.toFixed(2);
  }

  setFlashIntensity(level: number): void {
    this.#flashIntensity = Number.isFinite(level)
      ? Math.min(1, Math.max(0, level))
      : 1;
    this.#layer.dataset.flashIntensity = this.#flashIntensity.toFixed(2);
  }

  apply(
    state: RuntimeBrowserState,
    reset = false,
    renderDiffCount = 0,
  ): Promise<FeedbackApplicationReceipt> {
    const cueCount = state.presentation.cues.length;
    if (this.#disposed) {
      return Promise.resolve({
        cueCount,
        failedOperations: 0,
        scheduledSounds: 0,
        viewmodelOperations: 0,
      });
    }
    const attempt = this.#tail
      .then(() => this.#applyNow(state, reset, renderDiffCount))
      .catch(() => ({
        cueCount,
        failedOperations: 1,
        scheduledSounds: 0,
        viewmodelOperations: 0,
      }));
    this.#tail = attempt.then(() => undefined);
    return attempt;
  }

  settled(): Promise<void> {
    return this.#tail;
  }

  async #applyNow(
    state: RuntimeBrowserState,
    reset: boolean,
    renderDiffCount: number,
  ): Promise<FeedbackApplicationReceipt> {
    if (this.#disposed) {
      return {
        cueCount: state.presentation.cues.length,
        failedOperations: 0,
        scheduledSounds: 0,
        viewmodelOperations: 0,
      };
    }
    if (reset) {
      await this.#resetTransient();
    }
    const viewmodel = this.#applyViewmodel(
      this.#viewmodel.project(state, reset, this.#flashIntensity),
    );
    const impulse = state.presentation.cues.some(
      (cue) =>
        (cue.kind === "attack" || cue.kind === "dryFire") &&
        cue.attacker === state.player.id,
    );
    if (viewmodel.failedOperations === 0 && impulse) {
      this.#scheduleViewmodelImpulseClear();
    }
    const projected = this.#adapter.project(
      state,
      this.#audioLevel,
      this.#flashIntensity,
    );
    projected.animationStates.forEach((animation) =>
      this.#setAnimationState(animation),
    );

    this.#soundAttempts += projected.soundKinds.length;
    this.#audioStatus.dataset.attempted = String(this.#soundAttempts);
    const receipt = await this.#surface.applyPresentation(projected.frame);
    state.presentation.cues.forEach((cue, index) => {
      this.#pulseAnimation(
        cueEntity(cue),
        projected.animationPulses[index] ?? cue.kind,
      );
    });
    const scheduledSounds =
      receipt.domains.find((domain) => domain.domain === "audio")?.applied ?? 0;
    this.#scheduledSounds += scheduledSounds;
    this.#audioStatus.dataset.scheduled = String(this.#scheduledSounds);
    projected.soundKinds.forEach((kind) => this.#recordAudioKind(kind));
    if (scheduledSounds > 0) {
      this.#activeSoundWindows += scheduledSounds;
      this.#audioStatus.dataset.activeSounds = String(this.#activeSoundWindows);
      this.#schedule(() => {
        this.#activeSoundWindows = Math.max(
          0,
          this.#activeSoundWindows - scheduledSounds,
        );
        this.#audioStatus.dataset.activeSounds = String(
          this.#activeSoundWindows,
        );
      }, ACTIVE_SOUND_EVIDENCE_MILLISECONDS);
    }

    projected.billboardHandles.forEach((handle) =>
      this.#scheduleBillboardDestroy(handle),
    );
    projected.particleKinds.forEach((kind) =>
      this.#record("particleKinds", kind),
    );
    projected.billboardValues.forEach((value) =>
      this.#record("billboardValues", value),
    );
    const telemetry = captureRendererTelemetry(
      this.#surface,
      state,
      renderDiffCount + viewmodel.appliedOperations,
    );
    const snapshot = this.#hosts.telemetry.sampleSurface(
      telemetry,
      telemetry.timing.sourceTimeMs,
    );
    this.#recordTelemetryEvidence(telemetry.timing, snapshot);
    this.#updateActiveEffects();
    this.#layer.dataset.lastCueCount = String(projected.cueCount);
    this.#layer.dataset.sharedPresentationApplied = String(receipt.applied);
    this.#layer.dataset.sharedPresentationDiagnostics = String(
      receipt.diagnostics.length,
    );
    return {
      cueCount: projected.cueCount,
      failedOperations: receipt.diagnostics.length + viewmodel.failedOperations,
      scheduledSounds,
      viewmodelOperations: viewmodel.appliedOperations,
    };
  }

  async dispose(): Promise<void> {
    if (this.#disposed) {
      return;
    }
    this.#viewmodelImpulseGeneration += 1;
    this.#applyViewmodel(this.#viewmodel.destroy());
    this.#disposed = true;
    this.#surface.setPresentationHosts(null);
    this.#clearTimersAndPulses();
    this.#hosts.billboard.dispose();
    this.#hosts.particle.dispose();
    this.#hosts.particleSink.dispose();
    this.#hosts.telemetry.cleanup();
    this.#hosts.telemetrySink.dispose();
    await this.#hosts.audio.dispose();
    this.#updateActiveEffects();
  }

  async #resetTransient(): Promise<void> {
    this.#viewmodelImpulseGeneration += 1;
    this.#surface.setPresentationHosts(null);
    this.#clearTimersAndPulses();
    this.#hosts.billboard.dispose();
    this.#hosts.particle.dispose();
    this.#hosts.particleSink.dispose();
    this.#hosts.telemetry.cleanup();
    this.#hosts.telemetrySink.dispose();
    // Audio graph disposal is synchronous inside the shared host; browser
    // AudioContext.close() may settle only after virtual/headless time moves.
    // Start it, retain the cleanup, and do not stall authoritative reset.
    void this.#hosts.audio.dispose().catch(() => undefined);
    this.#adapter.reset();
    this.#clearTelemetryEvidence();
    this.#hosts = this.#createHosts();
    this.#surface.setPresentationHosts(this.#hosts.set);
    this.#activeSoundWindows = 0;
    this.#audioStatus.dataset.activeSounds = "0";
    this.#setAudioStatus("inactive");
    delete this.#audioStatus.dataset.lastSound;
    delete this.#audioStatus.dataset.soundKinds;
    for (const key of [
      "animationStates",
      "cueKinds",
      "particleKinds",
      "billboardValues",
      "animationPulses",
      "viewmodelWeapons",
      "viewmodelImpulses",
    ] as const) {
      delete this.#layer.dataset[key];
    }
    this.#updateActiveEffects();
  }

  #applyViewmodel(plan: WeaponViewmodelPlan): {
    readonly appliedOperations: number;
    readonly failedOperations: number;
  } {
    if (plan.ops.length === 0) {
      plan.commit();
      this.#publishViewmodelEvidence(0);
      return { appliedOperations: 0, failedOperations: 0 };
    }
    try {
      const receipt = this.#surface.applyFrame(plan);
      if (!receipt.applied) {
        this.#layer.dataset.viewmodelStatus = "unavailable";
        this.#layer.dataset.viewmodelDiagnostics = receipt.diagnostics
          .map((diagnostic) => diagnostic.message)
          .join(";");
        return {
          appliedOperations: 0,
          failedOperations: Math.max(1, receipt.diagnostics.length),
        };
      }
      plan.commit();
      delete this.#layer.dataset.viewmodelDiagnostics;
      this.#publishViewmodelEvidence(plan.ops.length);
      return { appliedOperations: plan.ops.length, failedOperations: 0 };
    } catch (error) {
      this.#layer.dataset.viewmodelStatus = "unavailable";
      this.#layer.dataset.viewmodelDiagnostics =
        error instanceof Error ? error.message : String(error);
      return { appliedOperations: 0, failedOperations: 1 };
    }
  }

  #scheduleViewmodelImpulseClear(): void {
    this.#viewmodelImpulseGeneration += 1;
    const generation = this.#viewmodelImpulseGeneration;
    this.#schedule(() => {
      if (generation === this.#viewmodelImpulseGeneration) {
        this.#applyViewmodel(this.#viewmodel.clearImpulse());
      }
    }, VIEWMODEL_IMPULSE_LIFETIME_MILLISECONDS);
  }

  #publishViewmodelEvidence(appliedOperations: number): void {
    const readout = this.#viewmodel.readout();
    const retained = this.#surface
      .projectionSnapshot()
      .nodes.filter((node) => node.layer === "viewmodel");
    const coherent =
      retained.length === readout.liveNodeCount &&
      (readout.mounted || retained.length === 0);
    this.#layer.dataset.viewmodelStatus = coherent
      ? readout.visible
        ? "active"
        : readout.mounted
          ? "hidden"
          : "unmounted"
      : "unavailable";
    this.#layer.dataset.viewmodelWeapon = readout.weapon ?? "none";
    this.#layer.dataset.viewmodelImpulse = readout.impulse;
    this.#layer.dataset.viewmodelNodes = String(retained.length);
    this.#layer.dataset.viewmodelLastOperations = String(appliedOperations);
    this.#record("viewmodelWeapons", readout.weapon ?? "none");
    this.#record("viewmodelImpulses", readout.impulse);
    document.body.dataset.weaponViewmodel = coherent ? "pass" : "fail";
    document.body.dataset.weaponViewmodelLayer = retained.every(
      (node) => node.layer === "viewmodel",
    )
      ? "viewmodel"
      : "invalid";
    document.body.dataset.weaponViewmodelLifecycle = readout.mounted
      ? "mounted"
      : "disposed";
  }

  #createHosts(): SharedPresentationHosts {
    const resolveEntityPosition = (
      entity: number,
    ): readonly [number, number, number] | null =>
      entityPosition(this.#readState(), entity);
    const audio = new RendererAudioHost({
      resolveEntityPosition,
      resolveResource: async (clip) => {
        const resource = AUDIO_RESOURCES_BY_ASSET.get(clip.asset);
        if (resource === undefined) {
          throw new Error(`unknown loading-bay audio resource ${clip.asset}`);
        }
        return {
          bytes: resource.bytes.slice(0),
          contentHash: resource.contentHash,
        };
      },
    });
    const billboard = new RendererBillboardHost({
      container: this.#layer,
      createElement: () => {
        const element = document.createElement("strong");
        element.className = "feedback-billboard";
        return element;
      },
      projectWorld: (position) => ({
        ...this.#surface.projectWorldPoint(position),
        occluded: false,
      }),
      resolveEntityPosition,
    });
    const particleSink = new RendererDomParticleBillboardSink({
      container: this.#layer,
      createElement: () => {
        const element = document.createElement("div");
        element.className = "feedback-particle";
        return element;
      },
      pixelsPerWorldUnit: 22,
      projectWorld: this.#surface.projectWorldPoint,
    });
    const particle = new RendererParticleHost({
      maxActiveEmitters: MAX_ACTIVE_EFFECTS,
      maxParticles: MAX_ACTIVE_EFFECTS,
      resolveEntityPosition,
      resolveResource: async (sprite) =>
        sprite.asset === PARTICLE_ASSET
          ? { bytes: PARTICLE_BYTES.slice(0), url: PARTICLE_DATA_URL }
          : null,
      sink: particleSink,
    });
    const telemetryCollector = new RendererLiveTelemetryCollector({
      expectedCounters: [
        "entityCount",
        "residentChunkCount",
        "renderDiffCount",
      ],
      maxFrameTimeSamples: 20,
    });
    const telemetrySink = new RendererDomTelemetryOverlaySink({
      container: this.#telemetryLayer,
      createElement: () => {
        const element = document.createElement("pre");
        element.className = "shared-render-telemetry";
        return element;
      },
    });
    const telemetry = new RendererTelemetryOverlayHost({
      collector: telemetryCollector,
      sink: telemetrySink,
    });
    return {
      audio,
      billboard,
      particle,
      particleSink,
      telemetry,
      telemetrySink,
      set: new RendererPresentationHostSet({
        audio,
        billboard,
        particle,
        telemetryOverlay: telemetry,
      }),
    };
  }

  #recordTelemetryEvidence(
    timing: RendererSurfaceTimingSample,
    snapshot: ReturnType<RendererLiveTelemetryCollector["readSnapshot"]>,
  ): void {
    this.#telemetryLayer.dataset.rendererSampleSequence = String(
      snapshot.sampleSequence,
    );
    this.#telemetryLayer.dataset.rendererRenderSequence = String(
      timing.renderSequence,
    );
    this.#telemetryLayer.dataset.rendererTimingSource = timing.source;
    this.#telemetryLayer.dataset.rendererFrameIntervalStatus =
      timing.frameIntervalStatus;
    this.#telemetryLayer.dataset.rendererFrameIntervalMilliseconds =
      optionalMetric(timing.frameIntervalMs);
    this.#telemetryLayer.dataset.rendererBackendSubmissionStatus =
      timing.backendSubmissionDurationStatus;
    this.#telemetryLayer.dataset.rendererBackendSubmissionMilliseconds =
      optionalMetric(timing.backendSubmissionDurationMs);
    this.#telemetryLayer.dataset.rendererFrameHistoryMilliseconds =
      snapshot.frameTimeHistoryMs.map((value) => value.toFixed(3)).join(",");
    this.#telemetryLayer.dataset.rendererEntityCount = snapshotMetric(
      snapshot,
      "entityCount",
    );
    this.#telemetryLayer.dataset.rendererResidentChunkCount = snapshotMetric(
      snapshot,
      "residentChunkCount",
    );
    this.#telemetryLayer.dataset.rendererRenderDiffCount = snapshotMetric(
      snapshot,
      "renderDiffCount",
    );
  }

  #clearTelemetryEvidence(): void {
    for (const key of [
      "rendererSampleSequence",
      "rendererRenderSequence",
      "rendererTimingSource",
      "rendererFrameIntervalStatus",
      "rendererFrameIntervalMilliseconds",
      "rendererBackendSubmissionStatus",
      "rendererBackendSubmissionMilliseconds",
      "rendererFrameHistoryMilliseconds",
      "rendererEntityCount",
      "rendererResidentChunkCount",
      "rendererRenderDiffCount",
    ] as const) {
      delete this.#telemetryLayer.dataset[key];
    }
  }

  #setAnimationState(state: RuntimeAnimationState): void {
    const entity = document.querySelector<HTMLElement>(
      `[data-entity-id="${String(state.entity)}"]`,
    );
    if (entity !== null) entity.dataset.posture = state.posture;
    this.#record("animationStates", `${String(state.entity)}:${state.posture}`);
  }

  #pulseAnimation(entity: number, name: string): void {
    const target = document.querySelector<HTMLElement>(
      `[data-entity-id="${String(entity)}"]`,
    );
    if (target !== null) {
      target.dataset.animationPulse = name;
      this.#pulseTargets.add(target);
      this.#schedule(() => {
        if (target.dataset.animationPulse === name) {
          delete target.dataset.animationPulse;
          this.#pulseTargets.delete(target);
        }
      }, PULSE_LIFETIME_MILLISECONDS);
    }
    this.#record("animationPulses", name);
    this.#record("cueKinds", name);
  }

  #scheduleBillboardDestroy(handle: BillboardHandle): void {
    this.#schedule(() => {
      void this.#surface
        .applyPresentation({
          schemaVersion: 1,
          ops: [
            {
              domain: "billboard",
              meta: { sequence: 0 },
              op: { op: "destroy", handle },
            },
          ],
        })
        .then(() => this.#updateActiveEffects());
    }, BILLBOARD_LIFETIME_MILLISECONDS);
  }

  #recordAudioKind(kind: FeedbackSoundKind): void {
    this.#audioStatus.dataset.lastSound = kind;
    const kinds = new Set(
      (this.#audioStatus.dataset.soundKinds ?? "").split(",").filter(Boolean),
    );
    kinds.add(kind);
    this.#audioStatus.dataset.soundKinds = [...kinds].join(",");
  }

  #record(
    field:
      | "animationStates"
      | "animationPulses"
      | "cueKinds"
      | "particleKinds"
      | "billboardValues"
      | "viewmodelWeapons"
      | "viewmodelImpulses",
    value: string,
  ): void {
    const values = new Set(
      (this.#layer.dataset[field] ?? "").split(",").filter(Boolean),
    );
    values.add(value);
    this.#layer.dataset[field] = [...values].join(",");
  }

  #schedule(operation: () => void, delay: number): void {
    const timeout = globalThis.setTimeout(() => {
      this.#timeouts.delete(timeout);
      this.#layer.dataset.pendingTimers = String(this.#timeouts.size);
      operation();
    }, delay);
    this.#timeouts.add(timeout);
    this.#layer.dataset.pendingTimers = String(this.#timeouts.size);
  }

  #clearTimersAndPulses(): void {
    for (const timeout of this.#timeouts) globalThis.clearTimeout(timeout);
    this.#timeouts.clear();
    this.#layer.dataset.pendingTimers = "0";
    for (const target of this.#pulseTargets)
      delete target.dataset.animationPulse;
    this.#pulseTargets.clear();
  }

  #updateActiveEffects(): void {
    const active =
      this.#hosts.particleSink.activeCount +
      this.#hosts.billboard.readout().activeBillboards;
    this.#layer.dataset.activeEffects = String(active);
  }

  #setAudioStatus(
    status: "inactive" | "running" | "blocked" | "unavailable",
  ): void {
    this.#audioStatus.dataset.state = status;
    this.#audioStatus.textContent =
      status === "running"
        ? "AUDIO ARMED"
        : status === "inactive"
          ? "AUDIO WAITING"
          : status === "unavailable"
            ? "AUDIO UNAVAILABLE"
            : "AUDIO BLOCKED";
  }
}

function cueFeedback(
  state: RuntimeBrowserState,
  cue: RuntimeFeedbackCue,
): CueFeedback {
  switch (cue.kind) {
    case "movement":
      return { particle: "movement", pulse: "movement", sound: "step" };
    case "movementBlocked":
      return {
        particle: "blocked",
        pulse: "blocked",
        sound: "blocked",
        billboard: { text: "BLOCKED", tone: "warning" },
      };
    case "attack":
      return {
        particle: "muzzle",
        pulse: `${cue.presentation}-attack`,
        sound: weaponShotSound(cue.presentation),
      };
    case "dryFire":
      return {
        particle: "dry",
        pulse: `${cue.presentation}-dry`,
        sound: "dryFire",
        billboard: { text: "EMPTY", tone: "warning" },
      };
    case "attackHit":
      return {
        particle: "impact",
        pulse: "attack-hit",
        sound: "hit",
        billboard: { text: "HIT", tone: "success" },
      };
    case "attackMissed":
      return {
        particle: "blocked",
        pulse: `attack-miss-${cue.reason}`,
        sound: "blocked",
        billboard: { text: "MISS", tone: "neutral" },
      };
    case "damage":
      return cue.target === state.player.id
        ? {
            particle: "impact",
            pulse: "player-damage",
            sound: "hit",
            billboard: {
              text: `PLAYER -${String(cue.amount)}`,
              tone: "warning",
            },
          }
        : {
            particle: "impact",
            pulse: "enemy-hurt",
            sound: "hit",
            billboard: {
              text: `-${String(cue.amount)}`,
              tone: "warning",
            },
          };
    case "enemyAlert":
      return {
        particle: "blocked",
        pulse: `enemy-alert-${cue.cause}`,
        sound: "beacon",
        billboard: { text: "ENEMY ALERT", tone: "warning" },
      };
    case "enemyAttack":
      return {
        particle: "muzzle",
        pulse: `${cue.presentation}-attack`,
        sound: cue.attackKind === "melee" ? "hit" : "shot",
      };
    case "enemyAttackMissed":
      return {
        particle: "blocked",
        pulse: `enemy-miss-${cue.reason}`,
        sound: "blocked",
      };
    case "defeat":
      return cue.entity === state.player.id
        ? {
            particle: "defeat",
            pulse: "player-defeated",
            sound: "defeat",
            billboard: { text: "PLAYER DOWN", tone: "warning" },
          }
        : {
            particle: "defeat",
            pulse: "enemy-defeated",
            sound: "defeat",
            billboard: { text: "DEFEATED", tone: "neutral" },
          };
    case "enemyDropMaterialized":
      return {
        particle: "pickup",
        pulse: "drop-materialized",
        sound: "pickup",
        billboard: {
          text: `DROP +${String(cue.quantity)} ${cue.item}`,
          tone: "success",
        },
      };
    case "encounterActivated":
      return {
        particle: "beacon",
        pulse: "encounter-activated",
        sound: "beacon",
        billboard: { text: "ENCOUNTER ACTIVE", tone: "warning" },
      };
    case "doorChanged":
      return {
        particle: "door",
        pulse: cue.state,
        sound: cue.state === "open" ? "doorOpen" : "doorClose",
        billboard: {
          text: cue.state === "open" ? "EXIT OPEN" : "EXIT SEALED",
          tone: cue.state === "open" ? "success" : "neutral",
        },
      };
    case "switchActivated":
      return {
        particle: "door",
        pulse: "switch-activated",
        sound: "doorOpen",
        billboard: { text: "SWITCH ACTIVE", tone: "success" },
      };
    case "checkpoint":
      return {
        particle: "beacon",
        pulse: `checkpoint-${cue.action}`,
        sound: "beacon",
        billboard: {
          text:
            cue.action === "saved" ? "CHECKPOINT SAVED" : "CHECKPOINT RESTORED",
          tone: "success",
        },
      };
    case "extractionBeaconActivated":
      return {
        particle: "beacon",
        pulse: "active",
        sound: "beacon",
        billboard: { text: "EXTRACTION ONLINE", tone: "success" },
      };
    case "pickupCollected":
      return {
        particle: "pickup",
        pulse: "pickup",
        sound: "pickup",
        billboard: {
          text: `+${String(cue.quantity)} ${cue.item}`,
          tone: "success",
        },
      };
    case "doorAccessGranted":
      return {
        particle: "door",
        pulse: "access-granted",
        sound: "doorOpen",
        billboard: { text: "ACCESS GRANTED", tone: "success" },
      };
    case "doorAccessDenied":
      return {
        particle: "blocked",
        pulse: "access-denied",
        sound: "blocked",
        billboard: { text: cue.presentation, tone: "warning" },
      };
    case "secretDiscovered":
      return {
        particle: "pickup",
        pulse: "secret-discovered",
        sound: "pickup",
        billboard: { text: cue.presentation, tone: "success" },
      };
    case "levelCompleted":
      return {
        particle: "beacon",
        pulse: "level-completed",
        sound: "beacon",
        billboard: { text: cue.presentation, tone: "success" },
      };
  }
}

function cueAnchor(
  state: RuntimeBrowserState,
  cue: RuntimeFeedbackCue,
): FeedbackAnchor {
  switch (cue.kind) {
    case "movement":
      return { entity: cue.entity, position: cue.to };
    case "attack":
      return { entity: cue.attacker, position: cue.origin };
    case "dryFire":
    case "attackMissed":
      return entityAnchor(state, cue.attacker);
    case "attackHit":
      return entityAnchor(state, cue.target);
    case "movementBlocked":
      return entityAnchor(state, cue.entity);
    case "damage":
      return entityAnchor(state, cue.target);
    case "enemyAlert":
      return entityAnchor(state, cue.entity);
    case "enemyAttack":
      return { entity: cue.attacker, position: cue.origin };
    case "enemyAttackMissed":
      return entityAnchor(state, cue.target);
    case "defeat":
      return entityAnchor(state, cue.entity);
    case "enemyDropMaterialized":
      return { entity: cue.pickup, position: cue.position };
    case "encounterActivated":
      return entityAnchor(state, cue.entity);
    case "doorChanged":
      return entityAnchor(state, cue.entity);
    case "switchActivated":
      return entityAnchor(state, cue.entity);
    case "checkpoint":
      return entityAnchor(state, cue.player);
    case "extractionBeaconActivated":
      return entityAnchor(state, cue.entity);
    case "pickupCollected":
      return entityAnchor(state, cue.actor);
    case "doorAccessGranted":
    case "secretDiscovered":
    case "levelCompleted":
      return entityAnchor(state, cue.entity);
    case "doorAccessDenied":
      return entityAnchor(state, cue.entity);
  }
}

function cueEntity(cue: RuntimeFeedbackCue): number {
  switch (cue.kind) {
    case "movement":
    case "movementBlocked":
      return cue.entity;
    case "attack":
    case "dryFire":
    case "attackMissed":
      return cue.attacker;
    case "attackHit":
      return cue.target;
    case "damage":
      return cue.target;
    case "enemyAlert":
      return cue.entity;
    case "enemyAttack":
    case "enemyAttackMissed":
      return cue.attacker;
    case "defeat":
    case "encounterActivated":
    case "doorChanged":
    case "switchActivated":
    case "extractionBeaconActivated":
      return cue.entity;
    case "checkpoint":
      return cue.player;
    case "enemyDropMaterialized":
      return cue.pickup;
    case "pickupCollected":
      return cue.actor;
    case "doorAccessGranted":
    case "doorAccessDenied":
    case "secretDiscovered":
    case "levelCompleted":
      return cue.entity;
  }
}

function weaponShotSound(presentation: string): FeedbackSoundKind {
  switch (presentation) {
    case "arc-pistol":
      return "sidearmShot";
    case "breach-scattergun":
      return "spreadShot";
    case "rivet-carbine":
      return "automaticShot";
    default:
      return "shot";
  }
}

function entityAnchor(
  state: RuntimeBrowserState,
  entity: number,
): FeedbackAnchor {
  return { entity, position: entityPosition(state, entity) ?? [0, 0, 0] };
}

function entityPosition(
  state: RuntimeBrowserState,
  entity: number,
): readonly [number, number, number] | null {
  if (state.player.id === entity) return state.player.position;
  const enemy = state.enemies.find((candidate) => candidate.id === entity);
  if (enemy !== undefined) return enemy.position;
  return (
    state.projection.find((candidate) => candidate.id === entity)
      ?.translation ?? null
  );
}

function telemetryCreate(sequence: number): PresentationOp {
  return {
    domain: "telemetryOverlay",
    meta: { sequence },
    op: {
      op: "create",
      handle: telemetryOverlayHandle(1),
      descriptor: {
        title: "Shared renderer",
        corner: "bottomRight",
        refreshIntervalMs: 100,
        maxFrameTimeSamples: 20,
        visible: true,
      },
    },
  };
}

function particleEmit(
  sequence: number,
  signalId: string,
  kind: FeedbackParticleKind,
  anchor: FeedbackAnchor,
  seed: number,
  flashIntensity: number,
): PresentationOp {
  const color = PARTICLE_COLORS[kind];
  const intensity = Number.isFinite(flashIntensity)
    ? Math.min(1, Math.max(0, flashIntensity))
    : 1;
  return {
    domain: "particle",
    meta: { sequence },
    op: {
      op: "emit",
      signalId,
      descriptor: {
        anchor: { kind: "world", position: anchor.position },
        sprite: {
          asset: PARTICLE_ASSET,
          contentHash: PARTICLE_HASH,
          frameCount: 1,
        },
        ratePerSecond: 0,
        burstCount: 1,
        lifetimeSeconds: [0.45, 0.7],
        velocityMin: [-0.35, 0.25, -0.35],
        velocityMax: [0.35, 0.9, 0.35],
        acceleration: [0, -0.65, 0],
        sizeCurve: [
          { age: 0, value: 0.7 * intensity },
          { age: 1, value: 0.08 * intensity },
        ],
        colorCurve: [
          {
            age: 0,
            color: [color[0], color[1], color[2], color[3] * intensity],
          },
          { age: 1, color: [color[0], color[1], color[2], 0] },
        ],
        flipbookFramesPerSecond: 0,
        seed: Math.max(0, seed),
        maxParticles: 1,
        visible: intensity > 0,
      },
    },
  };
}

function audioEmit(
  sequence: number,
  signalId: string,
  kind: FeedbackSoundKind,
  anchor: FeedbackAnchor,
  audioLevel: number,
): PresentationOp {
  const resource = AUDIO_RESOURCES[kind];
  return {
    domain: "audio",
    meta: { sequence },
    op: {
      op: "emit",
      signalId,
      descriptor: {
        clip: { asset: resource.asset, contentHash: resource.contentHash },
        bus: "sfx",
        volume: resource.volume * Math.min(1, Math.max(0, audioLevel)),
        pitch: 1,
        looping: false,
        spatialBlend: 0.65,
        attenuation: 24,
        pan: 0,
        emitter: { kind: "world3d", position: anchor.position },
      },
    },
  };
}

function billboardCreate(
  sequence: number,
  handle: BillboardHandle,
  text: string,
  tone: "neutral" | "warning" | "success",
  anchor: FeedbackAnchor,
): PresentationOp {
  const colors = BILLBOARD_COLORS[tone];
  return {
    domain: "billboard",
    meta: { sequence },
    op: {
      op: "create",
      handle,
      descriptor: {
        anchor: { kind: "world", position: anchor.position },
        content: {
          kind: "text",
          localizationKey: `loading-bay.feedback.${text.toLowerCase().replaceAll(" ", "-")}`,
          fallbackText: text,
          arguments: [],
        },
        font: { kind: "system", family: "ui-monospace, monospace" },
        heightPixels: 14,
        color: colors.foreground,
        background: colors.background,
        maxDistance: 60,
        layer: "alwaysOnTop",
        visible: true,
      },
    },
  };
}

interface AudioResource {
  readonly asset: string;
  readonly bytes: ArrayBuffer;
  readonly contentHash: string;
  readonly volume: number;
}

const SOUND_PROFILES: Record<
  FeedbackSoundKind,
  {
    readonly duration: number;
    readonly frequency: number;
    readonly frequencyEnd: number;
    readonly volume: number;
  }
> = {
  step: { frequency: 95, frequencyEnd: 70, duration: 0.05, volume: 0.12 },
  blocked: { frequency: 120, frequencyEnd: 55, duration: 0.11, volume: 0.18 },
  shot: { frequency: 220, frequencyEnd: 48, duration: 0.13, volume: 0.2 },
  sidearmShot: {
    frequency: 260,
    frequencyEnd: 58,
    duration: 0.12,
    volume: 0.18,
  },
  spreadShot: {
    frequency: 150,
    frequencyEnd: 36,
    duration: 0.2,
    volume: 0.24,
  },
  automaticShot: {
    frequency: 340,
    frequencyEnd: 92,
    duration: 0.08,
    volume: 0.15,
  },
  dryFire: {
    frequency: 1_100,
    frequencyEnd: 680,
    duration: 0.045,
    volume: 0.11,
  },
  hit: { frequency: 440, frequencyEnd: 180, duration: 0.09, volume: 0.16 },
  defeat: { frequency: 180, frequencyEnd: 48, duration: 0.3, volume: 0.18 },
  doorOpen: { frequency: 150, frequencyEnd: 310, duration: 0.24, volume: 0.15 },
  doorClose: { frequency: 260, frequencyEnd: 90, duration: 0.2, volume: 0.15 },
  beacon: { frequency: 240, frequencyEnd: 720, duration: 0.32, volume: 0.15 },
  pickup: { frequency: 420, frequencyEnd: 760, duration: 0.16, volume: 0.13 },
};

const AUDIO_RESOURCES = createAudioResources();
const AUDIO_RESOURCES_BY_ASSET = new Map(
  Object.values(AUDIO_RESOURCES).map((resource) => [resource.asset, resource]),
);

function createAudioResources(): Record<FeedbackSoundKind, AudioResource> {
  return Object.fromEntries(
    Object.entries(SOUND_PROFILES).map(([kind, profile]) => {
      const bytes = toneWave(
        profile.frequency,
        profile.frequencyEnd,
        profile.duration,
      );
      return [
        kind,
        {
          asset: `audio/loading-bay/${kind}`,
          bytes,
          contentHash: fnv1a64(new Uint8Array(bytes)),
          volume: profile.volume,
        },
      ];
    }),
  ) as unknown as Record<FeedbackSoundKind, AudioResource>;
}

function toneWave(
  frequency: number,
  frequencyEnd: number,
  duration: number,
): ArrayBuffer {
  const sampleRate = 8_000;
  const sampleCount = Math.max(1, Math.round(sampleRate * duration));
  const bytes = new ArrayBuffer(44 + sampleCount * 2);
  const view = new DataView(bytes);
  writeAscii(view, 0, "RIFF");
  view.setUint32(4, bytes.byteLength - 8, true);
  writeAscii(view, 8, "WAVE");
  writeAscii(view, 12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  writeAscii(view, 36, "data");
  view.setUint32(40, sampleCount * 2, true);
  let phase = 0;
  for (let index = 0; index < sampleCount; index += 1) {
    const progress = index / sampleCount;
    const frequencyAtSample = frequency + (frequencyEnd - frequency) * progress;
    phase += (Math.PI * 2 * frequencyAtSample) / sampleRate;
    const sample = Math.sin(phase) * (1 - progress) * 0.5;
    view.setInt16(44 + index * 2, Math.round(sample * 0x7fff), true);
  }
  return bytes;
}

function writeAscii(view: DataView, offset: number, value: string): void {
  for (let index = 0; index < value.length; index += 1) {
    view.setUint8(offset + index, value.charCodeAt(index));
  }
}

function fnv1a64(bytes: Uint8Array): string {
  let hash = 0xcbf29ce484222325n;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = BigInt.asUintN(64, hash * 0x100000001b3n);
  }
  return hash.toString(16).padStart(16, "0");
}

function optionalMetric(value: number | null): string {
  return value === null ? "unavailable" : value.toFixed(3);
}

function snapshotMetric(
  snapshot: ReturnType<RendererLiveTelemetryCollector["readSnapshot"]>,
  counter: "entityCount" | "residentChunkCount" | "renderDiffCount",
): string {
  return String(
    snapshot.metrics.find((metric) => metric.counter === counter)?.value ??
      "unavailable",
  );
}

const PARTICLE_SOURCE = [
  '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16">',
  '<circle cx="8" cy="8" r="7" fill="white"/>',
  "</svg>",
].join("");
const PARTICLE_BYTES_VIEW = new TextEncoder().encode(PARTICLE_SOURCE);
const PARTICLE_BYTES = PARTICLE_BYTES_VIEW.buffer.slice(
  PARTICLE_BYTES_VIEW.byteOffset,
  PARTICLE_BYTES_VIEW.byteOffset + PARTICLE_BYTES_VIEW.byteLength,
) as ArrayBuffer;
const PARTICLE_ASSET = "sprite/loading-bay-feedback";
const PARTICLE_HASH = fnv1a64(new Uint8Array(PARTICLE_BYTES));
const PARTICLE_DATA_URL = `data:image/svg+xml,${encodeURIComponent(PARTICLE_SOURCE)}`;

const PARTICLE_COLORS: Record<
  FeedbackParticleKind,
  readonly [number, number, number, number]
> = {
  movement: [0.5, 0.78, 0.84, 0.9],
  blocked: [0.94, 0.68, 0.4, 1],
  dry: [0.72, 0.68, 0.58, 0.9],
  muzzle: [1, 0.94, 0.64, 1],
  impact: [1, 0.44, 0.37, 1],
  defeat: [0.91, 0.37, 0.32, 1],
  door: [0.36, 0.83, 0.65, 1],
  beacon: [0.4, 0.94, 0.76, 1],
  pickup: [0.95, 0.78, 0.3, 1],
};

const BILLBOARD_COLORS = {
  neutral: {
    foreground: [0.86, 0.92, 0.9, 1],
    background: [0.03, 0.06, 0.07, 0.88],
  },
  warning: {
    foreground: [1, 0.6, 0.52, 1],
    background: [0.18, 0.04, 0.03, 0.9],
  },
  success: {
    foreground: [0.47, 0.91, 0.74, 1],
    background: [0.02, 0.12, 0.09, 0.9],
  },
} as const;

const MAX_ACTIVE_EFFECTS = 24;
const BILLBOARD_LIFETIME_MILLISECONDS = 1_100;
const PULSE_LIFETIME_MILLISECONDS = 420;
const ACTIVE_SOUND_EVIDENCE_MILLISECONDS = 400;
const VIEWMODEL_IMPULSE_LIFETIME_MILLISECONDS = 90;
