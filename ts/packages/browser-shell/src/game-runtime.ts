import { mountRendererSurface } from "@rusty-engine/renderer-host";

import { SerializedActionQueue } from "./action-queue.js";
import { CoalescedLookInput } from "./coalesced-look.js";
import { LoadingBayGameSession } from "./game-session.js";
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

export interface LoadingBayPresentationSnapshot {
  readonly ammoCapacity: number;
  readonly ammoRemaining: number;
  readonly encounterState: string;
  readonly events: readonly string[];
  readonly headingDegrees: number;
}

export interface LoadingBayGameOptions {
  readonly onProjection?: (snapshot: LoadingBayPresentationSnapshot) => void;
}

export interface LoadingBayGameHandle {
  readonly dispose: () => Promise<void>;
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
  session.setStateListener(applySessionState);
  session.setFailureListener((error) => {
    if (!disposed) {
      if (error.retry === "reconnect" || error.code === "sessionClosed") {
        clearActiveInput();
      }
      recordActionRejection(error);
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
        document.pointerLockElement !== canvas
      ) {
        return;
      }
      const action = resolvePointerAction(
        event.movementX,
        event.movementY,
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
    const clearedPassage = await walkPlayerPath([
      [3.5, 5.5],
      [4.5, 5.5],
      [4.5, 7.5],
    ]);
    await performRestart();
    const blockedByRestoredVoxel = !(await walkPlayerPath([
      [3.5, 5.5],
      [4.5, 5.5],
      [4.5, 7.5],
    ]));
    await performRestart();
    const voxelEditPassed =
      editBecameVisibleAndNavigable && clearedPassage && blockedByRestoredVoxel;
    document.body.dataset.voxelEdit = voxelEditPassed ? "pass" : "fail";
    document.body.dataset.voxelRejection = rejectedUnchanged ? "pass" : "fail";
    document.body.dataset.voxelCollision =
      clearedPassage && blockedByRestoredVoxel ? "pass" : "fail";

    await presentationFeedback.activateAudio();
    primaryFireHeld = true;
    await performInputIntent([0, 0]);
    await presentationFeedback.settled();
    const resetStartedWithConcreteTransients =
      playerMotionState.dataset.animationPulse === "attack" &&
      Number(feedbackLayer.dataset.activeEffects ?? "0") > 0 &&
      Number(feedbackAudioStatus.dataset.activeSounds ?? "0") > 0;
    document.body.dataset.feedbackResetStart = [
      playerMotionState.dataset.animationPulse ?? "none",
      feedbackLayer.dataset.activeEffects ?? "none",
      feedbackAudioStatus.dataset.activeSounds ?? "none",
    ].join(":");
    await performRestart();
    const resetCuesBelongToFreshSimulation = current.presentation.cues.every(
      (cue) => cue.kind === "movement",
    );
    const resetFeedbackRebuilt =
      resetStartedWithConcreteTransients &&
      resetCuesBelongToFreshSimulation &&
      playerMotionState.dataset.animationPulse !== "attack" &&
      includesEvery(feedbackLayer.dataset.animationStates, [
        "1:idle",
        "3:closed",
        "4:moving",
      ]);
    document.body.dataset.feedbackResetResult = [
      current.presentation.cues.length,
      feedbackLayer.dataset.activeEffects ?? "none",
      feedbackAudioStatus.dataset.activeSounds ?? "none",
      document.querySelector("[data-animation-pulse]") === null,
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
    await delay(current.player.moveStepSeconds * 8_000);
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
    const initialEnemyPosition = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.position;
    await delay(100);
    current = await requestState("/api/state");
    const movingEnemyPosition = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.position;
    const movingTargetAdvanced =
      initialEnemyPosition !== undefined &&
      movingEnemyPosition !== undefined &&
      vectorChanged(movingEnemyPosition, initialEnemyPosition, 0.001);
    await aimAtEnemy(4);
    const healthBeforeCooldownProbe = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.currentHealth;
    const rejectionsBeforeCooldown = eventHistory.filter(
      (event) => event === "CombatRejectedCooldown",
    ).length;
    primaryFireHeld = true;
    await performInputIntent([0, 0]);
    const healthAfterFirstShot = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.currentHealth;
    await performInputIntent([0, 0]);
    const healthAfterRepeatedHeldFire = current.enemies.find(
      (enemy) => enemy.id === 4,
    )?.currentHealth;
    primaryFireHeld = false;
    await performInputIntent([0, 0]);
    const rejectionsAfterCooldown = eventHistory.filter(
      (event) => event === "CombatRejectedCooldown",
    ).length;
    const cooldownRejected = rejectionsAfterCooldown > rejectionsBeforeCooldown;
    document.body.dataset.cooldownEvidence = [
      healthBeforeCooldownProbe ?? "missing",
      healthAfterFirstShot ?? "missing",
      healthAfterRepeatedHeldFire ?? "missing",
      rejectionsBeforeCooldown,
      rejectionsAfterCooldown,
    ].join(":");
    const movingTargetDamaged =
      cooldownRejected || (await damageEnemyTo(4, 40));
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
      cooldownRejected && lookRecoveredAfterRejection && openGateTraversed;
    document.body.dataset.queueEvidence = [
      cooldownRejected,
      lookRecoveredAfterRejection,
      openGateTraversed,
    ].join(":");
    document.body.dataset.queueRecovery = queueRecovered ? "pass" : "fail";
    const cooldownRecovered = cooldownRejected && firstEnemyDefeated;
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
      movingTargetAdvanced &&
      movingTargetDamaged &&
      queueRecovered &&
      cooldownRecovered &&
      current.generatedEnvironment?.seed === 4 &&
      combatHit &&
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
        "attack",
        "damage",
        "defeat",
        "open",
        "active",
      ]) &&
      includesEvery(feedbackLayer.dataset.particleKinds, [
        "movement",
        "blocked",
        "muzzle",
        "impact",
        "defeat",
        "door",
        "beacon",
      ]) &&
      includesEvery(feedbackLayer.dataset.billboardValues, [
        "BLOCKED",
        "-60",
        "DEFEATED",
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
      Number(feedbackAudioStatus.dataset.scheduled ?? "0") > 0;
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

  return { dispose };

  async function performRestart(): Promise<void> {
    heldMovement.clear(false);
    lookGeneration += 1;
    lookInput.clear();
    clearLocalLookPresentation();
    latestMovement = { kind: "move", forward: 0, right: 0 };
    primaryFireHeld = false;
    session.neutralizeInput();
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

  function applySessionState(state: RuntimeBrowserState): void {
    if (disposed) {
      return;
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
    weaponState.textContent = `${String(state.weapon.damage)} DMG · ${String(state.weapon.ammoRemaining)}/${String(state.weapon.ammoCapacity)} AMMO`;
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
        const name = document.createElement("span");
        name.textContent = enemy.name;
        const status = document.createElement("strong");
        status.textContent = `${enemy.state.toUpperCase()} · ${String(enemy.currentHealth)}/${String(enemy.maxHealth)} HP`;
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
      encounterState: state.encounterState,
      events: [...eventHistory],
      headingDegrees: normalizeDegrees(state.player.yawDegrees),
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
    const pickupIds = current.pickups.map((pickup) => pickup.id);
    const startedAvailable =
      pickupIds.length === 7 &&
      current.pickups.every((pickup) => pickup.state === "available");
    const walked = await walkPlayerPath([
      [2.5, 2.5],
      [3.5, 2.5],
      [4.5, 2.5],
      [5.5, 2.5],
      [6.5, 2.5],
      [6.5, 3.5],
      [5.5, 3.5],
    ]);
    await presentationFeedback.settled();
    const collected = current.pickups
      .filter((pickup) => pickup.state === "collected")
      .map((pickup) => pickup.id);
    const available = current.pickups
      .filter((pickup) => pickup.state === "available")
      .map((pickup) => pickup.id);
    const inventoryExact =
      inventoryQuantity("ammo/energy-cell") === 200 &&
      inventoryQuantity("ammo/scatter-shell") === 12 &&
      inventoryQuantity("weapon/breach-scattergun") === 1 &&
      inventoryQuantity("supply/med-patch") === 2 &&
      inventoryQuantity("armor/impact-vest") === 1 &&
      inventoryQuantity("key/maintenance-pass") === 1;
    const worldExact =
      JSON.stringify(collected) === JSON.stringify([20, 22, 23, 24, 25, 26]) &&
      JSON.stringify(available) === JSON.stringify([21]) &&
      !current.projection.some((node) => collected.includes(node.id)) &&
      current.projection.some((node) => node.id === 21);
    const rejectionExact = eventHistory.includes(
      "PickupRejectedQuantityOverflow",
    );
    const factsProjected =
      eventHistory.filter((event) => event === "PickupCollected").length >= 6;
    const cueProjected =
      includesEvery(feedbackLayer.dataset.animationPulses, ["pickup"]) &&
      includesEvery(feedbackLayer.dataset.particleKinds, ["pickup"]) &&
      includesEvery(feedbackAudioStatus.dataset.soundKinds, ["pickup"]);
    document.body.dataset.pickupEvidence = [
      pickupIds.join(","),
      collected.join(","),
      available.join(","),
      (current.inventory?.stacks ?? [])
        .map((stack) => `${stack.item}:${String(stack.quantity)}`)
        .join(","),
      rejectionExact,
      cueProjected,
    ].join("|");
    return (
      startedAvailable &&
      walked &&
      inventoryExact &&
      worldExact &&
      rejectionExact &&
      factsProjected &&
      cueProjected
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
      error instanceof Error ? error.message : String(error);
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
