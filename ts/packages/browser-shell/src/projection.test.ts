import assert from "node:assert/strict";
import test from "node:test";

import {
  RuntimeProjectionAdapter,
  derivePlayerCameraPose,
  entityHandle,
  type RuntimeBrowserState,
} from "./projection.ts";

function state(
  projection: RuntimeBrowserState["projection"],
): RuntimeBrowserState {
  return {
    tick: 0,
    entityRevision: 0,
    voxelRevision: 0,
    voxelAuthorityHash: "0000000000000000",
    voxelSolidCount: 0,
    voxelNavigationHash: "0000000000000000",
    voxelProbePathLength: 0,
    projection,
    doorState: "closed",
    encounterState: "active",
    motionState: "moving",
    navigationState: "following",
    playerMotionState: "idle",
    combatState: "ready",
    input: {
      connectionGeneration: 1,
      connected: true,
      paused: false,
      acknowledgedSequence: 0,
      consumedSequence: 0,
      queuedEdgeCommands: 0,
    },
    player: {
      id: 1,
      position: [0.5, 0.5, 0.5],
      yawDegrees: 180,
      pitchDegrees: -10,
      moveStepSeconds: 0.1,
      lookDegreesPerUnit: 12,
      bindings: {
        moveForward: "KeyW",
        moveBackward: "KeyS",
        moveLeft: "KeyA",
        moveRight: "KeyD",
        mouseLook: "pointer",
        primaryFire: "Mouse0",
      },
    },
    weapon: {
      damage: 100,
      ammoRemaining: 8,
      ammoCapacity: 8,
      readyAtTick: 0,
    },
    extractionBeacon: null,
    voxelMeshes: [],
    generatedEnvironment: null,
    enemies: [],
    presentation: { animationStates: [], cues: [] },
    lastEvents: [],
  };
}

test("whole Rust readouts become create update and destroy diffs", () => {
  const adapter = new RuntimeProjectionAdapter();
  const original = {
    id: 3,
    name: "exit",
    asset: "mesh/security-door",
    translation: [0, 0, 8] as const,
    visible: true,
  };

  const created = adapter.apply(state([original]));
  assert.deepEqual(
    created.ops.map((op) => op.op),
    ["create"],
  );
  assert.equal(
    created.ops[0]?.op === "create" ? created.ops[0].handle : null,
    entityHandle(3),
  );
  created.commit();

  const updated = adapter.apply(
    state([{ ...original, translation: [0, 3, 8] as const }]),
  );
  assert.deepEqual(
    updated.ops.map((op) => op.op),
    ["update"],
  );
  updated.commit();

  const destroyed = adapter.apply(state([]));
  assert.deepEqual(
    destroyed.ops.map((op) => op.op),
    ["destroy"],
  );
  destroyed.commit();
  assert.equal(adapter.trackedEntityCount, 0);
});

test("generated chunk mesh is retained by content hash and uses the typed mesh payload path", () => {
  const adapter = new RuntimeProjectionAdapter();
  const mesh = {
    chunk: [0, 0, 0] as const,
    contentHash: "abc",
    translation: [0, 0, 0] as const,
    positions: [0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0],
    normals: [0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1],
    indices: [0, 1, 2, 0, 2, 3],
    groups: [{ materialSlot: 3, start: 0, count: 6 }],
    boundsMin: [0, 0, 0] as const,
    boundsMax: [1, 1, 0] as const,
  };
  const initial = { ...state([]), voxelMeshes: [mesh] };

  const created = adapter.apply(initial);
  assert.deepEqual(
    created.ops.map((op) => op.op),
    ["create", "replaceMeshPayload"],
  );
  created.commit();
  const unchanged = adapter.apply(initial);
  assert.deepEqual(unchanged.ops, []);
  unchanged.commit();
  const updated = adapter.apply({
    ...initial,
    voxelMeshes: [{ ...mesh, contentHash: "def" }],
  });
  assert.deepEqual(
    updated.ops.map((op) => op.op),
    ["replaceMeshPayload"],
  );
  updated.commit();
  assert.equal(adapter.trackedMeshCount, 1);
});

test("camera pose is rebuilt as a presentation offset from accepted player state", () => {
  const player = state([]).player;
  const camera = derivePlayerCameraPose(player);

  assert.ok(Math.abs(camera.position[0] - 0.5) < 0.000_001);
  assert.equal(camera.position[1], 1.7);
  assert.equal(camera.position[2], -0.5);
  assert.equal(camera.yawDegrees, 180);
  assert.equal(camera.pitchDegrees, -10);
  assert.equal("camera" in player, false);

  const localPlayer = {
    id: 1,
    name: "player",
    asset: "primitive/player-marker",
    translation: [0.5, 0.5, 0.5] as const,
    visible: true,
  };
  const localPlan = new RuntimeProjectionAdapter().apply(state([localPlayer]));
  const created = localPlan.ops[0];
  assert.equal(created?.op === "create" ? created.node.visible : true, false);
  localPlan.commit();
});

test("demo-owned beacon state changes retained Three material without a generic bridge", () => {
  const adapter = new RuntimeProjectionAdapter();
  const beacon = {
    id: 7,
    name: "extraction-beacon",
    asset: "mesh/extraction-beacon",
    translation: [4.5, 1.5, 12.5] as const,
    visible: true,
  };
  const standby = {
    ...state([beacon]),
    extractionBeacon: {
      id: 7,
      state: "standby" as const,
      activationRadius: 2.5,
      activatedBy: null,
      activatedAtTick: null,
    },
  };

  const standbyPlan = adapter.apply(standby);
  const standbyCreated = standbyPlan.ops[0];
  standbyPlan.commit();
  const activePlan = adapter.apply({
    ...standby,
    extractionBeacon: {
      ...standby.extractionBeacon,
      state: "active",
      activatedBy: 1,
      activatedAtTick: 9,
    },
  });
  const active = activePlan.ops[0];

  assert.deepEqual(
    standbyCreated?.op === "create" ? standbyCreated.node.material.color : null,
    [0.85, 0.54, 0.18, 1],
  );
  assert.deepEqual(
    active?.op === "update" ? active.material?.color : null,
    [0.22, 0.95, 0.72, 1],
  );
  activePlan.commit();
});

test("rejected create update destroy and mesh plans remain retryable until commit", () => {
  const adapter = new RuntimeProjectionAdapter();
  const original = {
    id: 3,
    name: "exit",
    asset: "mesh/security-door",
    translation: [0, 0, 8] as const,
    visible: true,
  };

  const rejectedCreate = adapter.apply(state([original]));
  assert.deepEqual(
    rejectedCreate.ops.map((operation) => operation.op),
    ["create"],
  );
  assert.equal(adapter.trackedEntityCount, 0);
  const retriedCreate = adapter.apply(state([original]));
  assert.deepEqual(
    retriedCreate.ops.map((operation) => operation.op),
    ["create"],
  );
  retriedCreate.commit();

  const moved = state([{ ...original, translation: [0, 2, 8] as const }]);
  const rejectedUpdate = adapter.apply(moved);
  assert.deepEqual(
    rejectedUpdate.ops.map((operation) => operation.op),
    ["update"],
  );
  const retriedUpdate = adapter.apply(moved);
  assert.deepEqual(
    retriedUpdate.ops.map((operation) => operation.op),
    ["update"],
  );
  retriedUpdate.commit();

  const rejectedDestroy = adapter.apply(state([]));
  assert.deepEqual(
    rejectedDestroy.ops.map((operation) => operation.op),
    ["destroy"],
  );
  assert.equal(adapter.trackedEntityCount, 1);
  const retriedDestroy = adapter.apply(state([]));
  assert.deepEqual(
    retriedDestroy.ops.map((operation) => operation.op),
    ["destroy"],
  );
  retriedDestroy.commit();
  assert.equal(adapter.trackedEntityCount, 0);

  const meshAdapter = new RuntimeProjectionAdapter();
  const mesh = {
    chunk: [0, 0, 0] as const,
    contentHash: "accepted",
    translation: [0, 0, 0] as const,
    positions: [0, 0, 0, 1, 0, 0, 0, 1, 0],
    normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
    indices: [0, 1, 2],
    groups: [{ materialSlot: 1, start: 0, count: 3 }],
    boundsMin: [0, 0, 0] as const,
    boundsMax: [1, 1, 0] as const,
  };
  const acceptedMesh = meshAdapter.apply({ ...state([]), voxelMeshes: [mesh] });
  acceptedMesh.commit();
  const changedMeshState = {
    ...state([]),
    voxelMeshes: [{ ...mesh, contentHash: "candidate" }],
  };
  const rejectedMesh = meshAdapter.apply(changedMeshState);
  assert.deepEqual(
    rejectedMesh.ops.map((operation) => operation.op),
    ["replaceMeshPayload"],
  );
  const retriedMesh = meshAdapter.apply(changedMeshState);
  assert.deepEqual(
    retriedMesh.ops.map((operation) => operation.op),
    ["replaceMeshPayload"],
  );
  retriedMesh.commit();
});
