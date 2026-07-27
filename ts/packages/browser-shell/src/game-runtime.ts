import { mountRendererSurface } from "@rusty-engine/renderer-host";

import { SerializedActionQueue } from "./action-queue.js";
import { CoalescedLookInput } from "./coalesced-look.js";
import {
  MAX_PRESENTATION_EVENT_HISTORY,
  MAX_PRESENTATION_EVENT_KINDS,
  appendPresentationEvents,
  observePresentationEventKinds,
} from "./event-history.js";
import { GameSessionError, LoadingBayGameSession } from "./game-session.js";
import { HeldMovementInput } from "./held-movement.js";
import {
  clampInputUnit,
  resolveKeyboardAction,
  resolvePointerAction,
  resolvePointerButtonAction,
  type ResolvedAttackAction,
  type ResolvedPlayerAction,
} from "./input-resolver.js";
import { LocalLookPresentationOffset } from "./local-look-offset.js";
import { BrowserPresentationFeedback } from "./presentation-feedback.js";
import {
  RuntimeProjectionAdapter,
  derivePlayerCameraPose,
  type RuntimeBrowserState,
  type RuntimeFeedbackCue,
  type RuntimeSaveSlotId,
  type RuntimeSaveSlotSummary,
} from "./projection.js";
import { WeaponViewmodelAdapter } from "./weapon-viewmodel.js";

type VoxelEditOperation =
  | {
      readonly kind: "set";
      readonly address: readonly [number, number, number];
      readonly materialSlot: number;
    }
  | {
      readonly kind: "clear";
      readonly address: readonly [number, number, number];
    };

const PRODUCT_EDIT_VOXEL = [2, 1, 6] as const;

export interface LoadingBayHostPresentationPreferences {
  readonly mouseSensitivity: number;
  readonly invertY: boolean;
  readonly sfxVolume: number;
  readonly flashIntensity: number;
  readonly telemetryVisible: boolean;
}

export type LoadingBaySaveSlotId = RuntimeSaveSlotId;
export type LoadingBaySaveSlot = RuntimeSaveSlotSummary;

export interface LoadingBayInventoryStack {
  readonly item: string;
  readonly quantity: number;
}

export interface LoadingBayWeaponSlot {
  readonly slot: number;
  readonly item: string;
  readonly owned: boolean;
  readonly selected: boolean;
  readonly ammunition: string;
  readonly ammunitionQuantity: number;
}

export interface LoadingBayInputBindings {
  readonly moveForward: string;
  readonly moveBackward: string;
  readonly moveLeft: string;
  readonly moveRight: string;
  readonly mouseLook: string;
  readonly primaryFire: string;
  readonly selectWeapon: readonly string[];
}

export interface LoadingBayPresentationSnapshot {
  readonly ammoCapacity: number;
  readonly ammoRemaining: number;
  readonly armor: number;
  readonly bindings: LoadingBayInputBindings;
  readonly connected: boolean;
  readonly doorState: "closed" | "open";
  readonly equippedWeapon: string | null;
  readonly encounterState: string;
  readonly events: readonly string[];
  readonly health: number;
  readonly headingDegrees: number;
  readonly hostSessionId: string;
  readonly interactionPrompt: string | null;
  readonly interactionTarget: number | null;
  readonly inventoryCapacity: number;
  readonly inventoryStacks: readonly LoadingBayInventoryStack[];
  readonly lastRejection: string | null;
  readonly maxArmor: number;
  readonly maxHealth: number;
  readonly paused: boolean;
  readonly levelComplete: boolean;
  readonly levelCompletionPresentation: string | null;
  readonly restartAvailable: boolean;
  readonly saveSlots: readonly LoadingBaySaveSlot[];
  readonly vitalityState: "alive" | "dead";
  readonly weaponItem: string;
  readonly weaponPresentation: string;
  readonly weaponSlots: readonly LoadingBayWeaponSlot[];
}

export interface LoadingBayGameOptions {
  readonly onProjection?: (snapshot: LoadingBayPresentationSnapshot) => void;
  readonly onConnectionFailure?: (message: string) => void;
  readonly preferences?: LoadingBayHostPresentationPreferences;
}

export interface LoadingBayGameHandle {
  readonly dispose: () => Promise<void>;
  readonly interact: (target: number) => Promise<void>;
  readonly loadGame: (
    slot: LoadingBaySaveSlotId,
    expectedStorageRevision: string | null,
  ) => Promise<void>;
  readonly releaseInput: () => void;
  readonly restart: () => Promise<void>;
  readonly saveGame: (
    slot: LoadingBaySaveSlotId,
    overwrite: boolean,
    expectedStorageRevision: string | null,
  ) => Promise<void>;
  readonly selectWeaponSlot: (slot: number) => Promise<void>;
  readonly setPaused: (paused: boolean) => Promise<void>;
  readonly updatePreferences: (
    preferences: LoadingBayHostPresentationPreferences,
  ) => void;
  readonly useItem: (item: string) => Promise<void>;
}

export async function mountLoadingBayGame(
  options: LoadingBayGameOptions = {},
): Promise<LoadingBayGameHandle> {
  const eventController = new AbortController();
  const eventOptions = { signal: eventController.signal };
  const canvas = requiredElement("viewport", HTMLCanvasElement);
  const encounterState = requiredElement("encounter-state", HTMLElement);
  const revision = requiredElement("revision", HTMLElement);
  const doorCaption = requiredElement("door-caption", HTMLElement);
  const enemyList = requiredElement("enemy-list", HTMLElement);
  const motionState = requiredElement("motion-state", HTMLElement);
  const navigationState = requiredElement("navigation-state", HTMLElement);
  const playerMotionState = requiredElement("player-motion-state", HTMLElement);
  const combatState = requiredElement("combat-state", HTMLElement);
  const beaconState = requiredElement("beacon-state", HTMLElement);
  const playerPose = requiredElement("player-pose", HTMLElement);
  const weaponState = requiredElement("weapon-state", HTMLElement);
  const inventoryState = requiredElement("inventory-state", HTMLElement);
  const pickupState = requiredElement("pickup-state", HTMLElement);
  const environmentState = requiredElement("environment-state", HTMLElement);
  const voxelState = requiredElement("voxel-state", HTMLElement);
  const persistVoxelEdit = requiredElement(
    "persist-voxel-edit",
    HTMLInputElement,
  );
  const eventList = requiredElement("event-list", HTMLOListElement);
  const rendererStatus = requiredElement("renderer-status", HTMLElement);
  const smokeResult = requiredElement("smoke-result", HTMLElement);
  const feedbackLayer = requiredElement("feedback-layer", HTMLElement);
  const feedbackAudioStatus = requiredElement(
    "feedback-audio-status",
    HTMLElement,
  );
  const telemetryLayer = requiredElement("renderer-telemetry", HTMLElement);
  const sessionTelemetry = requiredElement("session-telemetry", HTMLElement);
  const projection = new RuntimeProjectionAdapter();
  const eventHistory: string[] = [];
  const observedEventKinds = new Set<string>();
  let eventKindOverflow = false;
  const query = new URLSearchParams(location.search);
  const smokeMode = query.has("smoke");
  const reloadSmokeMode = query.has("reload-smoke");
  const convertedSmokeMode = query.has("converted-smoke");
  const inputProofMode = query.has("input-proof");
  let lastActionRejection: string | null = null;
  const authoringQueue = new SerializedActionQueue(recordActionRejection);

  const session = await LoadingBayGameSession.connect();
  let current = session.state;
  let latestMovement = { kind: "move" as const, forward: 0, right: 0 };
  let primaryFireHeld = false;
  let lookGeneration = 0;
  let disposed = false;
  let rendererTelemetryRefreshObserved = false;
  let rendererTelemetryResetObserved = false;
  let hostPreferences = normalizeHostPreferences(options.preferences);
  const localLookOffset = new LocalLookPresentationOffset();
  const heldMovement = new HeldMovementInput({
    bindings: () => current.player.bindings,
    intervalMilliseconds: () => 1_000 / 60,
    dispatch: (action) =>
      enqueueMovementIntent(action).catch((error: unknown) => {
        recordActionRejection(error);
      }),
  });
  const lookInput = new CoalescedLookInput({
    dispatch: (action) => {
      const generation = lookGeneration;
      return (async () => {
        if (generation !== lookGeneration || disposed) {
          return;
        }
        try {
          await performPlayerAction(action);
        } finally {
          if (generation === lookGeneration && !disposed) {
            localLookOffset.settleAcceptedFrame(action);
            applyPresentationCamera();
          }
        }
      })().catch((error: unknown) => {
        recordActionRejection(error);
      });
    },
  });
  const initialFrame = projection.apply(current);
  const initialCamera = derivePlayerCameraPose(current.player);
  const surface = mountRendererSurface(canvas, {
    autoStart: true,
    controls: {
      initialPosition: initialCamera.position,
      initialPitchDegrees: initialCamera.pitchDegrees,
      initialYawDegrees: initialCamera.yawDegrees,
    },
    clearColor: 0x071012,
    frame: initialFrame,
    pixelRatio: Math.min(globalThis.devicePixelRatio ?? 1, 2),
    projection: { fovYDegrees: 50, near: 0.1, far: 100 },
  });
  initialFrame.commit();
  const presentationFeedback = new BrowserPresentationFeedback({
    audioStatus: feedbackAudioStatus,
    layer: feedbackLayer,
    readState: () => current,
    surface,
    telemetryLayer,
    viewmodel: new WeaponViewmodelAdapter(),
  });
  presentationFeedback.setAudioLevel(hostPreferences.sfxVolume);
  presentationFeedback.setFlashIntensity(hostPreferences.flashIntensity);
  telemetryLayer.hidden = !hostPreferences.telemetryVisible;
  session.setStateListener(applySessionState);
  session.setFailureListener((error) => {
    if (!disposed) {
      const connectionFailure =
        (error.retry === "reconnect" && error.code !== "sessionClosed") ||
        error.code === "protocolMismatch";
      if (connectionFailure) {
        clearActiveInput();
        recordActionRejection(error);
        options.onConnectionFailure?.(error.message);
      }
    }
  });
  const dispose = async (): Promise<void> => {
    if (disposed) {
      return;
    }
    disposed = true;
    eventController.abort();
    heldMovement.clear(false);
    lookGeneration += 1;
    lookInput.dispose();
    localLookOffset.reset();
    latestMovement = { kind: "move", forward: 0, right: 0 };
    primaryFireHeld = false;
    if (document.pointerLockElement === canvas) {
      document.exitPointerLock();
    }
    const disconnected = session.close();
    const feedbackDisposed = presentationFeedback.dispose();
    surface.dispose();
    await Promise.all([disconnected, feedbackDisposed]);
  };
  renderReadout(current);
  await applyPresentationFeedback(true, initialFrame.ops.length);
  updateRendererStatus();
  updateSessionDiagnostics();

  requiredElement("primary-fire", HTMLButtonElement).addEventListener(
    "click",
    () => {
      void presentationFeedback.activateAudio();
      void enqueueAttackAction({ kind: "attack" }).catch(recordActionRejection);
    },
    eventOptions,
  );
  requiredElement("reset", HTMLButtonElement).addEventListener(
    "click",
    () => {
      void presentationFeedback.activateAudio();
      heldMovement.clear(false);
      lookGeneration += 1;
      lookInput.clear();
      clearLocalLookPresentation();
      latestMovement = { kind: "move", forward: 0, right: 0 };
      primaryFireHeld = false;
      void performRestart();
    },
    eventOptions,
  );
  requiredElement("activate-beacon", HTMLButtonElement).addEventListener(
    "click",
    () => {
      void presentationFeedback.activateAudio();
      void enqueueInteraction(7).catch(recordActionRejection);
    },
    eventOptions,
  );
  requiredElement("use-health-supply", HTMLButtonElement).addEventListener(
    "click",
    () => {
      void presentationFeedback.activateAudio();
      void performUseItem("supply/med-patch").catch(recordActionRejection);
    },
    eventOptions,
  );
  requiredElement("remove-voxel", HTMLButtonElement).addEventListener(
    "click",
    () => {
      void authoringQueue.enqueue(() =>
        performVoxelEdit({
          kind: "clear",
          address: PRODUCT_EDIT_VOXEL,
        }),
      );
    },
    eventOptions,
  );
  requiredElement("place-voxel", HTMLButtonElement).addEventListener(
    "click",
    () => {
      void authoringQueue.enqueue(() =>
        performVoxelEdit({
          kind: "set",
          address: PRODUCT_EDIT_VOXEL,
          materialSlot: 3,
        }),
      );
    },
    eventOptions,
  );

  window.addEventListener(
    "keydown",
    (event) => {
      const action = resolveKeyboardAction(event.code, current.player.bindings);
      if (action === null) {
        return;
      }
      void presentationFeedback.activateAudio();
      event.preventDefault();
      if (action.kind === "move") {
        heldMovement.press(event.code);
      } else if (action.kind === "selectWeaponSlot") {
        if (!event.repeat) {
          void enqueueWeaponSelection(action.slot).catch(recordActionRejection);
        }
      } else if (!event.repeat) {
        primaryFireHeld = true;
        enqueueCurrentInput();
      }
    },
    eventOptions,
  );
  window.addEventListener(
    "keyup",
    (event) => {
      const releasedMovement = heldMovement.release(event.code);
      const releasedFire =
        event.code === current.player.bindings.primaryFire && primaryFireHeld;
      if (releasedFire) {
        primaryFireHeld = false;
        enqueueCurrentInput();
      }
      if (releasedMovement || releasedFire) {
        event.preventDefault();
      }
    },
    eventOptions,
  );
  window.addEventListener(
    "blur",
    () => {
      clearActiveInput();
    },
    eventOptions,
  );
  document.addEventListener(
    "visibilitychange",
    () => {
      if (document.hidden) {
        clearActiveInput();
      }
    },
    eventOptions,
  );
  canvas.addEventListener(
    "click",
    () => {
      void presentationFeedback.activateAudio();
      void canvas.requestPointerLock();
    },
    eventOptions,
  );
  canvas.addEventListener(
    "mousedown",
    (event) => {
      if (document.pointerLockElement !== canvas) {
        return;
      }
      const action = resolvePointerButtonAction(
        event.button,
        current.player.bindings,
      );
      if (action !== null) {
        void presentationFeedback.activateAudio();
        event.preventDefault();
        primaryFireHeld = true;
        enqueueCurrentInput();
      }
    },
    eventOptions,
  );
  window.addEventListener(
    "mouseup",
    (event) => {
      const action = resolvePointerButtonAction(
        event.button,
        current.player.bindings,
      );
      if (action !== null && primaryFireHeld) {
        primaryFireHeld = false;
        event.preventDefault();
        enqueueCurrentInput();
      }
    },
    eventOptions,
  );
  document.addEventListener(
    "pointerlockchange",
    () => {
      if (document.pointerLockElement !== canvas) {
        clearActiveInput();
      }
    },
    eventOptions,
  );
  window.addEventListener(
    "mousemove",
    (event) => {
      if (
        !smokeMode &&
        !convertedSmokeMode &&
        !inputProofMode &&
        document.pointerLockElement !== canvas
      ) {
        return;
      }
      const pitchDirection = hostPreferences.invertY ? -1 : 1;
      const action = resolvePointerAction(
        event.movementX * hostPreferences.mouseSensitivity,
        event.movementY * hostPreferences.mouseSensitivity * pitchDirection,
        current.player.bindings,
      );
      if (action?.kind === "look") {
        const acceptedPreview = lookInput.push(
          action.yawDelta,
          action.pitchDelta,
        );
        if (acceptedPreview !== null) {
          localLookOffset.applyPendingDelta(acceptedPreview);
          applyPresentationCamera();
        }
      }
    },
    eventOptions,
  );
  window.addEventListener(
    "pagehide",
    () => {
      void dispose();
    },
    eventOptions,
  );

  if (reloadSmokeMode) {
    const door = current.projection.find((node) => node.id === 3);
    const reloadPostureRebuilt =
      current.encounterState === "cleared" &&
      current.doorState === "open" &&
      door?.translation?.[1] === 5.5 &&
      current.enemies.every((enemy) => enemy.state === "defeated") &&
      doorCaption.dataset.posture === "open" &&
      document.querySelector<HTMLElement>('[data-entity-id="4"]')?.dataset
        .posture === "defeated" &&
      document.querySelector<HTMLElement>('[data-entity-id="5"]')?.dataset
        .posture === "defeated" &&
      current.extractionBeacon?.state === "active" &&
      beaconState.dataset.posture === "active" &&
      includesEvery(feedbackLayer.dataset.animationStates, [
        "3:open",
        "4:defeated",
        "5:defeated",
        "7:active",
      ]);
    const reloadCuesCleared =
      current.presentation.cues.length === 0 &&
      feedbackLayer.dataset.lastCueCount === "0";
    const reloadPulsesCleared =
      document.querySelector("[data-animation-pulse]") === null;
    const reloadDomTargets = document.querySelectorAll(
      ".feedback-particle, .feedback-billboard",
    ).length;
    const reloadAudioTargets = Number(
      feedbackAudioStatus.dataset.activeSounds ?? "-1",
    );
    const reloadPassed =
      reloadPostureRebuilt &&
      reloadCuesCleared &&
      reloadPulsesCleared &&
      reloadDomTargets === 0 &&
      reloadAudioTargets === 0 &&
      feedbackLayer.dataset.activeEffects === "0";
    document.body.dataset.reloadPosture = reloadPostureRebuilt
      ? "pass"
      : "fail";
    document.body.dataset.reloadCues = reloadCuesCleared ? "pass" : "fail";
    document.body.dataset.reloadPulses = reloadPulsesCleared ? "pass" : "fail";
    document.body.dataset.reloadDomTargets = String(reloadDomTargets);
    document.body.dataset.reloadAudioTargets = String(reloadAudioTargets);
    document.body.dataset.feedbackPageReload = reloadPassed ? "pass" : "fail";
    document.body.dataset.smokeStatus = reloadPassed ? "pass" : "fail";
    smokeResult.dataset.status = reloadPassed ? "pass" : "fail";
    smokeResult.textContent = reloadPassed
      ? "PASS · Page reload rebuilt posture without transient feedback"
      : "FAIL · Page reload retained or rebuilt transient feedback";
  } else if (convertedSmokeMode) {
    const before = voxelFingerprint(current);
    const convertedAssetLoaded =
      before.revision === 0 &&
      before.solidCount === 94 &&
      before.probePathLength === 9 &&
      current.generatedEnvironment === null &&
      current.voxelMeshes.length === 1;
    const convertedAssetVisible =
      convertedAssetLoaded &&
      surface.snapshot().includes("generated-room-chunk");
    const blockedByConvertedWall = !(await walkPlayerPath([
      [1.5, 5.5],
      [4.5, 5.5],
      [4.5, 8.5],
    ]));
    await performRestart();

    await performVoxelEdits([
      { kind: "clear", address: [4, 1, 6] },
      { kind: "clear", address: [5, 1, 6] },
      { kind: "clear", address: [4, 1, 7] },
      { kind: "clear", address: [5, 1, 7] },
    ]);
    const receipt = current.voxelEditReceipt;
    const convertedEditApplied =
      receipt?.acceptedRevision === 1 &&
      receipt.changedVoxels === 4 &&
      receipt.persistedToProject === false &&
      current.voxelRevision === 1 &&
      current.voxelSolidCount === before.solidCount - 4 &&
      current.voxelAuthorityHash !== before.authorityHash &&
      meshFingerprint(current) !== before.meshHash &&
      current.generatedEnvironment === null &&
      surface.snapshot().includes("generated-room-chunk");
    const convertedNavigationUpdated =
      current.voxelNavigationHash !== before.navigationHash &&
      current.voxelProbePathLength < before.probePathLength;
    const clearedWallTraversed = await walkPlayerPath([
      [1.5, 5.5],
      [4.5, 5.5],
      [4.5, 8.5],
    ]);
    const convertedCollisionPassed =
      blockedByConvertedWall && clearedWallTraversed;
    const passed =
      convertedAssetLoaded &&
      convertedAssetVisible &&
      convertedCollisionPassed &&
      convertedNavigationUpdated &&
      convertedEditApplied;
    document.body.dataset.convertedAsset = convertedAssetLoaded
      ? "pass"
      : "fail";
    document.body.dataset.convertedVisible = convertedAssetVisible
      ? "pass"
      : "fail";
    document.body.dataset.convertedCollision = convertedCollisionPassed
      ? "pass"
      : "fail";
    document.body.dataset.convertedNavigation = convertedNavigationUpdated
      ? "pass"
      : "fail";
    document.body.dataset.convertedEdit = convertedEditApplied
      ? "pass"
      : "fail";
    document.body.dataset.smokeStatus = passed ? "pass" : "fail";
    smokeResult.dataset.status = passed ? "pass" : "fail";
    smokeResult.textContent = passed
      ? "PASS · Converted voxel asset reached retained WebGL, collision, navigation, and live edits"
      : "FAIL · Converted voxel product proof did not converge";
  } else if (smokeMode) {
    let campaignPassed = false;
    try {
      campaignPassed = await proveCampaignRoute();
    } catch (error) {
      document.body.dataset.campaignError =
        error instanceof Error ? error.message : String(error);
    }
    if (campaignPassed) {
      await performSaveGame("slot3", false, null);
    }
    const completedSavePublished =
      campaignPassed &&
      current.saveSlots.some(
        (slot) =>
          slot.slot === "slot3" &&
          slot.compatibility === "available" &&
          slot.metadata?.levelComplete === true,
      ) &&
      current.levelComplete;
    document.body.dataset.completedSave = completedSavePublished
      ? "pass"
      : "fail";
    updateSessionDiagnostics();
    const rendererTelemetryPassed =
      rendererTelemetryRefreshObserved &&
      telemetryLayer.dataset.rendererTimingSource === "animationFrame" &&
      telemetryLayer.dataset.rendererFrameIntervalStatus === "available" &&
      Number(
        telemetryLayer.dataset.rendererFrameIntervalMilliseconds ?? "NaN",
      ) > 0 &&
      Number(
        telemetryLayer.dataset.rendererBackendSubmissionMilliseconds ?? "NaN",
      ) >= 0;
    document.body.dataset.rendererTelemetry = rendererTelemetryPassed
      ? "pass"
      : "fail";
    document.body.dataset.rendererSingleLoop =
      telemetryLayer.dataset.rendererTimingSource === "animationFrame"
        ? "pass"
        : "fail";
    const sessionMetrics = session.metrics;
    const sessionTransportPassed =
      sessionMetrics.legacyWholeStateBytes > 0 &&
      sessionMetrics.bootstrapOutboundBytes > 0 &&
      sessionMetrics.steadyStateUpdateCount > 0 &&
      sessionMetrics.steadyStateLastBytes <
        sessionMetrics.legacyWholeStateBytes / 2 &&
      sessionMetrics.steadyStateMaxBytes <
        sessionMetrics.legacyWholeStateBytes / 2 &&
      sessionMetrics.maximumPendingOutboundUpdates === 1 &&
      sessionMetrics.droppedFactCount === 0 &&
      session.pendingInputFrameCount === 0 &&
      session.pendingEdgeCount === 0 &&
      session.maximumPendingInputFrameCount <= 2 &&
      session.maximumPendingEdgeCount <= 32 &&
      session.maximumCommandRoundTripMilliseconds > 0 &&
      session.maximumCommandRoundTripMilliseconds < 2_000;
    document.body.dataset.sessionTransport = sessionTransportPassed
      ? "pass"
      : "fail";
    const passed =
      campaignPassed &&
      completedSavePublished &&
      rendererTelemetryPassed &&
      sessionTransportPassed;
    smokeResult.dataset.status = passed ? "pass" : "fail";
    smokeResult.textContent = passed
      ? "PASS · Original Loading Bay campaign completed through Rust authority"
      : "FAIL · Loading Bay campaign route did not converge";
    document.body.dataset.smokeStatus = passed ? "pass" : "fail";
  }

  return {
    dispose,
    interact: (target) => runUiAction(() => enqueueInteraction(target)),
    loadGame: (slot, expectedStorageRevision) =>
      runUiAction(() => performLoadGame(slot, expectedStorageRevision)),
    releaseInput: releaseCapturedInput,
    restart: () => runUiAction(performRestart),
    saveGame: (slot, overwrite, expectedStorageRevision) =>
      runUiAction(() =>
        performSaveGame(slot, overwrite, expectedStorageRevision),
      ),
    selectWeaponSlot: (slot) => runUiAction(() => enqueueWeaponSelection(slot)),
    setPaused: (paused) => runUiAction(() => performSetPaused(paused)),
    updatePreferences,
    useItem: (item) => runUiAction(() => performUseItem(item)),
  };

  async function performRestart(): Promise<void> {
    await performSessionReplacement({
      kind: "restart",
      mode: "authoredBaseline",
    });
  }

  async function performLoadGame(
    slot: LoadingBaySaveSlotId,
    expectedStorageRevision: string | null,
  ): Promise<void> {
    await performSessionReplacement({
      kind: "loadGame",
      slot,
      expectedStorageRevision,
    });
  }

  async function performSessionReplacement(
    command:
      | {
          readonly kind: "restart";
          readonly mode: "authoredBaseline" | "checkpoint";
        }
      | {
          readonly kind: "loadGame";
          readonly slot: LoadingBaySaveSlotId;
          readonly expectedStorageRevision: string | null;
        },
  ): Promise<void> {
    heldMovement.clear(false);
    lookGeneration += 1;
    lookInput.clear();
    clearLocalLookPresentation();
    latestMovement = { kind: "move", forward: 0, right: 0 };
    primaryFireHeld = false;
    session.discardInputForSessionReplacement();
    const telemetrySamplesBeforeRestart = Number(
      telemetryLayer.dataset.rendererSampleSequence ?? "0",
    );
    current = await session.sendEdge(command);
    eventHistory.length = 0;
    observedEventKinds.clear();
    eventKindOverflow = false;
    lastActionRejection = null;
    const frame = projection.apply(current);
    applyRendererFrame(frame);
    applyPresentationCamera();
    renderReadout(current);
    await applyPresentationFeedback(true, frame.ops.length);
    rendererTelemetryResetObserved ||=
      telemetrySamplesBeforeRestart > 1 &&
      telemetryLayer.dataset.rendererSampleSequence === "1";
    document.body.dataset.rendererTelemetryReset =
      rendererTelemetryResetObserved ? "pass" : "pending";
    updateRendererStatus();
  }

  async function performSaveGame(
    slot: LoadingBaySaveSlotId,
    overwrite: boolean,
    expectedStorageRevision: string | null,
  ): Promise<void> {
    current = await session.sendEdge({
      kind: "saveGame",
      slot,
      overwrite,
      expectedStorageRevision,
    });
    lastActionRejection = null;
    renderReadout(current);
  }

  async function performSetPaused(paused: boolean): Promise<void> {
    clearActiveInput();
    if (paused && document.pointerLockElement === canvas) {
      document.exitPointerLock();
    }
    await performInputEdge({ kind: "setPaused", paused });
  }

  async function performUseItem(item: string): Promise<void> {
    current = await session.sendEdge({ kind: "useItem", item });
    lastActionRejection = null;
    renderReadout(current);
  }

  async function runUiAction(operation: () => Promise<void>): Promise<void> {
    void presentationFeedback.activateAudio();
    try {
      await operation();
    } catch (error) {
      recordActionRejection(error);
      throw error;
    }
  }

  function updatePreferences(
    preferences: LoadingBayHostPresentationPreferences,
  ): void {
    hostPreferences = normalizeHostPreferences(preferences);
    presentationFeedback.setAudioLevel(hostPreferences.sfxVolume);
    presentationFeedback.setFlashIntensity(hostPreferences.flashIntensity);
    telemetryLayer.hidden = !hostPreferences.telemetryVisible;
  }

  function applySessionState(state: RuntimeBrowserState): void {
    if (disposed) {
      return;
    }
    if (
      (current.player.vitalityState !== "dead" &&
        state.player.vitalityState === "dead") ||
      (current.input.connected && !state.input.connected)
    ) {
      releaseCapturedInput();
    }
    current = state;
    recordCommittedEvents(state.lastEvents);
    const frame = projection.apply(state);
    applyRendererFrame(frame);
    applyPresentationCamera();
    renderReadout(state);
    void applyPresentationFeedback(false, frame.ops.length);
    updateRendererStatus();
    updateSessionDiagnostics();
  }

  async function performVoxelEdit(edit: VoxelEditOperation): Promise<void> {
    await performVoxelEdits([edit], persistVoxelEdit.checked);
  }

  async function performVoxelEdits(
    edits: readonly VoxelEditOperation[],
    persistToProject = false,
  ): Promise<void> {
    current = await requestState("/api/voxel-edit", "POST", {
      expectedRevision: current.voxelRevision,
      persistToProject,
      edits,
    });
    lastActionRejection = null;
    recordCommittedEvents(current.lastEvents);
    const frame = projection.apply(current);
    applyRendererFrame(frame);
    applyPresentationCamera();
    renderReadout(current);
    void applyPresentationFeedback(false, frame.ops.length);
    updateRendererStatus();
  }

  function renderReadout(state: RuntimeBrowserState): void {
    eventList.dataset.history = eventHistory.join(",");
    document.body.dataset.eventHistoryCount = String(eventHistory.length);
    document.body.dataset.eventHistoryCapacity = String(
      MAX_PRESENTATION_EVENT_HISTORY,
    );
    document.body.dataset.eventHistoryBounded =
      eventHistory.length <= MAX_PRESENTATION_EVENT_HISTORY ? "pass" : "fail";
    document.body.dataset.eventKinds = [...observedEventKinds].join(",");
    document.body.dataset.eventKindCount = String(observedEventKinds.size);
    document.body.dataset.eventKindCapacity = String(
      MAX_PRESENTATION_EVENT_KINDS,
    );
    document.body.dataset.eventKindsBounded = eventKindOverflow
      ? "fail"
      : "pass";
    encounterState.textContent = state.encounterState.toUpperCase();
    encounterState.dataset.state = state.encounterState;
    revision.textContent = `REV ${String(state.entityRevision)}`;
    doorCaption.textContent = state.doorState === "open" ? "OPEN" : "LOCKED";
    doorCaption.dataset.state = state.doorState;
    motionState.textContent = state.motionState.toUpperCase();
    motionState.dataset.state = state.motionState;
    navigationState.textContent = state.navigationState.toUpperCase();
    navigationState.dataset.state = state.navigationState;
    playerMotionState.textContent = state.playerMotionState.toUpperCase();
    playerMotionState.dataset.state = state.playerMotionState;
    combatState.textContent =
      lastActionRejection === null
        ? state.combatState.toUpperCase()
        : "REJECTED";
    combatState.dataset.state =
      lastActionRejection === null ? state.combatState : "rejected";
    combatState.title = lastActionRejection ?? "";
    playerPose.textContent = `${state.player.position.map((value) => value.toFixed(1)).join(", ")} · YAW ${state.player.yawDegrees.toFixed(0)}°`;
    weaponState.textContent = `${state.weapon.presentation.toUpperCase()} · ${String(state.weapon.damage)} DMG · ${String(state.weapon.ammoRemaining)}/${String(state.weapon.ammoCapacity)} ${state.weapon.ammunition.toUpperCase()}`;
    inventoryState.textContent =
      state.inventory === null
        ? "NO AUTHORED INVENTORY"
        : state.inventory.stacks
            .map((stack) => `${stack.item} ×${String(stack.quantity)}`)
            .join(" · ");
    inventoryState.dataset.equipped = state.inventory?.equippedWeapon ?? "none";
    const availablePickups = state.pickups.filter(
      (pickup) => pickup.state === "available",
    );
    pickupState.textContent = `${String(availablePickups.length)}/${String(state.pickups.length)} PICKUPS AVAILABLE`;
    pickupState.dataset.available = availablePickups
      .map((pickup) => String(pickup.id))
      .join(",");
    pickupState.dataset.collected = state.pickups
      .filter((pickup) => pickup.state === "collected")
      .map((pickup) => String(pickup.id))
      .join(",");
    environmentState.textContent =
      state.generatedEnvironment === null
        ? `MATERIALIZED · ${String(state.voxelSolidCount)} VOXELS`
        : `SEED ${String(state.generatedEnvironment.seed)} · ${String(state.generatedEnvironment.meshQuads)} QUADS · ${state.generatedEnvironment.outputHash.slice(0, 8)}`;
    voxelState.textContent = `VOXEL REV ${String(state.voxelRevision)} · NAV ${state.voxelNavigationHash.slice(0, 8)} · PATH ${String(state.voxelProbePathLength)}`;
    beaconState.textContent =
      state.extractionBeacon?.state.toUpperCase() ?? "UNAVAILABLE";
    beaconState.dataset.state = state.extractionBeacon?.state ?? "unavailable";
    if (state.extractionBeacon !== null) {
      beaconState.dataset.entityId = String(state.extractionBeacon.id);
    } else {
      delete beaconState.dataset.entityId;
    }
    enemyList.replaceChildren(
      ...state.enemies.map((enemy) => {
        const row = document.createElement("div");
        row.className = "enemy-row";
        row.dataset.entityId = String(enemy.id);
        row.dataset.state = enemy.state;
        row.dataset.combatPosture = enemy.combatPosture ?? enemy.state;
        const name = document.createElement("span");
        name.textContent = enemy.name;
        const status = document.createElement("strong");
        const posture = enemy.combatPosture ?? enemy.state;
        status.textContent = `${posture.toUpperCase()} · ${String(enemy.currentHealth)}/${String(enemy.maxHealth)} HP`;
        row.append(name, status);
        return row;
      }),
    );
    eventList.replaceChildren(
      ...(eventHistory.length === 0
        ? ["Awaiting action"]
        : eventHistory.slice(-20)
      ).map((event) => {
        const item = document.createElement("li");
        item.textContent = event;
        return item;
      }),
    );
    options.onProjection?.({
      ammoCapacity: state.weapon.ammoCapacity,
      ammoRemaining: state.weapon.ammoRemaining,
      armor: state.player.armor,
      bindings: state.player.bindings,
      connected: state.input.connected,
      doorState: state.doorState,
      equippedWeapon: state.inventory?.equippedWeapon ?? null,
      encounterState: state.encounterState,
      events: [...eventHistory],
      health: state.player.currentHealth,
      headingDegrees: normalizeDegrees(state.player.yawDegrees),
      hostSessionId: state.hostSessionId,
      interactionPrompt: state.interaction?.prompt ?? null,
      interactionTarget: state.interaction?.target ?? null,
      inventoryCapacity: state.inventory?.capacitySlots ?? 0,
      inventoryStacks: state.inventory?.stacks ?? [],
      lastRejection: lastActionRejection,
      maxArmor: state.player.maxArmor,
      maxHealth: state.player.maxHealth,
      paused: state.input.paused,
      levelComplete: state.levelComplete,
      levelCompletionPresentation:
        state.levelExits.find((exit) => exit.state === "completed")
          ?.presentation ?? null,
      restartAvailable: state.restart.authoredBaselineAvailable,
      saveSlots: state.saveSlots,
      vitalityState: state.player.vitalityState,
      weaponItem: state.weapon.item,
      weaponPresentation: state.weapon.presentation,
      weaponSlots: state.inventory?.weapons ?? [],
    });
  }

  async function applyPresentationFeedback(
    reset = false,
    renderDiffCount = 0,
  ): Promise<void> {
    doorCaption.dataset.entityId = "3";
    playerMotionState.dataset.entityId = String(current.player.id);
    const receipt = await presentationFeedback.apply(
      current,
      reset,
      renderDiffCount,
    );
    feedbackLayer.dataset.lastCueCount = String(receipt.cueCount);
    feedbackLayer.dataset.failedOperations = String(receipt.failedOperations);
    feedbackLayer.dataset.scheduledSounds = String(receipt.scheduledSounds);
    feedbackLayer.dataset.viewmodelOperations = String(
      receipt.viewmodelOperations,
    );
    rendererTelemetryRefreshObserved ||=
      Number(telemetryLayer.dataset.rendererSampleSequence ?? "0") > 1;
    document.body.dataset.rendererTelemetryRefresh =
      rendererTelemetryRefreshObserved ? "pass" : "pending";
  }

  function applyRendererFrame(
    frame: ReturnType<RuntimeProjectionAdapter["apply"]>,
  ): void {
    if (frame.ops.length > 0) {
      const receipt = surface.applyFrame(frame);
      if (!receipt.applied) {
        throw new Error(
          receipt.diagnostics
            .map((diagnostic) => diagnostic.message)
            .join("; "),
        );
      }
    }
    frame.commit();
  }

  function enqueueMovementIntent(action: ResolvedPlayerAction): Promise<void> {
    if (action.kind !== "move") {
      throw new Error("held movement can only dispatch movement intent");
    }
    latestMovement = action;
    return performInputIntent([0, 0]);
  }

  function enqueueCurrentInput(): void {
    session.queueInput({
      movement: [latestMovement.forward, latestMovement.right],
      lookDelta: [0, 0],
      primaryFireHeld,
    });
  }

  function enqueuePlayerAction(action: ResolvedPlayerAction): Promise<void> {
    return performPlayerAction(action);
  }

  function enqueueAttackAction(action: ResolvedAttackAction): Promise<void> {
    return performAttackAction(action);
  }

  function enqueueInteraction(target: number): Promise<void> {
    return performInputEdge({ kind: "interact", target });
  }

  function enqueueWeaponSelection(slot: number): Promise<void> {
    return performInputEdge({ kind: "selectWeaponSlot", slot });
  }

  async function performPlayerAction(
    action: ResolvedPlayerAction,
  ): Promise<void> {
    if (action.kind === "look") {
      await performInputIntent([action.yawDelta, action.pitchDelta]);
      return;
    }
    latestMovement = action;
    await performInputIntent([0, 0]);
    latestMovement = { kind: "move", forward: 0, right: 0 };
    await performInputIntent([0, 0]);
  }

  async function performInputIntent(
    lookDelta: readonly [number, number],
  ): Promise<void> {
    current = await session.sendInput({
      movement: [latestMovement.forward, latestMovement.right],
      lookDelta,
      primaryFireHeld,
    });
    lastActionRejection = null;
    renderReadout(current);
  }

  async function performInputEdge(
    command:
      | { readonly kind: "interact"; readonly target: number }
      | { readonly kind: "selectWeaponSlot"; readonly slot: number }
      | { readonly kind: "setPaused"; readonly paused: boolean },
  ): Promise<void> {
    current = await session.sendEdge(command);
    lastActionRejection = null;
    renderReadout(current);
  }

  async function performAttackAction(
    action: ResolvedAttackAction,
  ): Promise<void> {
    primaryFireHeld = action.kind === "attack";
    await performInputIntent([0, 0]);
    primaryFireHeld = false;
    await performInputIntent([0, 0]);
  }

  function clearActiveInput(): void {
    heldMovement.clear(false);
    lookGeneration += 1;
    lookInput.clear();
    clearLocalLookPresentation();
    latestMovement = { kind: "move", forward: 0, right: 0 };
    primaryFireHeld = false;
    if (current.input.connected) {
      session.neutralizeInput();
    }
  }

  function releaseCapturedInput(): void {
    clearActiveInput();
    if (document.pointerLockElement === canvas) {
      document.exitPointerLock();
    }
  }

  function clearLocalLookPresentation(): void {
    localLookOffset.reset();
    applyPresentationCamera();
  }

  function applyPresentationCamera(): void {
    const projectedLook = localLookOffset.project(
      current.player.yawDegrees,
      current.player.pitchDegrees,
      current.player.lookDegreesPerUnit,
    );
    surface.setCameraPose(
      derivePlayerCameraPose({
        ...current.player,
        yawDegrees: projectedLook.yawDegrees,
        pitchDegrees: projectedLook.pitchDegrees,
      }),
    );
    const [yawUnits, pitchUnits] = localLookOffset.pendingUnits;
    document.body.dataset.localLookPresentation = "bounded-disposable";
    document.body.dataset.localLookYawUnits = yawUnits.toFixed(3);
    document.body.dataset.localLookPitchUnits = pitchUnits.toFixed(3);
  }

  async function aimAtEnemy(enemyId: number): Promise<void> {
    const enemy = current.enemies.find((candidate) => candidate.id === enemyId);
    if (enemy === undefined) {
      throw new Error(`enemy ${String(enemyId)} is absent`);
    }
    const offset: [number, number, number] = [
      enemy.position[0] - current.player.position[0],
      enemy.position[1] - current.player.position[1],
      enemy.position[2] - current.player.position[2],
    ];
    const horizontal = Math.hypot(offset[0], offset[2]);
    const desiredYaw = normalizeDegrees(
      (Math.atan2(-offset[0], -offset[2]) * 180) / Math.PI,
    );
    const desiredPitch = (Math.atan2(offset[1], horizontal) * 180) / Math.PI;
    for (let step = 0; step < 40; step += 1) {
      const yawDifference = normalizeDegrees(
        desiredYaw - current.player.yawDegrees,
      );
      const pitchDifference = desiredPitch - current.player.pitchDegrees;
      if (Math.abs(yawDifference) < 0.01 && Math.abs(pitchDifference) < 0.01) {
        return;
      }
      await enqueuePlayerAction({
        kind: "look",
        yawDelta: clampInputUnit(
          yawDifference / current.player.lookDegreesPerUnit,
        ),
        pitchDelta: clampInputUnit(
          pitchDifference / current.player.lookDegreesPerUnit,
        ),
      });
    }
    throw new Error(`could not aim at enemy ${String(enemyId)}`);
  }

  async function firePrimary(): Promise<
    Extract<RuntimeFeedbackCue, { readonly kind: "attack" }> | undefined
  > {
    const action = resolvePointerButtonAction(0, current.player.bindings);
    if (action === null) {
      throw new Error("authored primary-fire binding did not resolve Mouse0");
    }
    primaryFireHeld = action.kind === "attack";
    await performInputIntent([0, 0]);
    const cue = current.presentation.cues.find(
      (
        candidate,
      ): candidate is Extract<
        RuntimeFeedbackCue,
        { readonly kind: "attack" }
      > => candidate.kind === "attack",
    );
    primaryFireHeld = false;
    await performInputIntent([0, 0]);
    return cue;
  }

  async function damageEnemyTo(
    enemyId: number,
    targetHealth: number,
    maxAttempts = 8,
  ): Promise<boolean> {
    for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
      const health = current.enemies.find(
        (enemy) => enemy.id === enemyId,
      )?.currentHealth;
      if (health !== undefined && health <= targetHealth) {
        return true;
      }
      await useCampaignMedPatchIfNeeded();
      await aimAtEnemy(enemyId);
      await firePrimary();
    }
    return (
      current.enemies.find((enemy) => enemy.id === enemyId)?.currentHealth ===
      targetHealth
    );
  }

  async function walkPlayerPath(
    waypoints: readonly (readonly [number, number])[],
  ): Promise<boolean> {
    for (const waypoint of waypoints) {
      if (!(await walkPlayerTo(waypoint))) {
        return false;
      }
    }
    return true;
  }

  async function useCampaignMedPatchIfNeeded(
    healthThreshold = 75,
  ): Promise<void> {
    while (
      current.player.currentHealth <= healthThreshold &&
      current.player.currentHealth < current.player.maxHealth &&
      inventoryQuantity("supply/med-patch") > 0
    ) {
      const beforeHealth = current.player.currentHealth;
      const beforeQuantity = inventoryQuantity("supply/med-patch");
      await performUseItem("supply/med-patch");
      if (
        current.player.currentHealth <= beforeHealth ||
        inventoryQuantity("supply/med-patch") >= beforeQuantity
      ) {
        throw new Error("accepted campaign med patch made no progress");
      }
    }
  }

  async function proveCampaignRoute(): Promise<boolean> {
    await presentationFeedback.activateAudio();
    await presentationFeedback.settled();
    const enemyIds = [4, 5, 41, 42, 51, 52, 53, 54] as const;
    const pickupIds = current.pickups.map((pickup) => pickup.id);
    const availablePickupIds = current.pickups
      .filter((pickup) => pickup.state === "available")
      .map((pickup) => pickup.id);
    const dormantPickupIds = current.pickups
      .filter((pickup) => pickup.state === "dormant")
      .map((pickup) => pickup.id);
    const authoredBaseline =
      current.generatedEnvironment === null &&
      current.voxelSolidCount === 3_931 &&
      projection.trackedEntityCount >= 20 &&
      projection.trackedLightCount === 8 &&
      JSON.stringify(current.enemies.map((enemy) => enemy.id)) ===
        JSON.stringify(enemyIds) &&
      JSON.stringify(pickupIds) ===
        JSON.stringify([
          20, 21, 22, 23, 24, 25, 26, 28, 33, 34, 60, 61, 62, 63, 64, 65,
        ]) &&
      JSON.stringify(availablePickupIds) ===
        JSON.stringify([20, 21, 22, 23, 24, 25, 26, 28]) &&
      JSON.stringify(dormantPickupIds) ===
        JSON.stringify([33, 34, 60, 61, 62, 63, 64, 65]);
    document.body.dataset.campaignBaseline = authoredBaseline ? "pass" : "fail";

    const initialViewmodelFingerprint = viewmodelFingerprint();
    const viewmodelNodes = surface
      .projectionSnapshot()
      .nodes.filter((node) => node.layer === "viewmodel");
    const viewmodelPassed =
      viewmodelNodes.length === 7 &&
      surface.pick({
        ray: { kind: "viewport", point: [0, 0] },
        filter: { layers: ["viewmodel"] },
      }).hint === null;
    document.body.dataset.weaponViewmodel = viewmodelPassed ? "pass" : "fail";
    document.body.dataset.weaponViewmodelLayer = viewmodelPassed
      ? "viewmodel"
      : "fail";

    const arrivalReached = await walkPlayerPath([[6.5, 6.5]]);
    const earlyEnemyDefeated =
      arrivalReached && (await damageEnemyTo(4, 0, 10));
    const arrivalSupplyReached =
      earlyEnemyDefeated &&
      (await walkPlayerPath([
        [4.5, 9.5],
        [7.5, 10.5],
      ]));
    const cargoDoorHeight = current.projection.find((node) => node.id === 11)
      ?.translation?.[1];
    const cargoDoorOpened =
      earlyEnemyDefeated &&
      ((cargoDoorHeight ?? 0) > 4 || eventHistory.includes("DoorOpened"));
    const sideStorageReached =
      cargoDoorOpened &&
      (await walkPlayerPath([
        [4.5, 15.5],
        [4.5, 18.5],
        [2.5, 20.5],
        [3.5, 21.5],
        [5.5, 22.5],
        [6.5, 22.5],
        [6.5, 24.5],
        [3.5, 24.5],
      ]));
    const secretDiscovered =
      sideStorageReached &&
      current.secretRegions.some(
        (secret) => secret.id === 31 && secret.state === "discovered",
      );
    const scattergunCollected =
      inventoryQuantity("weapon/breach-scattergun") === 1 &&
      inventoryQuantity("ammo/scatter-shell") >= 14;
    document.body.dataset.campaignArrival =
      arrivalReached &&
      earlyEnemyDefeated &&
      arrivalSupplyReached &&
      cargoDoorOpened
        ? "pass"
        : "fail";
    document.body.dataset.campaignArrivalEvidence = [
      arrivalReached,
      earlyEnemyDefeated,
      arrivalSupplyReached,
      cargoDoorOpened,
      cargoDoorHeight ?? "missing",
      current.player.position.join(","),
    ].join(":");
    document.body.dataset.campaignStorage =
      sideStorageReached && secretDiscovered && scattergunCollected
        ? "pass"
        : "fail";
    document.body.dataset.campaignStorageEvidence = [
      sideStorageReached,
      secretDiscovered,
      scattergunCollected,
      current.player.position.join(","),
      inventoryQuantity("ammo/scatter-shell"),
    ].join(":");
    if (
      !arrivalReached ||
      !earlyEnemyDefeated ||
      !arrivalSupplyReached ||
      !cargoDoorOpened ||
      !sideStorageReached ||
      !secretDiscovered ||
      !scattergunCollected
    ) {
      return false;
    }

    const generatorReached =
      sideStorageReached &&
      (await walkPlayerPath([
        [6.5, 26.5],
        [15.5, 27.5],
        [18.5, 27.5],
        [18.5, 24.5],
      ]));
    const generatorProtectionReady =
      generatorReached &&
      current.player.armor > 0 &&
      inventoryQuantity("armor/impact-vest") === 0;
    if (
      generatorReached &&
      scattergunCollected &&
      current.weapon.item !== "weapon/breach-scattergun"
    ) {
      await enqueueWeaponSelection(1);
    }
    await aimAtEnemy(5);
    const scatterAmmoBefore = current.weapon.ammoRemaining;
    const scatterCue = await firePrimary();
    const spreadObserved =
      scatterCue?.weapon === "weapon/breach-scattergun" &&
      scatterCue.attackMode === "spread" &&
      scatterCue.rayCount === 7 &&
      current.weapon.ammoRemaining === scatterAmmoBefore - 1;
    const generatorSentryDefeated = await damageEnemyTo(5, 0, 10);
    await useCampaignMedPatchIfNeeded();
    if (current.weapon.item !== "weapon/arc-pistol") {
      await enqueueWeaponSelection(0);
    }
    const generatorLoaderDefeated =
      (await walkPlayerPath([[21.5, 23.5]])) &&
      (await damageEnemyTo(41, 0, 10));
    await useCampaignMedPatchIfNeeded();
    const generatorWardenDefeated =
      (await walkPlayerPath([[24.5, 27.5]])) &&
      (await damageEnemyTo(42, 0, 10));
    await useCampaignMedPatchIfNeeded();
    const lockedDoorReached = await walkPlayerPath([
      [23.5, 29.2],
      [23.5, 31.5],
      [15.5, 32.5],
      [11.5, 28.5],
      [11.5, 19.5],
    ]);
    if (lockedDoorReached) {
      await enqueueInteraction(30);
    }
    await presentationFeedback.settled();
    const lockedDoorDenied =
      lockedDoorReached &&
      current.doorAccess.some(
        (door) => door.id === 30 && door.state === "closed",
      ) &&
      inventoryQuantity("key/maintenance-pass") === 0 &&
      includesEvery(feedbackLayer.dataset.animationPulses, ["access-denied"]);
    document.body.dataset.campaignLockedDoor = lockedDoorDenied
      ? "pass"
      : "fail";
    if (
      !lockedDoorDenied ||
      !(await walkPlayerPath([
        [11.5, 28.5],
        [15.5, 32.5],
        [23.5, 31.5],
        [23.5, 29.2],
        [24.5, 27.5],
      ]))
    ) {
      return false;
    }
    const generatorPickupsReached = await walkPlayerPath([
      [24.5, 28.5],
      [27.5, 28.5],
      [26, 28.5],
      [26, 23],
      [23.5, 22.5],
      [23.5, 20.5],
      [19.5, 24.5],
      [18.5, 24.5],
      [18.5, 28.5],
    ]);
    const generatorInventoryReady =
      generatorPickupsReached &&
      inventoryQuantity("key/maintenance-pass") === 1 &&
      inventoryQuantity("armor/impact-vest") === 0 &&
      inventoryQuantity("weapon/rivet-carbine") === 1;
    if (
      generatorInventoryReady &&
      current.weapon.item !== "weapon/rivet-carbine"
    ) {
      await enqueueWeaponSelection(2);
    }
    await aimAtEnemy(42);
    const automaticAmmoBefore = current.weapon.ammoRemaining;
    const automaticCue = await firePrimary();
    const automaticObserved =
      automaticCue?.weapon === "weapon/rivet-carbine" &&
      automaticCue.attackMode === "automatic" &&
      current.weapon.ammoRemaining === automaticAmmoBefore - 1;
    if (current.weapon.item !== "weapon/arc-pistol") {
      await enqueueWeaponSelection(0);
    }
    const generatorDoorOpened =
      (current.projection.find((node) => node.id === 13)?.translation?.[1] ??
        0) > 4;
    document.body.dataset.campaignGenerator =
      generatorReached &&
      generatorProtectionReady &&
      generatorSentryDefeated &&
      generatorLoaderDefeated &&
      generatorWardenDefeated &&
      generatorInventoryReady &&
      generatorDoorOpened
        ? "pass"
        : "fail";
    document.body.dataset.campaignGeneratorEvidence = [
      generatorReached,
      generatorProtectionReady,
      generatorSentryDefeated,
      generatorLoaderDefeated,
      generatorWardenDefeated,
      generatorPickupsReached,
      generatorInventoryReady,
      generatorDoorOpened,
      current.player.position.join(","),
    ].join(":");
    if (
      !generatorReached ||
      !generatorProtectionReady ||
      !generatorSentryDefeated ||
      !generatorLoaderDefeated ||
      !generatorWardenDefeated ||
      !generatorInventoryReady ||
      !generatorDoorOpened
    ) {
      return false;
    }
    await useCampaignMedPatchIfNeeded(95);

    const returnGantryReached =
      generatorDoorOpened &&
      (await walkPlayerPath([
        [23.5, 29.2],
        [23.5, 31.5],
        [15.5, 32.5],
        [11.5, 28.5],
        [11.5, 19.5],
      ]));
    if (returnGantryReached) {
      await enqueueInteraction(30);
    }
    const keyedDoorOpened = current.doorAccess.some(
      (door) => door.id === 30 && door.state === "open",
    );
    const loopbackTraversed =
      keyedDoorOpened &&
      (await walkPlayerPath([
        [11.5, 15.5],
        [11.5, 20.5],
      ]));
    if (loopbackTraversed) {
      await enqueueInteraction(6);
    }
    const extractionGateOpened =
      (current.projection.find((node) => node.id === 12)?.translation?.[1] ??
        0) > 4;
    const finalArenaReached =
      extractionGateOpened &&
      (await walkPlayerPath([
        [15.5, 29.5],
        [15.5, 32.5],
        [21.5, 34.2],
        [21.5, 36.5],
      ]));
    document.body.dataset.campaignLoopback =
      returnGantryReached &&
      keyedDoorOpened &&
      loopbackTraversed &&
      extractionGateOpened &&
      finalArenaReached
        ? "pass"
        : "fail";
    document.body.dataset.campaignLoopbackEvidence = [
      returnGantryReached,
      keyedDoorOpened,
      loopbackTraversed,
      extractionGateOpened,
      finalArenaReached,
      current.player.position.join(","),
    ].join(":");
    if (
      !returnGantryReached ||
      !keyedDoorOpened ||
      !loopbackTraversed ||
      !extractionGateOpened ||
      !finalArenaReached
    ) {
      return false;
    }

    let finalFight: boolean = finalArenaReached;
    for (const [enemyId, waypoint] of [
      [51, [20.5, 38.5]],
      [52, [23.5, 38.5]],
      [53, [20.5, 42.5]],
      [54, [24.5, 43.5]],
    ] as const) {
      if (!finalFight) {
        break;
      }
      if (enemyId === 51) {
        finalFight = await walkPlayerPath([waypoint]);
      }
      finalFight = finalFight && (await damageEnemyTo(enemyId, 0, 10));
      await useCampaignMedPatchIfNeeded();
    }
    const allEnemiesDefeated =
      finalFight &&
      current.enemies.every(
        (enemy) => enemy.state === "defeated" && enemy.currentHealth === 0,
      );
    const finalDoorOpened =
      (current.projection.find((node) => node.id === 3)?.translation?.[1] ??
        0) > 4;
    let dryFireInitialAmmo = -1;
    let dryFireAttempts = 0;
    if (
      allEnemiesDefeated &&
      finalDoorOpened &&
      current.weapon.item !== "weapon/arc-pistol"
    ) {
      await enqueueWeaponSelection(0);
    }
    if (allEnemiesDefeated && finalDoorOpened) {
      await aimAtEnemy(54);
      dryFireInitialAmmo = current.weapon.ammoRemaining;
      const dryFireAttemptBudget = Math.min(dryFireInitialAmmo * 4 + 8, 512);
      for (
        dryFireAttempts = 0;
        current.weapon.ammoRemaining > 0 &&
        dryFireAttempts < dryFireAttemptBudget;
        dryFireAttempts += 1
      ) {
        await firePrimary();
      }
      await firePrimary();
      await presentationFeedback.settled();
    }
    const beaconReached =
      allEnemiesDefeated &&
      (await walkPlayerPath([
        [21.5, 46.5],
        [18.5, 46.5],
      ]));
    if (beaconReached) {
      await enqueueInteraction(7);
    }
    const beaconActivated =
      current.extractionBeacon?.state === "active" &&
      current.extractionBeacon.activatedBy === current.player.id;
    const exitReached =
      beaconActivated &&
      (await walkPlayerPath([
        [21.5, 48],
        [21.5, 50.5],
      ]));
    if (exitReached) {
      await enqueueInteraction(32);
    }
    const levelCompleted =
      current.levelComplete &&
      current.levelExits.some(
        (exit) =>
          exit.id === 32 &&
          exit.state === "completed" &&
          exit.completedBy === current.player.id,
      );
    if (levelCompleted) {
      await performSaveGame("checkpoint", false, null);
      await presentationFeedback.settled();
    }
    const checkpointSaved =
      current.saveSlots.some(
        (slot) =>
          slot.slot === "checkpoint" &&
          slot.compatibility === "available" &&
          slot.metadata?.levelComplete === true,
      ) &&
      includesEvery(feedbackLayer.dataset.animationPulses, [
        "checkpoint-saved",
      ]);
    const terminalWeaponFeedback =
      includesEvery(feedbackLayer.dataset.animationPulses, [
        "attack-hit",
        "arc-pistol-dry",
      ]) &&
      (feedbackLayer.dataset.animationPulses ?? "").includes("attack-miss-");
    document.body.dataset.campaignWeaponEvidence = [
      dryFireInitialAmmo,
      dryFireAttempts,
      current.weapon.ammoRemaining,
      terminalWeaponFeedback,
      feedbackLayer.dataset.animationPulses ?? "",
    ].join(":");
    const materializedDrops = current.pickups.filter(
      (pickup) => pickup.id >= 33 && pickup.state !== "dormant",
    ).length;
    const routePresentation =
      spreadObserved &&
      automaticObserved &&
      initialViewmodelFingerprint.length > 0 &&
      includesEvery(feedbackLayer.dataset.viewmodelWeapons, [
        "weapon/arc-pistol",
        "weapon/breach-scattergun",
        "weapon/rivet-carbine",
      ]) &&
      includesEvery(feedbackLayer.dataset.animationPulses, [
        "pickup",
        "encounter-activated",
        "attack-hit",
        "enemy-hurt",
        "enemy-defeated",
        "player-damage",
        "access-denied",
        "switch-activated",
        "checkpoint-saved",
        "arc-pistol-dry",
        "drop-materialized",
        "open",
        "active",
      ]);
    document.body.dataset.campaignFinale =
      allEnemiesDefeated && finalDoorOpened && beaconActivated && levelCompleted
        ? "pass"
        : "fail";
    document.body.dataset.campaignWeapons = routePresentation ? "pass" : "fail";
    document.body.dataset.enemyArchetypes = authoredBaseline ? "pass" : "fail";
    document.body.dataset.enemyDrops =
      materializedDrops === enemyIds.length ? "pass" : "fail";
    document.body.dataset.beaconActivation = beaconActivated ? "pass" : "fail";
    document.body.dataset.progressionRoute = levelCompleted ? "pass" : "fail";
    document.body.dataset.campaignEvidence = [
      authoredBaseline,
      arrivalReached,
      earlyEnemyDefeated,
      cargoDoorOpened,
      sideStorageReached,
      secretDiscovered,
      generatorReached,
      generatorProtectionReady,
      generatorSentryDefeated,
      generatorLoaderDefeated,
      generatorWardenDefeated,
      generatorInventoryReady,
      returnGantryReached,
      keyedDoorOpened,
      loopbackTraversed,
      extractionGateOpened,
      finalArenaReached,
      allEnemiesDefeated,
      finalDoorOpened,
      beaconActivated,
      levelCompleted,
      materializedDrops,
      spreadObserved,
      automaticObserved,
      lockedDoorDenied,
      checkpointSaved,
      terminalWeaponFeedback,
    ].join(":");
    return (
      authoredBaseline &&
      viewmodelPassed &&
      earlyEnemyDefeated &&
      cargoDoorOpened &&
      sideStorageReached &&
      secretDiscovered &&
      scattergunCollected &&
      generatorReached &&
      generatorProtectionReady &&
      generatorSentryDefeated &&
      generatorLoaderDefeated &&
      generatorWardenDefeated &&
      generatorInventoryReady &&
      generatorDoorOpened &&
      returnGantryReached &&
      keyedDoorOpened &&
      loopbackTraversed &&
      extractionGateOpened &&
      finalArenaReached &&
      allEnemiesDefeated &&
      finalDoorOpened &&
      beaconActivated &&
      levelCompleted &&
      materializedDrops === enemyIds.length &&
      lockedDoorDenied &&
      checkpointSaved &&
      terminalWeaponFeedback &&
      routePresentation
    );
  }

  function inventoryQuantity(item: string): number {
    return (
      current.inventory?.stacks.find((stack) => stack.item === item)
        ?.quantity ?? 0
    );
  }

  async function walkPlayerTo(
    target: readonly [number, number],
    maxSteps = 256,
  ): Promise<boolean> {
    const initialOffsetX = target[0] - current.player.position[0];
    const initialOffsetZ = target[1] - current.player.position[2];
    if (Math.hypot(initialOffsetX, initialOffsetZ) <= 0.25) {
      return true;
    }
    await turnPlayerToward(initialOffsetX, initialOffsetZ);
    const moveForward = current.player.bindings.moveForward;
    const action = resolveKeyboardAction(moveForward, current.player.bindings);
    if (action?.kind !== "move" || !heldMovement.press(moveForward)) {
      throw new Error(
        "authored move-forward binding did not resolve to movement",
      );
    }
    let observedSteps = 0;
    let observedPosition = current.player.position;
    let lastProgressAt = performance.now();
    try {
      while (observedSteps < maxSteps) {
        const offsetX = target[0] - current.player.position[0];
        const offsetZ = target[1] - current.player.position[2];
        if (Math.hypot(offsetX, offsetZ) <= 0.25) {
          return true;
        }
        if (current.player.vitalityState === "dead") {
          return false;
        }
        if (
          vectorChanged(current.player.position, observedPosition, 0.000_001)
        ) {
          observedPosition = current.player.position;
          observedSteps += 1;
          lastProgressAt = performance.now();
        } else if (performance.now() - lastProgressAt > 5_000) {
          return false;
        }
        await delay(16);
      }
      return false;
    } finally {
      heldMovement.release(moveForward);
      await heldMovement.settled();
    }
  }

  async function turnPlayerToward(
    offsetX: number,
    offsetZ: number,
  ): Promise<void> {
    const desiredYaw = normalizeDegrees(
      (Math.atan2(-offsetX, -offsetZ) * 180) / Math.PI,
    );
    for (let step = 0; step < 20; step += 1) {
      const yawDifference = normalizeDegrees(
        desiredYaw - current.player.yawDegrees,
      );
      if (Math.abs(yawDifference) < 0.01) {
        return;
      }
      await enqueuePlayerAction({
        kind: "look",
        yawDelta: clampInputUnit(
          yawDifference / current.player.lookDegreesPerUnit,
        ),
        pitchDelta: 0,
      });
    }
    throw new Error("could not orient player toward gate waypoint");
  }

  function updateRendererStatus(): void {
    const timing = surface.timing();
    const cadence =
      timing.frameIntervalMs === null
        ? timing.frameIntervalStatus
        : `${timing.frameIntervalMs.toFixed(2)} ms cadence`;
    rendererStatus.textContent = `${surface.kind} · ${String(projection.trackedEntityCount)} entities · ${String(projection.trackedMeshCount)} voxel meshes · ${cadence}`;
  }

  function viewmodelFingerprint(): string {
    return JSON.stringify(
      surface
        .projectionSnapshot()
        .nodes.filter((node) => node.layer === "viewmodel")
        .map((node) => ({
          handle: node.handle,
          parent: node.parent,
          kind: node.kind,
          transform: node.transform,
          visible: node.visible,
          material: node.material,
          label: node.metadata.label,
          tags: node.metadata.tags,
        })),
    );
  }

  function updateSessionDiagnostics(): void {
    const metrics = session.metrics;
    document.body.dataset.sessionProtocol = "1";
    document.body.dataset.sessionLegacyBytes = String(
      metrics.legacyWholeStateBytes,
    );
    document.body.dataset.sessionBootstrapBytes = String(
      metrics.bootstrapOutboundBytes,
    );
    document.body.dataset.sessionStaticUpdates = String(
      metrics.staticResourceUpdateCount,
    );
    document.body.dataset.sessionStaticBytes = String(
      metrics.staticResourceLastBytes,
    );
    document.body.dataset.sessionStaticMaxBytes = String(
      metrics.staticResourceMaxBytes,
    );
    document.body.dataset.sessionSteadyBytes = String(
      metrics.steadyStateLastBytes,
    );
    document.body.dataset.sessionSteadyMaxBytes = String(
      metrics.steadyStateMaxBytes,
    );
    document.body.dataset.sessionSteadyUpdates = String(
      metrics.steadyStateUpdateCount,
    );
    document.body.dataset.sessionPendingOutboundMax = String(
      metrics.maximumPendingOutboundUpdates,
    );
    document.body.dataset.sessionDroppedFacts = String(
      metrics.droppedFactCount,
    );
    document.body.dataset.sessionBuildMicroseconds = String(
      metrics.lastUpdateBuildMicroseconds,
    );
    document.body.dataset.sessionBuildMaxMicroseconds = String(
      metrics.maximumUpdateBuildMicroseconds,
    );
    document.body.dataset.sessionPendingInput = String(
      session.pendingInputFrameCount,
    );
    document.body.dataset.sessionPendingInputMax = String(
      session.maximumPendingInputFrameCount,
    );
    document.body.dataset.sessionPendingEdges = String(
      session.pendingEdgeCount,
    );
    document.body.dataset.sessionPendingEdgesMax = String(
      session.maximumPendingEdgeCount,
    );
    document.body.dataset.sessionRttMilliseconds =
      session.lastCommandRoundTripMilliseconds.toFixed(3);
    document.body.dataset.sessionRttMaxMilliseconds =
      session.maximumCommandRoundTripMilliseconds.toFixed(3);
    document.body.dataset.sessionServerTick = String(session.serverTick);
    document.body.dataset.sessionSnapshotSequence = String(
      session.snapshotSequence,
    );
    document.body.dataset.sessionSnapshotCadenceMilliseconds =
      session.lastSnapshotCadenceMilliseconds === null
        ? "unavailable"
        : session.lastSnapshotCadenceMilliseconds.toFixed(3);
    sessionTelemetry.textContent = [
      `serverTick: ${String(session.serverTick)} fixed@60Hz`,
      `snapshotSequence: ${String(session.snapshotSequence)}`,
      `snapshotCadenceMs: ${
        session.lastSnapshotCadenceMilliseconds === null
          ? "waiting"
          : session.lastSnapshotCadenceMilliseconds.toFixed(2)
      }`,
      `dynamicPayloadBytes: ${String(metrics.steadyStateLastBytes)}`,
      `inputFrames: ${String(session.pendingInputFrameCount)}/2 (max ${String(session.maximumPendingInputFrameCount)})`,
      `edgeCommands: ${String(session.pendingEdgeCount)}/32 (max ${String(session.maximumPendingEdgeCount)})`,
      `commandRttMs: ${session.lastCommandRoundTripMilliseconds.toFixed(2)}`,
    ].join("\n");
  }

  async function requestState(
    path: string,
    method = "GET",
    body?: unknown,
  ): Promise<RuntimeBrowserState> {
    const response = await fetch(path, {
      method,
      ...(body === undefined
        ? {}
        : {
            body: JSON.stringify(body),
            headers: { "Content-Type": "application/json" },
          }),
    });
    if (!response.ok) {
      const detail = (await response.json().catch(() => null)) as {
        readonly error?: unknown;
      } | null;
      const reason =
        typeof detail?.error === "string" ? `: ${detail.error}` : "";
      throw new Error(
        `${method} ${path} failed with ${String(response.status)}${reason}`,
      );
    }
    return (await response.json()) as RuntimeBrowserState;
  }

  function recordActionRejection(error: unknown): void {
    lastActionRejection =
      error instanceof GameSessionError
        ? `${error.code}: ${error.message}`
        : error instanceof Error
          ? error.message
          : String(error);
    recordCommittedEvents([
      lastActionRejection.includes("CombatRejected")
        ? "CombatRejected"
        : "ActionRejected",
    ]);
    renderReadout(current);
  }

  function recordCommittedEvents(events: readonly string[]): void {
    appendPresentationEvents(eventHistory, events);
    eventKindOverflow ||= !observePresentationEventKinds(
      observedEventKinds,
      events,
    );
  }

  function normalizeDegrees(value: number): number {
    return ((((value + 180) % 360) + 360) % 360) - 180;
  }

  function normalizeHostPreferences(
    preferences: LoadingBayHostPresentationPreferences | undefined,
  ): LoadingBayHostPresentationPreferences {
    return {
      mouseSensitivity: boundedPreference(
        preferences?.mouseSensitivity,
        0.25,
        2,
        1,
      ),
      invertY: preferences?.invertY ?? false,
      sfxVolume: boundedPreference(preferences?.sfxVolume, 0, 1, 1),
      flashIntensity: boundedPreference(preferences?.flashIntensity, 0, 1, 1),
      telemetryVisible: preferences?.telemetryVisible ?? true,
    };
  }

  function boundedPreference(
    value: number | undefined,
    minimum: number,
    maximum: number,
    fallback: number,
  ): number {
    return value !== undefined && Number.isFinite(value)
      ? Math.min(maximum, Math.max(minimum, value))
      : fallback;
  }

  function includesEvery(
    value: string | undefined,
    expected: readonly string[],
  ): boolean {
    const values = new Set((value ?? "").split(",").filter(Boolean));
    return expected.every((candidate) => values.has(candidate));
  }

  function meshFingerprint(state: RuntimeBrowserState): string {
    return state.voxelMeshes
      .map((mesh) => `${mesh.chunk.join(",")}:${mesh.contentHash}`)
      .join("|");
  }

  function voxelFingerprint(state: RuntimeBrowserState): {
    readonly revision: number;
    readonly authorityHash: string;
    readonly navigationHash: string;
    readonly probePathLength: number;
    readonly solidCount: number;
    readonly meshHash: string;
  } {
    return {
      revision: state.voxelRevision,
      authorityHash: state.voxelAuthorityHash,
      navigationHash: state.voxelNavigationHash,
      probePathLength: state.voxelProbePathLength,
      solidCount: state.voxelSolidCount,
      meshHash: meshFingerprint(state),
    };
  }

  function delay(milliseconds: number): Promise<void> {
    return new Promise((resolve) =>
      globalThis.setTimeout(resolve, milliseconds),
    );
  }

  function requiredElement<T extends Element>(
    id: string,
    constructor: { new (): T },
  ): T {
    const element = document.getElementById(id);
    if (!(element instanceof constructor)) {
      throw new Error(`missing required element #${id}`);
    }
    return element;
  }

  function vectorChanged(
    currentValue: readonly [number, number, number],
    previousValue: readonly [number, number, number],
    threshold: number,
  ): boolean {
    return (
      Math.abs(currentValue[0] - previousValue[0]) > threshold ||
      Math.abs(currentValue[1] - previousValue[1]) > threshold ||
      Math.abs(currentValue[2] - previousValue[2]) > threshold
    );
  }
}
