import assert from "node:assert/strict";
import test from "node:test";

import {
  RuntimeProjectionAdapter,
  derivePlayerCameraPose,
  entityHandle,
  type RuntimeBrowserState,
} from "./projection.ts";

function state(projection: RuntimeBrowserState["projection"]): RuntimeBrowserState {
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
  assert.deepEqual(created.ops.map((op) => op.op), ["create"]);
  assert.equal(created.ops[0]?.op === "create" ? created.ops[0].handle : null, entityHandle(3));

  const updated = adapter.apply(
    state([{ ...original, translation: [0, 3, 8] as const }]),
  );
  assert.deepEqual(updated.ops.map((op) => op.op), ["update"]);

  const destroyed = adapter.apply(state([]));
  assert.deepEqual(destroyed.ops.map((op) => op.op), ["destroy"]);
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

  assert.deepEqual(adapter.apply(initial).ops.map((op) => op.op), [
    "create",
    "replaceMeshPayload",
  ]);
  assert.deepEqual(adapter.apply(initial).ops, []);
  assert.deepEqual(
    adapter.apply({ ...initial, voxelMeshes: [{ ...mesh, contentHash: "def" }] }).ops.map((op) => op.op),
    ["replaceMeshPayload"],
  );
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
  const created = new RuntimeProjectionAdapter().apply(state([localPlayer])).ops[0];
  assert.equal(created?.op === "create" ? created.node.visible : true, false);
});
