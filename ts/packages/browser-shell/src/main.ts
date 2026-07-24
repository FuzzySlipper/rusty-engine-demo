import { mountRendererSurface } from "@rusty-engine/renderer-host";

import "./style.css";
import { SerializedActionQueue } from "./action-queue.js";
import { HeldMovementInput } from "./held-movement.js";
import {
  BrowserPresentationFeedback,
} from "./presentation-feedback.js";
import {
  RuntimeProjectionAdapter,
  derivePlayerCameraPose,
  type RuntimeBrowserState,
  type RuntimePlayerBindings,
} from "./projection.js";

type ResolvedPlayerAction =
  | { readonly kind: "move"; readonly forward: number; readonly right: number }
  | { readonly kind: "look"; readonly yawDelta: number; readonly pitchDelta: number };
type ResolvedAttackAction = { readonly kind: "attack" };
type ResolvedInputAction = ResolvedPlayerAction | ResolvedAttackAction;
type VoxelEditOperation =
  | { readonly kind: "set"; readonly address: readonly [number, number, number]; readonly materialSlot: number }
  | { readonly kind: "clear"; readonly address: readonly [number, number, number] };

const PRODUCT_EDIT_VOXEL = [4, 1, 6] as const;

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
const environmentState = requiredElement("environment-state", HTMLElement);
const voxelState = requiredElement("voxel-state", HTMLElement);
const persistVoxelEdit = requiredElement("persist-voxel-edit", HTMLInputElement);
const eventList = requiredElement("event-list", HTMLOListElement);
const rendererStatus = requiredElement("renderer-status", HTMLElement);
const smokeResult = requiredElement("smoke-result", HTMLElement);
const feedbackLayer = requiredElement("feedback-layer", HTMLElement);
const feedbackAudioStatus = requiredElement("feedback-audio-status", HTMLElement);
const projection = new RuntimeProjectionAdapter();
const eventHistory: string[] = [];
const query = new URLSearchParams(location.search);
const smokeMode = query.has("smoke");
const reloadSmokeMode = query.has("reload-smoke");
const convertedSmokeMode = query.has("converted-smoke");
let actionRejectionCount = 0;
let lastActionRejection: string | null = null;
const actionQueue = new SerializedActionQueue(recordActionRejection);

let current = await requestState("/api/state");
const heldMovement = new HeldMovementInput({
  bindings: () => current.player.bindings,
  intervalMilliseconds: () => current.player.moveStepSeconds * 1_000,
  dispatch: enqueuePlayerAction,
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
const presentationFeedback = new BrowserPresentationFeedback({
  audioStatus: feedbackAudioStatus,
  layer: feedbackLayer,
  readState: () => current,
  surface,
});
renderReadout(current);
await applyPresentationFeedback(true, initialFrame.ops.length);
updateRendererStatus();

requiredElement("primary-fire", HTMLButtonElement).addEventListener("click", () => {
  void presentationFeedback.activateAudio();
  enqueueAttackAction({ kind: "attack" });
});
requiredElement("reset", HTMLButtonElement).addEventListener("click", () => {
  void presentationFeedback.activateAudio();
  heldMovement.clear();
  void perform("/api/reset");
});
requiredElement("run-motion", HTMLButtonElement).addEventListener("click", () => {
  void presentationFeedback.activateAudio();
  void perform("/api/motion-phase");
});
requiredElement("run-navigation", HTMLButtonElement).addEventListener("click", () => {
  void presentationFeedback.activateAudio();
  void perform("/api/navigation-phase");
});
requiredElement("activate-beacon", HTMLButtonElement).addEventListener("click", () => {
  void presentationFeedback.activateAudio();
  void perform("/api/extraction-beacon/activate");
});
requiredElement("remove-voxel", HTMLButtonElement).addEventListener("click", () => {
  void actionQueue.enqueue(() => performVoxelEdit({
    kind: "clear",
    address: PRODUCT_EDIT_VOXEL,
  }));
});
requiredElement("place-voxel", HTMLButtonElement).addEventListener("click", () => {
  void actionQueue.enqueue(() => performVoxelEdit({
    kind: "set",
    address: PRODUCT_EDIT_VOXEL,
    materialSlot: 3,
  }));
});

window.addEventListener("keydown", (event) => {
  const action = resolveKeyboardAction(event.code, current.player.bindings);
  if (action === null) {
    return;
  }
  void presentationFeedback.activateAudio();
  event.preventDefault();
  if (action.kind === "move") {
    heldMovement.press(event.code);
  } else if (!event.repeat) {
    enqueueResolvedAction(action);
  }
});
window.addEventListener("keyup", (event) => {
  if (heldMovement.release(event.code)) {
    event.preventDefault();
  }
});
window.addEventListener("blur", () => {
  heldMovement.clear();
});
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    heldMovement.clear();
  }
});
canvas.addEventListener("click", () => {
  void presentationFeedback.activateAudio();
  void canvas.requestPointerLock();
});
canvas.addEventListener("mousedown", (event) => {
  if (document.pointerLockElement !== canvas) {
    return;
  }
  const action = resolvePointerButtonAction(event.button, current.player.bindings);
  if (action !== null) {
    void presentationFeedback.activateAudio();
    event.preventDefault();
    enqueueAttackAction(action);
  }
});
window.addEventListener("mousemove", (event) => {
  if (!smokeMode && !convertedSmokeMode && document.pointerLockElement !== canvas) {
    return;
  }
  const action = resolvePointerAction(
    event.movementX,
    event.movementY,
    current.player.bindings,
  );
  if (action !== null) {
    enqueuePlayerAction(action);
  }
});

if (reloadSmokeMode) {
  surface.renderOnce();
  const door = current.projection.find((node) => node.id === 3);
  const reloadPostureRebuilt =
    current.encounterState === "cleared" &&
    current.doorState === "open" &&
    door?.translation?.[1] === 4 &&
    current.enemies.every((enemy) => enemy.state === "defeated") &&
    doorCaption.dataset.posture === "open" &&
    document.querySelector<HTMLElement>('[data-entity-id="4"]')?.dataset.posture === "defeated" &&
    document.querySelector<HTMLElement>('[data-entity-id="5"]')?.dataset.posture === "defeated" &&
    current.extractionBeacon?.state === "active" &&
    beaconState.dataset.posture === "active" &&
    includesEvery(feedbackLayer.dataset.animationStates, [
      "3:open",
      "4:defeated",
      "5:defeated",
      "7:active",
    ]);
  const reloadCuesCleared = current.presentation.cues.length === 0 &&
    feedbackLayer.dataset.lastCueCount === "0";
  const reloadPulsesCleared = document.querySelector("[data-animation-pulse]") === null;
  const reloadDomTargets = document.querySelectorAll(
    ".feedback-particle, .feedback-billboard",
  ).length;
  const reloadAudioTargets = Number(feedbackAudioStatus.dataset.activeSounds ?? "-1");
  const reloadPassed =
    reloadPostureRebuilt &&
    reloadCuesCleared &&
    reloadPulsesCleared &&
    reloadDomTargets === 0 &&
    reloadAudioTargets === 0 &&
    feedbackLayer.dataset.activeEffects === "0";
  document.body.dataset.reloadPosture = reloadPostureRebuilt ? "pass" : "fail";
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
  surface.renderOnce();
  const convertedAssetVisible =
    convertedAssetLoaded && surface.snapshot().includes("generated-room-chunk");
  const blockedByConvertedWall = !await walkPlayerPath([
    [1.5, 5.5],
    [4.5, 5.5],
    [4.5, 8.5],
  ]);
  await perform("/api/reset");

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
  const convertedCollisionPassed = blockedByConvertedWall && clearedWallTraversed;
  const passed =
    convertedAssetLoaded &&
    convertedAssetVisible &&
    convertedCollisionPassed &&
    convertedNavigationUpdated &&
    convertedEditApplied;
  document.body.dataset.convertedAsset = convertedAssetLoaded ? "pass" : "fail";
  document.body.dataset.convertedVisible = convertedAssetVisible ? "pass" : "fail";
  document.body.dataset.convertedCollision = convertedCollisionPassed ? "pass" : "fail";
  document.body.dataset.convertedNavigation = convertedNavigationUpdated ? "pass" : "fail";
  document.body.dataset.convertedEdit = convertedEditApplied ? "pass" : "fail";
  document.body.dataset.smokeStatus = passed ? "pass" : "fail";
  smokeResult.dataset.status = passed ? "pass" : "fail";
  smokeResult.textContent = passed
    ? "PASS · Converted voxel asset reached retained WebGL, collision, navigation, and live edits"
    : "FAIL · Converted voxel product proof did not converge";
} else if (smokeMode) {
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
  const rejectedUnchanged = staleRejected &&
    JSON.stringify(voxelFingerprint(afterRejectedEdit)) === JSON.stringify(voxelBefore);
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
    [1.5, 5.5],
    [4.5, 5.5],
    [4.5, 7.5],
  ]);
  await perform("/api/reset");
  const blockedByRestoredVoxel = !await walkPlayerPath([
    [1.5, 5.5],
    [4.5, 5.5],
    [4.5, 7.5],
  ]);
  await perform("/api/reset");
  const voxelEditPassed = editBecameVisibleAndNavigable && clearedPassage && blockedByRestoredVoxel;
  document.body.dataset.voxelEdit = voxelEditPassed ? "pass" : "fail";
  document.body.dataset.voxelRejection = rejectedUnchanged ? "pass" : "fail";
  document.body.dataset.voxelCollision = clearedPassage && blockedByRestoredVoxel ? "pass" : "fail";

  await presentationFeedback.activateAudio();
  await enqueueAttackAction({ kind: "attack" });
  await presentationFeedback.settled();
  const resetStartedWithConcreteTransients =
    playerMotionState.dataset.animationPulse === "attack" &&
    Number(feedbackLayer.dataset.activeEffects ?? "0") > 0 &&
    Number(feedbackAudioStatus.dataset.activeSounds ?? "0") > 0;
  await perform("/api/reset");
  const resetFeedbackRebuilt =
    resetStartedWithConcreteTransients &&
    current.presentation.cues.length === 0 &&
    feedbackLayer.dataset.activeEffects === "0" &&
    feedbackAudioStatus.dataset.activeSounds === "0" &&
    document.querySelector("[data-animation-pulse]") === null &&
    includesEvery(feedbackLayer.dataset.animationStates, ["1:idle", "3:closed", "4:moving"]);
  document.body.dataset.feedbackReset = resetFeedbackRebuilt ? "pass" : "fail";
  document.body.dataset.feedbackConcreteReset = resetFeedbackRebuilt ? "pass" : "fail";
  const initialPlayerPosition = current.player.position;
  const initialPlayerYaw = current.player.yawDegrees;
  const heldCode = current.player.bindings.moveForward;
  window.dispatchEvent(new KeyboardEvent("keydown", { code: heldCode }));
  await delay(current.player.moveStepSeconds * 8_000);
  window.dispatchEvent(new KeyboardEvent("keyup", { code: heldCode }));
  await actionQueue.settled();
  const playerMoved = current.player.position.some(
    (value, axis) => Math.abs(value - initialPlayerPosition[axis]!) > 0.01,
  );
  const playerBlocked = current.playerMotionState === "blocked";
  const releasedPlayerPosition = current.player.position;
  await delay(current.player.moveStepSeconds * 2_000);
  await actionQueue.settled();
  const playerStopped = current.player.position.every(
    (value, axis) => Math.abs(value - releasedPlayerPosition[axis]!) < 0.000_001,
  );
  document.body.dataset.heldInput = playerMoved && playerBlocked && playerStopped
    ? "pass"
    : "fail";
  window.dispatchEvent(new MouseEvent("mousemove", { movementX: 20, movementY: 0 }));
  await actionQueue.settled();
  const playerLooked = current.player.yawDegrees !== initialPlayerYaw;
  const initialEnemyPosition = current.enemies.find((enemy) => enemy.id === 4)?.position;
  await perform("/api/navigation-step");
  const movingEnemyPosition = current.enemies.find((enemy) => enemy.id === 4)?.position;
  const movingTargetAdvanced = initialEnemyPosition !== undefined && movingEnemyPosition !== undefined
    && movingEnemyPosition.some(
      (value, axis) => Math.abs(value - initialEnemyPosition[axis]!) > 0.001,
    );
  await aimAtEnemy(4);
  await firePrimary();
  const movingTargetDamaged = current.enemies.find((enemy) => enemy.id === 4)?.currentHealth === 40;
  const rejectionsBeforeCooldown = actionRejectionCount;
  await firePrimary();
  const cooldownRejected =
    actionRejectionCount === rejectionsBeforeCooldown + 1 &&
    current.enemies.find((enemy) => enemy.id === 4)?.currentHealth === 40;
  const yawBeforeRecovery = current.player.yawDegrees;
  await enqueuePlayerAction({ kind: "look", yawDelta: 0.25, pitchDelta: 0 });
  const lookRecoveredAfterRejection = current.player.yawDegrees !== yawBeforeRecovery;
  await perform("/api/navigation-phase");
  await aimAtEnemy(4);
  await firePrimary();
  await aimAtEnemy(5);
  await firePrimary();
  await enqueuePlayerAction({ kind: "look", yawDelta: 0.25, pitchDelta: 0 });
  await aimAtEnemy(5);
  await firePrimary();
  const combatHit = current.combatState === "hit";
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
  const queueRecovered = cooldownRejected && lookRecoveredAfterRejection && openGateTraversed;
  document.body.dataset.queueRecovery = queueRecovered ? "pass" : "fail";
  const cooldownRecovered =
    cooldownRejected && current.enemies.find((enemy) => enemy.id === 4)?.currentHealth === 0;
  document.body.dataset.cooldown = cooldownRecovered ? "pass" : "fail";
  await perform("/api/extraction-beacon/activate");
  await presentationFeedback.settled();
  surface.renderOnce();
  const beaconActivated =
    current.extractionBeacon?.state === "active" &&
    current.extractionBeacon.activatedBy === current.player.id &&
    eventHistory.includes("ExtractionBeaconActivated") &&
    beaconState.dataset.state === "active" &&
    beaconState.dataset.posture === "active" &&
    surface.snapshot().includes("extraction-beacon");
  document.body.dataset.beaconActivation = beaconActivated ? "pass" : "fail";
  await perform("/api/motion-phase");
  await presentationFeedback.settled();
  surface.renderOnce();
  const door = current.projection.find((node) => node.id === 3);
  const gameplayPassed =
    current.encounterState === "cleared" &&
    current.doorState === "open" &&
    door?.translation?.[1] === 4 &&
    current.enemies.every((enemy) => enemy.state === "defeated") &&
    current.motionState === "blocked" &&
    current.navigationState === "arrived" &&
    playerMoved &&
    playerBlocked &&
    playerStopped &&
    playerLooked &&
    movingTargetAdvanced &&
    movingTargetDamaged &&
    queueRecovered &&
    cooldownRecovered &&
    current.projection.find((node) => node.id === 4)?.translation?.[0] === 7.5 &&
    (current.projection.find((node) => node.id === 10)?.translation?.[0] ?? -4) > 2 &&
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
  document.body.dataset.feedbackFamilies = feedbackFamiliesPassed ? "pass" : "fail";
  document.body.dataset.feedbackEvidence = [
    feedbackLayer.dataset.animationPulses ?? "",
    feedbackLayer.dataset.particleKinds ?? "",
    feedbackLayer.dataset.billboardValues ?? "",
  ].join("|");
  const audioFeedbackPassed =
    Number(feedbackAudioStatus.dataset.attempted ?? "0") > 0 &&
    Number(feedbackAudioStatus.dataset.scheduled ?? "0") > 0;
  document.body.dataset.audioFeedback = audioFeedbackPassed ? "pass" : "fail";

  const droppedResponse = await requestState("/api/player-action", "POST", {
    kind: "move",
    forward: -1,
    right: 0,
  });
  const droppedHadTransientCue = droppedResponse.presentation.cues.some(
    (cue) => cue.kind === "movement" || cue.kind === "movementBlocked",
  );
  const refreshed = await requestState("/api/state");
  const droppedDeliverySafe =
    droppedHadTransientCue &&
    refreshed.presentation.cues.length === 0 &&
    authoritativeBrowserFingerprint(refreshed) === authoritativeBrowserFingerprint(droppedResponse);
  current = refreshed;
  const refreshFrame = projection.apply(current);
  if (refreshFrame.ops.length > 0) {
    applyRendererFrame(refreshFrame);
  }
  surface.setCameraPose(derivePlayerCameraPose(current.player));
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
  if (restartFrame.ops.length > 0) {
    applyRendererFrame(restartFrame);
  }
  surface.setCameraPose(derivePlayerCameraPose(current.player));
  renderReadout(current);
  await applyPresentationFeedback(true, restartFrame.ops.length);
  surface.renderOnce();
  const restartRebuilt =
    restartStartedWithConcreteTransients &&
    current.presentation.cues.length === 0 &&
    feedbackLayer.dataset.activeEffects === "0" &&
    feedbackAudioStatus.dataset.activeSounds === "0" &&
    document.querySelector("[data-animation-pulse]") === null &&
    feedbackLayer.dataset.lastCueCount === "0" &&
    includesEvery(feedbackLayer.dataset.animationStates, ["3:open", "4:defeated", "5:defeated"]);
  document.body.dataset.feedbackConcreteRestart = restartRebuilt ? "pass" : "fail";
  const feedbackDropPassed = droppedDeliverySafe && restartRebuilt;
  document.body.dataset.feedbackDrop = feedbackDropPassed ? "pass" : "fail";
  const passed =
    gameplayPassed &&
    voxelEditPassed &&
    rejectedUnchanged &&
    resetFeedbackRebuilt &&
    feedbackFamiliesPassed &&
    audioFeedbackPassed &&
    feedbackDropPassed;
  smokeResult.dataset.status = passed ? "pass" : "fail";
  smokeResult.textContent = passed
    ? "PASS · Rust facts reached retained WebGL and disposable feedback"
    : "FAIL · Product proof did not converge";
  document.body.dataset.smokeStatus = passed ? "pass" : "fail";
}

async function perform(path: string): Promise<void> {
  current = await requestState(path, "POST");
  if (path === "/api/reset") {
    eventHistory.length = 0;
    actionRejectionCount = 0;
    lastActionRejection = null;
  }
  eventHistory.push(...current.lastEvents);
  const frame = projection.apply(current);
  if (frame.ops.length > 0) {
    applyRendererFrame(frame);
  }
  surface.setCameraPose(derivePlayerCameraPose(current.player));
  surface.renderOnce();
  renderReadout(current);
  if (path === "/api/reset") {
    await applyPresentationFeedback(true, frame.ops.length);
  } else {
    void applyPresentationFeedback(false, frame.ops.length);
  }
  updateRendererStatus();
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
  if (frame.ops.length > 0) {
    applyRendererFrame(frame);
  }
  surface.setCameraPose(derivePlayerCameraPose(current.player));
  surface.renderOnce();
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
  combatState.textContent = lastActionRejection === null
    ? state.combatState.toUpperCase()
    : "REJECTED";
  combatState.dataset.state = lastActionRejection === null ? state.combatState : "rejected";
  combatState.title = lastActionRejection ?? "";
  playerPose.textContent = `${state.player.position.map((value) => value.toFixed(1)).join(", ")} · YAW ${state.player.yawDegrees.toFixed(0)}°`;
  weaponState.textContent = `${String(state.weapon.damage)} DMG · ${String(state.weapon.ammoRemaining)}/${String(state.weapon.ammoCapacity)} AMMO`;
  environmentState.textContent = state.generatedEnvironment === null
    ? `MATERIALIZED · ${String(state.voxelSolidCount)} VOXELS`
    : `SEED ${String(state.generatedEnvironment.seed)} · ${String(state.generatedEnvironment.meshQuads)} QUADS · ${state.generatedEnvironment.outputHash.slice(0, 8)}`;
  voxelState.textContent = `VOXEL REV ${String(state.voxelRevision)} · NAV ${state.voxelNavigationHash.slice(0, 8)} · PATH ${String(state.voxelProbePathLength)}`;
  beaconState.textContent = state.extractionBeacon?.state.toUpperCase() ?? "UNAVAILABLE";
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
}

async function applyPresentationFeedback(reset = false, renderDiffCount = 0): Promise<void> {
  doorCaption.dataset.entityId = "3";
  playerMotionState.dataset.entityId = String(current.player.id);
  const receipt = await presentationFeedback.apply(current, reset, renderDiffCount);
  feedbackLayer.dataset.lastCueCount = String(receipt.cueCount);
  feedbackLayer.dataset.failedOperations = String(receipt.failedOperations);
  feedbackLayer.dataset.scheduledSounds = String(receipt.scheduledSounds);
}

function applyRendererFrame(frame: ReturnType<RuntimeProjectionAdapter["apply"]>): void {
  const receipt = surface.applyFrame(frame);
  if (!receipt.applied) {
    throw new Error(receipt.diagnostics.map((diagnostic) => diagnostic.message).join("; "));
  }
}

function enqueuePlayerAction(action: ResolvedPlayerAction): Promise<void> {
  return actionQueue.enqueue(() => performPlayerAction(action));
}

function enqueueAttackAction(action: ResolvedAttackAction): Promise<void> {
  return actionQueue.enqueue(() => performAttackAction(action));
}

function enqueueResolvedAction(action: ResolvedInputAction): void {
  if (action.kind === "attack") {
    enqueueAttackAction(action);
  } else {
    enqueuePlayerAction(action);
  }
}

async function performPlayerAction(action: ResolvedPlayerAction): Promise<void> {
  current = await requestState("/api/player-action", "POST", action);
  lastActionRejection = null;
  eventHistory.push(...current.lastEvents);
  const frame = projection.apply(current);
  if (frame.ops.length > 0) {
    applyRendererFrame(frame);
  }
  surface.setCameraPose(derivePlayerCameraPose(current.player));
  surface.renderOnce();
  renderReadout(current);
  void applyPresentationFeedback(false, frame.ops.length);
  updateRendererStatus();
}

async function performAttackAction(action: ResolvedAttackAction): Promise<void> {
  current = await requestState("/api/attack", "POST", action);
  lastActionRejection = null;
  eventHistory.push(...current.lastEvents);
  const frame = projection.apply(current);
  if (frame.ops.length > 0) {
    applyRendererFrame(frame);
  }
  surface.setCameraPose(derivePlayerCameraPose(current.player));
  surface.renderOnce();
  renderReadout(current);
  void applyPresentationFeedback(false, frame.ops.length);
  updateRendererStatus();
}

async function aimAtEnemy(enemyId: number): Promise<void> {
  const enemy = current.enemies.find((candidate) => candidate.id === enemyId);
  if (enemy === undefined) {
    throw new Error(`enemy ${String(enemyId)} is absent`);
  }
  const offset = enemy.position.map(
    (value, axis) => value - current.player.position[axis]!,
  ) as [number, number, number];
  const horizontal = Math.hypot(offset[0], offset[2]);
  const desiredYaw = normalizeDegrees((Math.atan2(-offset[0], -offset[2]) * 180) / Math.PI);
  const desiredPitch = (Math.atan2(offset[1], horizontal) * 180) / Math.PI;
  for (let step = 0; step < 40; step += 1) {
    const yawDifference = normalizeDegrees(desiredYaw - current.player.yawDegrees);
    const pitchDifference = desiredPitch - current.player.pitchDegrees;
    if (Math.abs(yawDifference) < 0.01 && Math.abs(pitchDifference) < 0.01) {
      return;
    }
    await enqueuePlayerAction({
      kind: "look",
      yawDelta: clampUnit(yawDifference / current.player.lookDegreesPerUnit),
      pitchDelta: clampUnit(pitchDifference / current.player.lookDegreesPerUnit),
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

async function walkPlayerPath(
  waypoints: readonly (readonly [number, number])[],
): Promise<boolean> {
  for (const waypoint of waypoints) {
    if (!await walkPlayerTo(waypoint)) {
      return false;
    }
  }
  return true;
}

async function walkPlayerTo(
  target: readonly [number, number],
  maxSteps = 64,
): Promise<boolean> {
  for (let step = 0; step < maxSteps; step += 1) {
    const offsetX = target[0] - current.player.position[0];
    const offsetZ = target[1] - current.player.position[2];
    if (Math.hypot(offsetX, offsetZ) <= 0.25) {
      return true;
    }
    await turnPlayerToward(offsetX, offsetZ);
    const action = resolveKeyboardAction(
      current.player.bindings.moveForward,
      current.player.bindings,
    );
    if (action?.kind !== "move") {
      throw new Error("authored move-forward binding did not resolve to movement");
    }
    const before = current.player.position;
    await enqueuePlayerAction(action);
    if (current.player.position.every(
      (value, axis) => Math.abs(value - before[axis]!) < 0.000_001,
    )) {
      return false;
    }
  }
  return false;
}

async function turnPlayerToward(offsetX: number, offsetZ: number): Promise<void> {
  const desiredYaw = normalizeDegrees((Math.atan2(-offsetX, -offsetZ) * 180) / Math.PI);
  for (let step = 0; step < 20; step += 1) {
    const yawDifference = normalizeDegrees(desiredYaw - current.player.yawDegrees);
    if (Math.abs(yawDifference) < 0.01) {
      return;
    }
    await enqueuePlayerAction({
      kind: "look",
      yawDelta: clampUnit(yawDifference / current.player.lookDegreesPerUnit),
      pitchDelta: 0,
    });
  }
  throw new Error("could not orient player toward gate waypoint");
}

function updateRendererStatus(): void {
  rendererStatus.textContent = `${surface.kind} · ${String(projection.trackedEntityCount)} entities · ${String(projection.trackedMeshCount)} voxel meshes`;
}

export function resolveKeyboardAction(
  code: string,
  bindings: RuntimePlayerBindings,
): ResolvedInputAction | null {
  if (code === bindings.moveForward) {
    return { kind: "move", forward: 1, right: 0 };
  }
  if (code === bindings.moveBackward) {
    return { kind: "move", forward: -1, right: 0 };
  }
  if (code === bindings.moveLeft) {
    return { kind: "move", forward: 0, right: -1 };
  }
  if (code === bindings.moveRight) {
    return { kind: "move", forward: 0, right: 1 };
  }
  if (code === bindings.primaryFire) {
    return { kind: "attack" };
  }
  return null;
}

export function resolvePointerButtonAction(
  button: number,
  bindings: RuntimePlayerBindings,
): ResolvedAttackAction | null {
  return bindings.primaryFire === `Mouse${String(button)}` ? { kind: "attack" } : null;
}

export function resolvePointerAction(
  movementX: number,
  movementY: number,
  bindings: RuntimePlayerBindings,
): ResolvedPlayerAction | null {
  if (bindings.mouseLook !== "pointer" || (movementX === 0 && movementY === 0)) {
    return null;
  }
  return {
    kind: "look",
    yawDelta: clampUnit(movementX / 20),
    pitchDelta: clampUnit(movementY / 20),
  };
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
      : { body: JSON.stringify(body), headers: { "Content-Type": "application/json" } }),
  });
  if (!response.ok) {
    const detail = await response.json().catch(() => null) as { readonly error?: unknown } | null;
    const reason = typeof detail?.error === "string" ? `: ${detail.error}` : "";
    throw new Error(`${method} ${path} failed with ${String(response.status)}${reason}`);
  }
  return (await response.json()) as RuntimeBrowserState;
}

function recordActionRejection(error: unknown): void {
  actionRejectionCount += 1;
  lastActionRejection = error instanceof Error ? error.message : String(error);
  eventHistory.push(
    lastActionRejection.includes("CombatRejected") ? "CombatRejected" : "ActionRejected",
  );
  renderReadout(current);
}

function clampUnit(value: number): number {
  return Math.max(-1, Math.min(1, value));
}

function normalizeDegrees(value: number): number {
  return ((value + 180) % 360 + 360) % 360 - 180;
}

function includesEvery(value: string | undefined, expected: readonly string[]): boolean {
  const values = new Set((value ?? "").split(",").filter(Boolean));
  return expected.every((candidate) => values.has(candidate));
}

function authoritativeBrowserFingerprint(state: RuntimeBrowserState): string {
  return JSON.stringify({
    tick: state.tick,
    entityRevision: state.entityRevision,
    projection: state.projection,
    doorState: state.doorState,
    encounterState: state.encounterState,
    player: state.player,
    weapon: state.weapon,
    extractionBeacon: state.extractionBeacon,
    enemies: state.enemies,
  });
}

function meshFingerprint(state: RuntimeBrowserState): string {
  return state.voxelMeshes.map((mesh) => `${mesh.chunk.join(",")}:${mesh.contentHash}`).join("|");
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
  return new Promise((resolve) => globalThis.setTimeout(resolve, milliseconds));
}

function requiredElement<T extends Element>(id: string, constructor: { new (): T }): T {
  const element = document.getElementById(id);
  if (!(element instanceof constructor)) {
    throw new Error(`missing required element #${id}`);
  }
  return element;
}
