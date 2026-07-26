import assert from "node:assert/strict";
import test from "node:test";

import { decodePresentationFrameDiff } from "@rusty-engine/render-contracts";

import {
  PresentationFeedbackAdapter,
  captureRendererTelemetry,
} from "./presentation-feedback.ts";
import type { RuntimeBrowserState } from "./projection.ts";

test("typed gameplay cues map to shared audio billboard particle and telemetry descriptors", () => {
  const adapter = new PresentationFeedbackAdapter();
  const projected = adapter.project(feedbackState());

  assert.equal(decodePresentationFrameDiff(projected.frame).ops.length, 26);
  assert.deepEqual(
    projected.animationStates.map(
      (state) => `${String(state.entity)}:${state.posture}`,
    ),
    ["1:idle", "4:defeated", "3:open"],
  );
  assert.deepEqual(projected.animationPulses, [
    "movement",
    "blocked",
    "arc-pistol-attack",
    "arc-pistol-dry",
    "damage",
    "defeat",
    "open",
    "active",
    "pickup",
  ]);
  assert.deepEqual(projected.particleKinds, [
    "movement",
    "blocked",
    "muzzle",
    "dry",
    "impact",
    "defeat",
    "door",
    "beacon",
    "pickup",
  ]);
  assert.deepEqual(projected.billboardValues, [
    "BLOCKED",
    "EMPTY",
    "-60",
    "DEFEATED",
    "EXIT OPEN",
    "EXTRACTION ONLINE",
    "+12 ammo/scatter-shell",
  ]);
  assert.deepEqual(projected.soundKinds, [
    "step",
    "blocked",
    "sidearmShot",
    "dryFire",
    "hit",
    "defeat",
    "doorOpen",
    "beacon",
    "pickup",
  ]);

  const domains = projected.frame.ops.map((operation) => operation.domain);
  assert.equal(
    domains.filter((domain) => domain === "telemetryOverlay").length,
    1,
  );
  assert.equal(domains.filter((domain) => domain === "particle").length, 9);
  assert.equal(domains.filter((domain) => domain === "audio").length, 9);
  assert.equal(domains.filter((domain) => domain === "billboard").length, 7);
  assert.deepEqual(
    projected.frame.ops.map((operation) => operation.meta.sequence),
    projected.frame.ops.map((_, index) => index),
  );

  const damageBillboard = projected.frame.ops.find(
    (operation) =>
      operation.domain === "billboard" &&
      operation.op.op === "create" &&
      operation.op.descriptor.content.kind === "text" &&
      operation.op.descriptor.content.fallbackText === "-60",
  );
  assert.deepEqual(
    damageBillboard?.domain === "billboard" &&
      damageBillboard.op.op === "create"
      ? damageBillboard.op.descriptor.anchor
      : null,
    { kind: "world", position: [7.5, 0, 5.5] },
  );
  const doorBillboard = projected.frame.ops.find(
    (operation) =>
      operation.domain === "billboard" &&
      operation.op.op === "create" &&
      operation.op.descriptor.content.kind === "text" &&
      operation.op.descriptor.content.fallbackText === "EXIT OPEN",
  );
  assert.deepEqual(
    doorBillboard?.domain === "billboard" && doorBillboard.op.op === "create"
      ? doorBillboard.op.descriptor.anchor
      : null,
    { kind: "world", position: [4.5, 4, 10.5] },
  );
});

test("shared signal ids are delivery-local and a reset reopens retained host identities", () => {
  const adapter = new PresentationFeedbackAdapter();
  const first = adapter.project(feedbackState());
  const second = adapter.project(feedbackState());
  const firstSignals = signalIds(first.frame);
  const secondSignals = signalIds(second.frame);

  assert.equal(first.frame.ops[0]?.domain, "telemetryOverlay");
  assert.equal(
    second.frame.ops.some(
      (operation) => operation.domain === "telemetryOverlay",
    ),
    false,
  );
  assert.equal(
    firstSignals.some((signal) => secondSignals.includes(signal)),
    false,
  );
  assert.notDeepEqual(first.billboardHandles, second.billboardHandles);

  adapter.reset();
  const reopened = adapter.project(feedbackState());
  assert.equal(reopened.frame.ops[0]?.domain, "telemetryOverlay");
  assert.deepEqual(reopened.billboardHandles, first.billboardHandles);
  assert.deepEqual(signalIds(reopened.frame), firstSignals);
});

test("progression cues remain typed disposable presentation", () => {
  const state = feedbackState();
  const projected = new PresentationFeedbackAdapter().project({
    ...state,
    projection: [
      ...state.projection,
      {
        id: 30,
        name: "maintenance-bulkhead",
        asset: "mesh/security-door",
        translation: [4.5, 1.5, 5.5],
        visible: true,
      },
      {
        id: 31,
        name: "overlook-secret",
        asset: "",
        translation: [6.5, 1.5, 8.5],
        visible: false,
      },
      {
        id: 32,
        name: "loading-bay-level-exit",
        asset: "mesh/level-exit",
        translation: [4.5, 1.5, 12.5],
        visible: true,
      },
    ],
    presentation: {
      animationStates: state.presentation.animationStates,
      cues: [
        {
          kind: "doorAccessDenied",
          entity: 30,
          requiredKey: "key/maintenance-pass",
          presentation: "Maintenance pass required",
        },
        {
          kind: "doorAccessGranted",
          entity: 30,
          actor: 1,
          requiredKey: "key/maintenance-pass",
          keyConsumed: false,
        },
        {
          kind: "secretDiscovered",
          entity: 31,
          actor: 1,
          presentation: "Secret overlook discovered",
        },
        {
          kind: "levelCompleted",
          entity: 32,
          actor: 1,
          presentation: "Loading Bay complete",
        },
      ],
    },
  });

  assert.deepEqual(projected.animationPulses, [
    "access-denied",
    "access-granted",
    "secret-discovered",
    "level-completed",
  ]);
  assert.deepEqual(projected.billboardValues, [
    "Maintenance pass required",
    "ACCESS GRANTED",
    "Secret overlook discovered",
    "Loading Bay complete",
  ]);
  assert.deepEqual(projected.soundKinds, [
    "blocked",
    "doorOpen",
    "pickup",
    "beacon",
  ]);
});

test("host-user effects volume scales disposable audio descriptors only", () => {
  const full = new PresentationFeedbackAdapter().project(feedbackState(), 1);
  const quiet = new PresentationFeedbackAdapter().project(
    feedbackState(),
    0.25,
  );
  const fullVolumes = audioVolumes(full.frame);
  const quietVolumes = audioVolumes(quiet.frame);

  assert.equal(fullVolumes.length, 9);
  assert.deepEqual(
    quietVolumes,
    fullVolumes.map((volume) => volume * 0.25),
  );
  assert.deepEqual(quiet.particleKinds, full.particleKinds);
  assert.deepEqual(quiet.billboardValues, full.billboardValues);
});

test("renderer telemetry uses the shared surface timing without a downstream clock", () => {
  const state = feedbackState();
  const timing = {
    schemaVersion: 1 as const,
    renderSequence: 42,
    source: "animationFrame" as const,
    sourceTimeMs: 900,
    frameIntervalMs: 16.75,
    frameIntervalStatus: "available" as const,
    backendSubmissionDurationMs: 0.85,
    backendSubmissionDurationStatus: "available" as const,
  };

  const sample = captureRendererTelemetry({ timing: () => timing }, state, 3);

  assert.equal(sample.timing, timing);
  assert.equal(sample.sourceTick, state.tick);
  assert.deepEqual(sample.counters, {
    entityCount: state.projection.length,
    residentChunkCount: state.voxelMeshes.length,
    renderDiffCount: 3,
  });
  assert.equal("frameTimeMs" in sample, false);
});

function signalIds(
  frame: ReturnType<PresentationFeedbackAdapter["project"]>["frame"],
): string[] {
  return frame.ops.flatMap((operation) => {
    if (
      (operation.domain === "audio" || operation.domain === "particle") &&
      operation.op.op === "emit"
    ) {
      return [operation.op.signalId];
    }
    return [];
  });
}

function audioVolumes(
  frame: ReturnType<PresentationFeedbackAdapter["project"]>["frame"],
): number[] {
  return frame.ops.flatMap((operation) =>
    operation.domain === "audio" && operation.op.op === "emit"
      ? [operation.op.descriptor.volume]
      : [],
  );
}

function feedbackState(): RuntimeBrowserState {
  return {
    hostSessionId: "host-a",
    tick: 5,
    entityRevision: 8,
    voxelRevision: 0,
    voxelAuthorityHash: "0000000000000000",
    voxelSolidCount: 0,
    voxelNavigationHash: "0000000000000000",
    voxelProbePathLength: 0,
    projection: [
      {
        id: 3,
        name: "exit",
        asset: "mesh/security-door",
        translation: [4.5, 4, 10.5],
        visible: true,
      },
    ],
    doorState: "open",
    encounterState: "cleared",
    motionState: "blocked",
    navigationState: "arrived",
    playerMotionState: "moved",
    combatState: "hit",
    input: {
      connectionGeneration: 1,
      connected: true,
      paused: false,
      acknowledgedSequence: 4,
      consumedSequence: 4,
      queuedEdgeCommands: 0,
    },
    player: {
      id: 1,
      position: [2, 0, 3],
      yawDegrees: 0,
      pitchDegrees: 0,
      moveStepSeconds: 0.1,
      lookDegreesPerUnit: 12,
      bindings: {
        moveForward: "KeyW",
        moveBackward: "KeyS",
        moveLeft: "KeyA",
        moveRight: "KeyD",
        mouseLook: "pointer",
        primaryFire: "Mouse0",
        selectWeapon: ["Digit1", "Digit2", "Digit3"],
      },
      currentHealth: 100,
      maxHealth: 100,
      armor: 0,
      maxArmor: 100,
      vitalityState: "alive",
    },
    weapon: {
      item: "weapon/arc-pistol",
      presentation: "arc-pistol",
      damage: 60,
      ammunition: "ammo/energy-cell",
      ammunitionCost: 1,
      ammoRemaining: 6,
      ammoCapacity: 8,
      readyAtTick: 6,
    },
    inventory: {
      owner: 1,
      capacitySlots: 8,
      stacks: [{ item: "weapon/arc-pistol", quantity: 1 }],
      equippedWeapon: "weapon/arc-pistol",
      weapons: [
        {
          slot: 0,
          item: "weapon/arc-pistol",
          owned: true,
          selected: true,
          ammunition: "ammo/energy-cell",
          ammunitionQuantity: 6,
        },
      ],
    },
    pickups: [],
    hazards: [],
    restart: {
      authoredBaselineAvailable: true,
      checkpointAvailable: false,
    },
    extractionBeacon: {
      id: 7,
      state: "active",
      activationRadius: 2.5,
      activatedBy: 1,
      activatedAtTick: 5,
    },
    doorAccess: [],
    secretRegions: [],
    levelExits: [],
    levelComplete: false,
    interaction: null,
    voxelMeshes: [],
    generatedEnvironment: null,
    enemies: [
      {
        id: 4,
        name: "sentry",
        state: "defeated",
        position: [7.5, 0, 5.5],
        currentHealth: 0,
        maxHealth: 100,
      },
    ],
    presentation: {
      animationStates: [
        { entity: 1, posture: "idle" },
        { entity: 4, posture: "defeated" },
        { entity: 3, posture: "open" },
      ],
      cues: [
        { kind: "movement", entity: 1, from: [1, 0, 3], to: [2, 0, 3] },
        { kind: "movementBlocked", entity: 1 },
        {
          kind: "attack",
          attacker: 1,
          weapon: "weapon/arc-pistol",
          presentation: "arc-pistol",
          attackMode: "hitscan",
          rayCount: 1,
          origin: [2, 1, 3],
          direction: [0, 0, -1],
        },
        {
          kind: "dryFire",
          attacker: 1,
          weapon: "weapon/arc-pistol",
          presentation: "arc-pistol",
        },
        { kind: "damage", attacker: 1, target: 4, amount: 60, remaining: 40 },
        { kind: "defeat", attacker: 1, entity: 4 },
        { kind: "doorChanged", entity: 3, state: "open" },
        { kind: "extractionBeaconActivated", entity: 7, actor: 1 },
        {
          kind: "pickupCollected",
          entity: 22,
          actor: 1,
          item: "ammo/scatter-shell",
          quantity: 12,
        },
      ],
    },
    lastEvents: [],
  };
}
