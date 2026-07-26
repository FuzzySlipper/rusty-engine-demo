import { mountRendererSurface } from "@rusty-engine/renderer-host";

import { SerializedActionQueue } from "./action-queue.js";
import { CoalescedLookInput } from "./coalesced-look.js";
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
} from "./projection.js";

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

const PRODUCT_EDIT_VOXEL = [4, 1, 6] as const;

export interface LoadingBayHostPresentationPreferences {
  readonly mouseSensitivity: number;
  readonly invertY: boolean;
  readonly sfxVolume: number;
  readonly telemetryVisible: boolean;
}

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
  readonly releaseInput: () => void;
  readonly restart: () => Promise<void>;
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
  });
  presentationFeedback.setAudioLevel(hostPreferences.sfxVolume);
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
      door?.translation?.[1] === 4 &&
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
    const pickupProofPassed = await proveWorldPickups();
    document.body.dataset.pickups = pickupProofPassed ? "pass" : "fail";
    await performRestart();
    const voxelEditFirstFight = await defeatEnemiesForSmoke();
    const voxelBefore = voxelFingerprint(current);
    let staleRejected = false;
    try {
      await requestState("/api/voxel-edit", "POST", {
        expectedRevision: current.voxelRevision + 1,
        persistToProject: false,
        edits: [{ kind: "clear", address: PRODUCT_EDIT_VOXEL }],
      });
    } catch {
      staleRejected = true;
    }
    const afterRejectedEdit = await requestState("/api/state");
    const rejectedUnchanged =
      staleRejected &&
      JSON.stringify(voxelFingerprint(afterRejectedEdit)) ===
        JSON.stringify(voxelBefore);
    current = afterRejectedEdit;
    await performVoxelEdit({ kind: "clear", address: PRODUCT_EDIT_VOXEL });
    const clearReceipt = current.voxelEditReceipt;
    const editBecameVisibleAndNavigable =
      clearReceipt?.acceptedRevision === 1 &&
      clearReceipt.changedVoxels === 1 &&
      current.voxelRevision === 1 &&
      current.voxelSolidCount === voxelBefore.solidCount - 1 &&
      current.voxelAuthorityHash !== voxelBefore.authorityHash &&
      current.voxelNavigationHash !== voxelBefore.navigationHash &&
      current.voxelProbePathLength < voxelBefore.probePathLength &&
      meshFingerprint(current) !== voxelBefore.meshHash &&
      current.generatedEnvironment === null &&
      surface.snapshot().includes("generated-room-chunk");
    const clearedBulkheadOpened = await openProgressionBulkhead();
    const clearedPassage = await walkPlayerPath([
      [3.5, 5.5],
      [4.5, 5.5],
      [4.5, 7.5],
    ]);
    await performRestart();
    const voxelEditSecondFight = await defeatEnemiesForSmoke();
    const restoredBulkheadOpened = await openProgressionBulkhead();
    const blockedByRestoredVoxel = !(await walkPlayerPath([
      [3.5, 5.5],
      [4.5, 5.5],
      [4.5, 7.5],
    ]));
    await performRestart();
    const voxelEditPassed =
      editBecameVisibleAndNavigable &&
      voxelEditFirstFight &&
      voxelEditSecondFight &&
      clearedBulkheadOpened &&
      clearedPassage &&
      restoredBulkheadOpened &&
      blockedByRestoredVoxel;
    document.body.dataset.voxelEditEvidence = [
      editBecameVisibleAndNavigable,
      voxelEditFirstFight,
      voxelEditSecondFight,
      clearedBulkheadOpened,
      clearedPassage,
      restoredBulkheadOpened,
      blockedByRestoredVoxel,
    ].join(":");
    document.body.dataset.voxelEdit = voxelEditPassed ? "pass" : "fail";
    document.body.dataset.voxelRejection = rejectedUnchanged ? "pass" : "fail";
    document.body.dataset.voxelCollision =
      clearedPassage && blockedByRestoredVoxel ? "pass" : "fail";

    await presentationFeedback.activateAudio();
    primaryFireHeld = true;
    await performInputIntent([0, 0]);
    await presentationFeedback.settled();
    const resetStartedWithConcreteTransients =
      playerMotionState.dataset.animationPulse === "arc-pistol-attack" &&
      Number(feedbackLayer.dataset.activeEffects ?? "0") > 0 &&
      Number(feedbackAudioStatus.dataset.activeSounds ?? "0") > 0;
    document.body.dataset.feedbackResetStart = [
      playerMotionState.dataset.animationPulse ?? "none",
      feedbackLayer.dataset.activeEffects ?? "none",
      feedbackAudioStatus.dataset.activeSounds ?? "none",
    ].join(":");
    await performRestart();
    await presentationFeedback.settled();
    const resetCuesBelongToFreshSimulation = current.presentation.cues.every(
      (cue) =>
        cue.kind === "movement" ||
        cue.kind === "enemyAlert" ||
        cue.kind === "enemyAttack" ||
        cue.kind === "enemyAttackMissed" ||
        cue.kind === "damage",
    );
    const resetAnimationStates = feedbackLayer.dataset.animationStates ?? "";
    const resetEnemyPostureRebuilt =
      includesEvery(resetAnimationStates, ["4:idle", "5:idle"]) ||
      includesEvery(resetAnimationStates, ["4:alert", "5:alert"]);
    const resetFeedbackRebuilt =
      resetStartedWithConcreteTransients &&
      resetCuesBelongToFreshSimulation &&
      playerMotionState.dataset.animationPulse !== "arc-pistol-attack" &&
      includesEvery(feedbackLayer.dataset.animationStates, [
        "1:idle",
        "3:closed",
      ]) &&
      resetEnemyPostureRebuilt;
    document.body.dataset.feedbackResetResult = [
      current.presentation.cues.length,
      feedbackLayer.dataset.activeEffects ?? "none",
      feedbackAudioStatus.dataset.activeSounds ?? "none",
      document.querySelector("[data-animation-pulse]") === null,
      playerMotionState.dataset.animationPulse ?? "none",
      feedbackLayer.dataset.animationStates ?? "none",
      resetCuesBelongToFreshSimulation,
    ].join(":");
    document.body.dataset.feedbackReset = resetFeedbackRebuilt
      ? "pass"
      : "fail";
    document.body.dataset.feedbackConcreteReset = resetFeedbackRebuilt
      ? "pass"
      : "fail";
    const initialPlayerPosition = current.player.position;
    const initialPlayerYaw = current.player.yawDegrees;
    const heldCode = current.player.bindings.moveForward;
    window.dispatchEvent(new KeyboardEvent("keydown", { code: heldCode }));
    await delay(1_200);
    window.dispatchEvent(new KeyboardEvent("keyup", { code: heldCode }));
    await performInputIntent([0, 0]);
    const playerMoved = vectorChanged(
      current.player.position,
      initialPlayerPosition,
      0.01,
    );
    const playerBlocked = current.playerMotionState === "blocked";
    const releasedPlayerPosition = current.player.position;
    await delay(current.player.moveStepSeconds * 2_000);
    await performInputIntent([0, 0]);
    current = await requestState("/api/state");
    const playerStopped = !vectorChanged(
      current.player.position,
      releasedPlayerPosition,
      0.000_001,
    );
    document.body.dataset.heldInput =
      playerMoved && playerBlocked && playerStopped ? "pass" : "fail";
    const initialPlayerPitch = current.player.pitchDegrees;
    window.dispatchEvent(
      new MouseEvent("mousemove", { movementX: 20, movementY: -10 }),
    );
    const [previewYawUnits, previewPitchUnits] = localLookOffset.pendingUnits;
    const localLookPreviewed =
      previewYawUnits !== 0 &&
      previewPitchUnits !== 0 &&
      Math.abs(previewYawUnits) <= 2 &&
      Math.abs(previewPitchUnits) <= 2;
    await lookInput.settled();
    const [settledYawUnits, settledPitchUnits] = localLookOffset.pendingUnits;
    const localLookReconciled =
      settledYawUnits === 0 && settledPitchUnits === 0;
    const localLookPresentationPassed =
      localLookPreviewed && localLookReconciled;
    document.body.dataset.localLookOffset = localLookPresentationPassed
      ? "pass"
      : "fail";
    document.body.dataset.localLookEvidence = [
      previewYawUnits,
      previewPitchUnits,
      settledYawUnits,
      settledPitchUnits,
    ].join(":");
    const playerLooked =
      normalizeDegrees(current.player.yawDegrees - initialPlayerYaw) < 0 &&
      current.player.pitchDegrees > initialPlayerPitch;
    const combatBulkheadOpenedBeforeProbe = await openProgressionBulkhead();
    await delay(100);
    current = await requestState("/api/state");
    const enemyCombatEvidence = [
      combatBulkheadOpenedBeforeProbe,
      eventHistory.includes("EnemyAlerted") ||
        includesEvery(feedbackLayer.dataset.animationPulses, [
          "enemy-alert-sight",
        ]),
      eventHistory.includes("EnemyAttackFired"),
      eventHistory.includes("EnemyAttackHit"),
      eventHistory.includes("DamageApplied"),
      current.player.currentHealth < current.player.maxHealth,
      includesEvery(feedbackLayer.dataset.animationPulses, [
        "enemy-alert-sight",
        "sentry-pulse-attack",
        "damage",
      ]),
    ];
    const enemyCombatProjected = enemyCombatEvidence.every(Boolean);
    document.body.dataset.enemyCombat = enemyCombatProjected ? "pass" : "fail";
    document.body.dataset.enemyCombatEvidence = enemyCombatEvidence.join(":");
    await aimAtEnemy(4);
    const healthBeforeCooldownProbe = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.currentHealth;
    const ammoBeforeSinglePress = current.weapon.ammoRemaining;
    primaryFireHeld = true;
    await performInputIntent([0, 0]);
    const healthAfterFirstShot = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.currentHealth;
    const ammoAfterFirstShot = current.weapon.ammoRemaining;
    // Real pointer capture sends the press transition once. Let several fixed
    // ticks pass without fabricating a second `pressed` frame; the Rust loop
    // must keep semiautomatic fire edge-triggered while the held intent ages.
    await delay(120);
    const healthAfterRepeatedHeldFire = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.currentHealth;
    const ammoAfterRepeatedHeldFire = current.weapon.ammoRemaining;
    primaryFireHeld = false;
    await performInputIntent([0, 0]);
    const singlePressHeld =
      ammoAfterFirstShot === ammoBeforeSinglePress - 1 &&
      ammoAfterRepeatedHeldFire === ammoAfterFirstShot &&
      healthAfterRepeatedHeldFire === healthAfterFirstShot;
    document.body.dataset.cooldownEvidence = [
      healthBeforeCooldownProbe ?? "missing",
      healthAfterFirstShot ?? "missing",
      healthAfterRepeatedHeldFire ?? "missing",
      ammoBeforeSinglePress,
      ammoAfterFirstShot,
      ammoAfterRepeatedHeldFire,
    ].join(":");
    const movingTargetDamaged =
      (healthBeforeCooldownProbe !== undefined &&
        healthAfterFirstShot !== undefined &&
        healthAfterFirstShot < healthBeforeCooldownProbe) ||
      (await damageEnemyTo(4, 40));
    const yawBeforeRecovery = current.player.yawDegrees;
    await enqueuePlayerAction({ kind: "look", yawDelta: 0.25, pitchDelta: 0 });
    const lookRecoveredAfterRejection =
      current.player.yawDegrees !== yawBeforeRecovery;
    const firstEnemyDefeated = await damageEnemyTo(4, 0);
    const secondEnemyDamaged = await damageEnemyTo(5, 40);
    await enqueuePlayerAction({ kind: "look", yawDelta: 0.25, pitchDelta: 0 });
    const secondEnemyDefeated = await damageEnemyTo(5, 0);
    const combatHit =
      firstEnemyDefeated &&
      secondEnemyDamaged &&
      secondEnemyDefeated &&
      eventHistory.includes("CombatHit");
    const combatBulkheadOpened = await openProgressionBulkhead();
    const openGateTraversed = await walkPlayerPath([
      [1.5, 9.5],
      [4.5, 9.5],
      [4.5, 12.5],
    ]);
    if (openGateTraversed) {
      await turnPlayerToward(
        4.5 - current.player.position[0],
        10.5 - current.player.position[2],
      );
    }
    document.body.dataset.gatePassage = openGateTraversed ? "pass" : "fail";
    const queueRecovered =
      singlePressHeld &&
      lookRecoveredAfterRejection &&
      combatBulkheadOpened &&
      openGateTraversed;
    document.body.dataset.queueEvidence = [
      singlePressHeld,
      lookRecoveredAfterRejection,
      openGateTraversed,
    ].join(":");
    document.body.dataset.queueRecovery = queueRecovered ? "pass" : "fail";
    const cooldownRecovered = singlePressHeld && firstEnemyDefeated;
    document.body.dataset.cooldown = cooldownRecovered ? "pass" : "fail";
    await enqueueInteraction(7);
    await presentationFeedback.settled();
    const beaconActivated =
      current.extractionBeacon?.state === "active" &&
      current.extractionBeacon.activatedBy === current.player.id &&
      eventHistory.includes("ExtractionBeaconActivated") &&
      beaconState.dataset.state === "active" &&
      beaconState.dataset.posture === "active" &&
      surface.snapshot().includes("extraction-beacon");
    document.body.dataset.beaconActivation = beaconActivated ? "pass" : "fail";
    const dryFirePassed = await proveDryFire();
    document.body.dataset.dryFire = dryFirePassed ? "pass" : "fail";
    await presentationFeedback.settled();
    const door = current.projection.find((node) => node.id === 3);
    const gameplayPassed =
      current.encounterState === "cleared" &&
      current.doorState === "open" &&
      door?.translation?.[1] === 4 &&
      current.enemies.every((enemy) => enemy.state === "defeated") &&
      playerMoved &&
      playerBlocked &&
      playerStopped &&
      playerLooked &&
      localLookPresentationPassed &&
      document.body.dataset.entityOcclusion === "pass" &&
      enemyCombatProjected &&
      movingTargetDamaged &&
      queueRecovered &&
      cooldownRecovered &&
      current.generatedEnvironment?.seed === 4 &&
      combatHit &&
      dryFirePassed &&
      openGateTraversed &&
      current.enemies.every((enemy) => enemy.currentHealth === 0) &&
      beaconActivated &&
      eventHistory.includes("CombatHit") &&
      eventHistory.includes("DamageApplied") &&
      current.voxelMeshes.length === 1 &&
      surface.snapshot().includes("loading-bay-exit") &&
      surface.snapshot().includes("generated-room-chunk");
    const feedbackFamiliesPassed =
      includesEvery(feedbackLayer.dataset.animationPulses, [
        "movement",
        "blocked",
        "encounter-activated",
        "enemy-alert-sight",
        "sentry-pulse-attack",
        "arc-pistol-attack",
        "arc-pistol-dry",
        "damage",
        "defeat",
        "drop-materialized",
        "open",
        "active",
      ]) &&
      includesEvery(feedbackLayer.dataset.particleKinds, [
        "movement",
        "blocked",
        "muzzle",
        "dry",
        "impact",
        "defeat",
        "pickup",
        "door",
        "beacon",
      ]) &&
      includesEvery(feedbackLayer.dataset.billboardValues, [
        "ENEMY ALERT",
        "BLOCKED",
        "EMPTY",
        "-60",
        "DEFEATED",
        "DROP +1 supply/med-patch",
        "DROP +20 ammo/energy-cell",
        "EXIT OPEN",
        "EXTRACTION ONLINE",
      ]) &&
      Number(feedbackLayer.dataset.activeEffects ?? "0") <= 24;
    document.body.dataset.feedbackFamilies = feedbackFamiliesPassed
      ? "pass"
      : "fail";
    document.body.dataset.feedbackEvidence = [
      feedbackLayer.dataset.animationPulses ?? "",
      feedbackLayer.dataset.particleKinds ?? "",
      feedbackLayer.dataset.billboardValues ?? "",
    ].join("|");
    const audioFeedbackPassed =
      Number(feedbackAudioStatus.dataset.attempted ?? "0") > 0 &&
      Number(feedbackAudioStatus.dataset.scheduled ?? "0") > 0 &&
      includesEvery(feedbackAudioStatus.dataset.soundKinds, [
        "beacon",
        "shot",
        "hit",
        "sidearmShot",
        "dryFire",
      ]);
    document.body.dataset.audioFeedback = audioFeedbackPassed ? "pass" : "fail";

    latestMovement = { kind: "move", forward: -1, right: 0 };
    const droppedResponse = await session.sendInput({
      movement: [-1, 0],
      lookDelta: [0, 0],
      primaryFireHeld: false,
    });
    const droppedHadTransientCue = droppedResponse.presentation.cues.some(
      (cue) => cue.kind === "movement" || cue.kind === "movementBlocked",
    );
    latestMovement = { kind: "move", forward: 0, right: 0 };
    const neutralResponse = await session.sendInput({
      movement: [0, 0],
      lookDelta: [0, 0],
      primaryFireHeld: false,
    });
    const neutralSequence = neutralResponse.input.acknowledgedSequence;
    const refreshed = await requestState("/api/state");
    const droppedDeliverySafe =
      droppedHadTransientCue &&
      refreshed.presentation.cues.length === 0 &&
      neutralResponse.input.consumedSequence === neutralSequence &&
      refreshed.input.consumedSequence === neutralSequence;
    document.body.dataset.feedbackDropEvidence = [
      droppedHadTransientCue,
      refreshed.presentation.cues.length,
      neutralResponse.input.consumedSequence,
      refreshed.input.consumedSequence,
      neutralSequence,
    ].join(":");
    current = refreshed;
    const refreshFrame = projection.apply(current);
    applyRendererFrame(refreshFrame);
    applyPresentationCamera();
    renderReadout(current);
    void applyPresentationFeedback(false, refreshFrame.ops.length);
    await enqueuePlayerAction({ kind: "move", forward: -1, right: 0 });
    await presentationFeedback.settled();
    const restartStartedWithConcreteTransients =
      playerMotionState.dataset.animationPulse !== undefined &&
      Number(feedbackLayer.dataset.activeEffects ?? "0") > 0 &&
      Number(feedbackAudioStatus.dataset.activeSounds ?? "0") > 0;
    current = await requestState("/api/state");
    const restartFrame = projection.apply(current);
    applyRendererFrame(restartFrame);
    applyPresentationCamera();
    renderReadout(current);
    await applyPresentationFeedback(true, restartFrame.ops.length);
    const restartRebuilt =
      restartStartedWithConcreteTransients &&
      current.presentation.cues.length === 0 &&
      feedbackLayer.dataset.activeEffects === "0" &&
      feedbackAudioStatus.dataset.activeSounds === "0" &&
      document.querySelector("[data-animation-pulse]") === null &&
      feedbackLayer.dataset.lastCueCount === "0" &&
      includesEvery(feedbackLayer.dataset.animationStates, [
        "3:open",
        "4:defeated",
        "5:defeated",
      ]);
    document.body.dataset.feedbackConcreteRestartEvidence = [
      restartStartedWithConcreteTransients,
      current.presentation.cues.length,
      feedbackLayer.dataset.activeEffects ?? "none",
      feedbackAudioStatus.dataset.activeSounds ?? "none",
      document.querySelector("[data-animation-pulse]") === null,
      feedbackLayer.dataset.lastCueCount ?? "none",
      feedbackLayer.dataset.animationStates ?? "none",
    ].join(":");
    document.body.dataset.feedbackConcreteRestart = restartRebuilt
      ? "pass"
      : "fail";
    const feedbackDropPassed = droppedDeliverySafe && restartRebuilt;
    document.body.dataset.feedbackDrop = feedbackDropPassed ? "pass" : "fail";
    updateSessionDiagnostics();
    const rendererTelemetryPassed =
      rendererTelemetryRefreshObserved &&
      rendererTelemetryResetObserved &&
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
      gameplayPassed &&
      pickupProofPassed &&
      voxelEditPassed &&
      rejectedUnchanged &&
      resetFeedbackRebuilt &&
      feedbackFamiliesPassed &&
      audioFeedbackPassed &&
      feedbackDropPassed &&
      rendererTelemetryPassed &&
      sessionTransportPassed;
    smokeResult.dataset.status = passed ? "pass" : "fail";
    smokeResult.textContent = passed
      ? "PASS · Rust facts reached retained WebGL and disposable feedback"
      : "FAIL · Product proof did not converge";
    document.body.dataset.smokeStatus = passed ? "pass" : "fail";
  }

  return {
    dispose,
    interact: (target) => runUiAction(() => enqueueInteraction(target)),
    releaseInput: releaseCapturedInput,
    restart: () => runUiAction(performRestart),
    selectWeaponSlot: (slot) => runUiAction(() => enqueueWeaponSelection(slot)),
    setPaused: (paused) => runUiAction(() => performSetPaused(paused)),
    updatePreferences,
    useItem: (item) => runUiAction(() => performUseItem(item)),
  };

  async function performRestart(): Promise<void> {
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
    current = await session.sendEdge({
      kind: "restart",
      mode: "authoredBaseline",
    });
    eventHistory.length = 0;
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
    eventHistory.push(...state.lastEvents);
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
    eventHistory.push(...current.lastEvents);
    const frame = projection.apply(current);
    applyRendererFrame(frame);
    applyPresentationCamera();
    renderReadout(current);
    void applyPresentationFeedback(false, frame.ops.length);
    updateRendererStatus();
  }

  function renderReadout(state: RuntimeBrowserState): void {
    eventList.dataset.history = eventHistory.join(",");
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

  async function firePrimary(): Promise<void> {
    const action = resolvePointerButtonAction(0, current.player.bindings);
    if (action === null) {
      throw new Error("authored primary-fire binding did not resolve Mouse0");
    }
    await enqueueAttackAction(action);
  }

  async function proveDryFire(): Promise<boolean> {
    const startedWithSidearm = current.weapon.item === "weapon/arc-pistol";
    let acceptedShots = 0;
    while (current.weapon.ammoRemaining > 0 && acceptedShots < 64) {
      await firePrimary();
      acceptedShots += 1;
    }
    const rejectionsBefore = eventHistory.filter(
      (event) => event === "CombatRejectedNoAmmo",
    ).length;
    primaryFireHeld = true;
    await performInputIntent([0, 0]);
    const cue = current.presentation.cues.find(
      (candidate) => candidate.kind === "dryFire",
    );
    primaryFireHeld = false;
    await performInputIntent([0, 0]);
    await presentationFeedback.settled();
    const rejectionObserved =
      eventHistory.filter((event) => event === "CombatRejectedNoAmmo").length >
      rejectionsBefore;
    const feedbackObserved =
      cue?.kind === "dryFire" &&
      cue.weapon === "weapon/arc-pistol" &&
      cue.presentation === "arc-pistol" &&
      includesEvery(feedbackLayer.dataset.animationPulses, [
        "arc-pistol-dry",
      ]) &&
      includesEvery(feedbackLayer.dataset.particleKinds, ["dry"]) &&
      includesEvery(feedbackLayer.dataset.billboardValues, ["EMPTY"]) &&
      includesEvery(feedbackAudioStatus.dataset.soundKinds, ["dryFire"]);
    document.body.dataset.dryFireEvidence = [
      startedWithSidearm,
      acceptedShots,
      current.weapon.ammoRemaining,
      rejectionObserved,
      feedbackObserved,
    ].join(":");
    return (
      startedWithSidearm &&
      acceptedShots <= 40 &&
      current.weapon.ammoRemaining === 0 &&
      rejectionObserved &&
      feedbackObserved
    );
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

  async function proveWorldPickups(): Promise<boolean> {
    await presentationFeedback.activateAudio();
    const pickupIds = current.pickups.map((pickup) => pickup.id);
    const initiallyAvailable = current.pickups
      .filter((pickup) => pickup.state === "available")
      .map((pickup) => pickup.id);
    const initiallyDormant = current.pickups
      .filter((pickup) => pickup.state === "dormant")
      .map((pickup) => pickup.id);
    const startedAvailable =
      pickupIds.length === 10 &&
      JSON.stringify(initiallyAvailable) ===
        JSON.stringify([20, 21, 22, 23, 24, 25, 26, 28]) &&
      JSON.stringify(initiallyDormant) === JSON.stringify([33, 34]);
    const bayRusher = current.projection.find((node) => node.id === 4);
    const arcWarden = current.projection.find((node) => node.id === 5);
    const archetypesDistinct =
      bayRusher?.asset === "mesh/bay-rusher" &&
      arcWarden?.asset === "mesh/arc-warden" &&
      current.enemies.find((enemy) => enemy.id === 4)?.attackKind === "melee" &&
      current.enemies.find((enemy) => enemy.id === 5)?.attackKind ===
        "rangedHitscan" &&
      surface.snapshot().includes("sentry-alpha") &&
      surface.snapshot().includes("sentry-beta");
    document.body.dataset.enemyArchetypes = archetypesDistinct
      ? "pass"
      : "fail";
    const capacityWalked = await walkPlayerPath([
      [2.5, 2.5],
      [3.5, 2.5],
      [3.5, 3.5],
      [2.5, 3.5],
    ]);
    const closedBulkheadBlockedSight =
      current.doorAccess.some(
        (door) => door.id === 30 && door.state === "closed",
      ) &&
      current.enemies.find((enemy) => enemy.id === 4)?.combatPosture ===
        "sleeping";
    const closedEnemyPosition = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.position;
    const occlusionBulkheadOpened = await openProgressionBulkhead();
    await delay(350);
    current = await requestState("/api/state");
    const openedBulkheadRestoredSight =
      current.enemies.find((enemy) => enemy.id === 4)?.combatPosture !==
      "sleeping";
    const openedEnemyPosition = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.position;
    const openedBulkheadRestoredNavigation =
      closedEnemyPosition !== undefined &&
      openedEnemyPosition !== undefined &&
      vectorChanged(openedEnemyPosition, closedEnemyPosition, 0.001) &&
      eventHistory.includes("NavigationAdvanced");
    const entityOcclusionPassed =
      closedBulkheadBlockedSight &&
      occlusionBulkheadOpened &&
      openedBulkheadRestoredSight &&
      openedBulkheadRestoredNavigation;
    document.body.dataset.entityOcclusion = entityOcclusionPassed
      ? "pass"
      : "fail";
    document.body.dataset.entityOcclusionEvidence = [
      closedBulkheadBlockedSight,
      occlusionBulkheadOpened,
      openedBulkheadRestoredSight,
      openedBulkheadRestoredNavigation,
    ].join(":");
    const firstEnemyDefeated = await damageEnemyTo(4, 0);
    const secondEnemyDefeated = await damageEnemyTo(5, 0);
    const pickupFightPassed =
      firstEnemyDefeated &&
      secondEnemyDefeated &&
      current.player.vitalityState === "alive";
    await presentationFeedback.settled();
    const materializedDrops = current.pickups
      .filter((pickup) => pickup.state === "available")
      .map((pickup) => pickup.id)
      .filter((pickup) => pickup === 33 || pickup === 34);
    const dropFacts = eventHistory.filter(
      (event) => event === "EnemyDropMaterialized",
    ).length;
    const dropsMaterialized =
      JSON.stringify(materializedDrops) === JSON.stringify([33, 34]) &&
      current.projection.some(
        (node) => node.id === 33 && node.asset === "mesh/pickup-health",
      ) &&
      current.projection.some(
        (node) => node.id === 34 && node.asset === "mesh/pickup-ammunition",
      ) &&
      dropFacts === 2;
    document.body.dataset.enemyDrops = dropsMaterialized ? "pass" : "fail";
    const energyAfterFight = inventoryQuantity("ammo/energy-cell");
    const walked =
      capacityWalked &&
      (await walkPlayerPath([
        [4.5, 3.5],
        [4.5, 2.5],
        [5.5, 2.5],
        [6.5, 2.5],
        [6.5, 3.5],
        [5.5, 3.5],
      ]));
    await presentationFeedback.settled();
    const collected = current.pickups
      .filter((pickup) => pickup.state === "collected")
      .map((pickup) => pickup.id);
    const available = current.pickups
      .filter((pickup) => pickup.state === "available")
      .map((pickup) => pickup.id);
    const inventoryAndArmorExact =
      inventoryQuantity("ammo/energy-cell") === energyAfterFight &&
      inventoryQuantity("ammo/scatter-shell") === 20 &&
      inventoryQuantity("weapon/breach-scattergun") === 1 &&
      inventoryQuantity("weapon/rivet-carbine") === 1 &&
      inventoryQuantity("supply/med-patch") === 2 &&
      inventoryQuantity("armor/impact-vest") === 0 &&
      inventoryQuantity("key/maintenance-pass") === 1 &&
      current.player.armor === 100 &&
      current.player.armor === current.player.maxArmor;
    const worldExact =
      JSON.stringify(collected) ===
        JSON.stringify([20, 22, 23, 24, 25, 26, 28]) &&
      JSON.stringify(available) === JSON.stringify([21, 33, 34]) &&
      !current.projection.some((node) => collected.includes(node.id)) &&
      [21, 33, 34].every((pickup) =>
        current.projection.some((node) => node.id === pickup),
      );
    const rejectionExact = eventHistory.includes(
      "PickupRejectedQuantityOverflow",
    );
    const factsProjected =
      eventHistory.filter((event) => event === "PickupCollected").length >= 7;
    const cueProjected =
      includesEvery(feedbackLayer.dataset.animationPulses, ["pickup"]) &&
      includesEvery(feedbackLayer.dataset.particleKinds, ["pickup"]) &&
      includesEvery(feedbackAudioStatus.dataset.soundKinds, ["pickup"]);
    const spreadSelection = resolveKeyboardAction(
      "Digit2",
      current.player.bindings,
    );
    if (spreadSelection?.kind === "selectWeaponSlot") {
      await enqueueWeaponSelection(spreadSelection.slot);
    }
    const selectedSpreadWeapon =
      current.weapon.item === "weapon/breach-scattergun" &&
      current.weapon.ammunition === "ammo/scatter-shell" &&
      current.weapon.ammoRemaining === 20 &&
      current.inventory?.equippedWeapon === "weapon/breach-scattergun" &&
      current.inventory.weapons.find((weapon) => weapon.slot === 1)
        ?.selected === true;
    const spreadAmmoBefore = current.weapon.ammoRemaining;
    primaryFireHeld = true;
    await performInputIntent([0, 0]);
    const spreadCue = current.presentation.cues.find(
      (cue) => cue.kind === "attack",
    );
    primaryFireHeld = false;
    await performInputIntent([0, 0]);
    const spreadPassed =
      spreadCue?.kind === "attack" &&
      spreadCue.weapon === "weapon/breach-scattergun" &&
      spreadCue.attackMode === "spread" &&
      spreadCue.rayCount === 7 &&
      current.weapon.ammoRemaining === spreadAmmoBefore - 1;

    const automaticSelection = resolveKeyboardAction(
      "Digit3",
      current.player.bindings,
    );
    if (automaticSelection?.kind === "selectWeaponSlot") {
      await enqueueWeaponSelection(automaticSelection.slot);
    }
    const automaticAmmoBefore = current.weapon.ammoRemaining;
    let automaticShotCount = 0;
    primaryFireHeld = true;
    for (let frame = 0; frame < 5; frame += 1) {
      await performInputIntent([0, 0]);
      automaticShotCount += current.presentation.cues.filter(
        (cue) =>
          cue.kind === "attack" &&
          cue.weapon === "weapon/rivet-carbine" &&
          cue.attackMode === "automatic",
      ).length;
    }
    primaryFireHeld = false;
    await performInputIntent([0, 0]);
    await presentationFeedback.settled();
    const automaticAmmunitionSpent =
      automaticAmmoBefore - current.weapon.ammoRemaining;
    const automaticPassed =
      current.weapon.item === "weapon/rivet-carbine" &&
      current.inventory?.equippedWeapon === "weapon/rivet-carbine" &&
      current.inventory.weapons.find((weapon) => weapon.slot === 2)
        ?.selected === true &&
      automaticShotCount >= 1 &&
      automaticAmmunitionSpent >= 2 &&
      automaticAmmunitionSpent <= 5;
    const weaponFeedbackPassed = includesEvery(
      feedbackAudioStatus.dataset.soundKinds,
      ["spreadShot", "automaticShot"],
    );
    document.body.dataset.pickupEvidence = [
      pickupIds.join(","),
      initiallyAvailable.join(","),
      initiallyDormant.join(","),
      collected.join(","),
      available.join(","),
      (current.inventory?.stacks ?? [])
        .map((stack) => `${stack.item}:${String(stack.quantity)}`)
        .join(","),
      `${String(current.player.armor)}/${String(current.player.maxArmor)}`,
      pickupFightPassed,
      archetypesDistinct,
      materializedDrops.join(","),
      dropFacts,
      rejectionExact,
      cueProjected,
      selectedSpreadWeapon,
      spreadPassed,
      automaticPassed,
      weaponFeedbackPassed,
    ].join("|");
    return (
      startedAvailable &&
      archetypesDistinct &&
      entityOcclusionPassed &&
      pickupFightPassed &&
      dropsMaterialized &&
      walked &&
      inventoryAndArmorExact &&
      worldExact &&
      rejectionExact &&
      factsProjected &&
      cueProjected &&
      selectedSpreadWeapon &&
      spreadPassed &&
      automaticPassed &&
      weaponFeedbackPassed
    );
  }

  async function defeatEnemiesForSmoke(): Promise<boolean> {
    return (
      (await openProgressionBulkhead()) &&
      (await damageEnemyTo(4, 0)) &&
      (await damageEnemyTo(5, 0))
    );
  }

  async function openProgressionBulkhead(): Promise<boolean> {
    if (inventoryQuantity("key/maintenance-pass") === 0) {
      const collected = await walkPlayerPath([
        [1.5, 3.5],
        [2.5, 3.5],
      ]);
      if (!collected || inventoryQuantity("key/maintenance-pass") !== 1) {
        return false;
      }
    }
    if (
      current.doorAccess.some((door) => door.id === 30 && door.state === "open")
    ) {
      return true;
    }
    if (!(await walkPlayerPath([[3.5, 4.5]]))) {
      return false;
    }
    if (current.interaction?.target !== 30) {
      return false;
    }
    await enqueueInteraction(30);
    return current.doorAccess.some(
      (door) => door.id === 30 && door.state === "open",
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
    const action = resolveKeyboardAction(
      current.player.bindings.moveForward,
      current.player.bindings,
    );
    if (action?.kind !== "move") {
      throw new Error(
        "authored move-forward binding did not resolve to movement",
      );
    }
    latestMovement = action;
    for (let step = 0; step < maxSteps; step += 1) {
      const offsetX = target[0] - current.player.position[0];
      const offsetZ = target[1] - current.player.position[2];
      if (Math.hypot(offsetX, offsetZ) <= 0.25) {
        latestMovement = { kind: "move", forward: 0, right: 0 };
        await performInputIntent([0, 0]);
        return true;
      }
      const before = current.player.position;
      await performInputIntent([0, 0]);
      if (!vectorChanged(current.player.position, before, 0.000_001)) {
        latestMovement = { kind: "move", forward: 0, right: 0 };
        await performInputIntent([0, 0]);
        return false;
      }
    }
    latestMovement = { kind: "move", forward: 0, right: 0 };
    await performInputIntent([0, 0]);
    return false;
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
    eventHistory.push(
      lastActionRejection.includes("CombatRejected")
        ? "CombatRejected"
        : "ActionRejected",
    );
    renderReadout(current);
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
