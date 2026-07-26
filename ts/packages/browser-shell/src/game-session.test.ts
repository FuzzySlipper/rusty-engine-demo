import assert from "node:assert/strict";
import test from "node:test";

import {
  GameSessionError,
  applyServerUpdate,
  coalesceSessionLook,
  type ServerUpdateEnvelope,
} from "./game-session.ts";

const dynamic = {
  tick: 1,
  entityRevision: 1,
  projection: [],
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
    position: [1.5, 1.5, 2.5],
    yawDegrees: 0,
    pitchDegrees: -10,
    moveStepSeconds: 0.1,
    lookDegreesPerUnit: 12,
    bindings: {
      moveForward: "KeyW",
      moveBackward: "KeyS",
      moveLeft: "KeyA",
      moveRight: "KeyD",
      mouseLook: "MouseMove",
      primaryFire: "Mouse0",
    },
  },
  weapon: {
    damage: 20,
    ammoRemaining: 8,
    ammoCapacity: 8,
    readyAtTick: 0,
  },
  extractionBeacon: null,
  enemies: [],
  presentation: { animationStates: [], cues: [] },
  lastEvents: [],
} as const;

const resources = {
  staticRevision: "1:abc",
  voxelRevision: 1,
  voxelAuthorityHash: "abc",
  voxelSolidCount: 1,
  voxelNavigationHash: "def",
  voxelProbePathLength: 2,
  voxelMeshes: [],
  generatedEnvironment: null,
} as const;

const metrics = {
  inboundCommandCount: 0,
  outboundUpdateCount: 1,
  rejectedCommandCount: 0,
  lastInboundBytes: 0,
  lastOutboundBytes: 0,
  legacyWholeStateBytes: 98_278,
  bootstrapOutboundBytes: 98_278,
  staticResourceUpdateCount: 0,
  staticResourceLastBytes: 0,
  staticResourceMaxBytes: 0,
  steadyStateLastBytes: 1_024,
  steadyStateMaxBytes: 2_048,
  steadyStateUpdateCount: 3,
  maximumPendingOutboundUpdates: 1,
  droppedFactCount: 0,
  lastUpdateBuildMicroseconds: 100,
  maximumUpdateBuildMicroseconds: 200,
} as const;

test("full session bootstrap composes dynamic state with immutable resources", () => {
  const envelope: ServerUpdateEnvelope = {
    protocolVersion: 1,
    sessionId: "loading-bay-1",
    connectionGeneration: 1,
    serverTick: 1,
    snapshotSequence: 1,
    acknowledgedCommandSequence: 0,
    staticRevision: resources.staticRevision,
    update: { kind: "full", state: dynamic },
    resources,
    facts: [],
    metrics,
  };

  const applied = applyServerUpdate(null, envelope);
  assert.equal(applied.state.tick, 1);
  assert.equal(applied.state.voxelAuthorityHash, "abc");
  assert.equal(applied.state.voxelMeshes, resources.voxelMeshes);
});

test("dynamic deltas retain cold resources and reject a sequence gap", () => {
  const full: ServerUpdateEnvelope = {
    protocolVersion: 1,
    sessionId: "loading-bay-1",
    connectionGeneration: 1,
    serverTick: 1,
    snapshotSequence: 1,
    acknowledgedCommandSequence: 0,
    staticRevision: resources.staticRevision,
    update: { kind: "full", state: dynamic },
    resources,
    facts: [],
    metrics,
  };
  const initial = applyServerUpdate(null, full);
  const delta: ServerUpdateEnvelope = {
    protocolVersion: 1,
    sessionId: "loading-bay-1",
    connectionGeneration: 1,
    serverTick: 2,
    snapshotSequence: 2,
    acknowledgedCommandSequence: 0,
    staticRevision: resources.staticRevision,
    update: {
      kind: "delta",
      baseSnapshotSequence: 1,
      changes: {
        tick: 2,
        player: {
          ...dynamic.player,
          position: [1.5, 1.5, 2.6],
        },
      },
    },
    facts: [],
    metrics,
  };

  const applied = applyServerUpdate(initial.baseline, delta);
  assert.deepEqual(applied.state.player.position, [1.5, 1.5, 2.6]);
  assert.equal(applied.state.voxelMeshes, resources.voxelMeshes);

  assert.throws(
    () =>
      applyServerUpdate(initial.baseline, {
        ...delta,
        snapshotSequence: 4,
        update: {
          kind: "delta",
          baseSnapshotSequence: 3,
          changes: { tick: 4 },
        },
      }),
    (error) =>
      error instanceof GameSessionError &&
      error.code === "deltaBaseUnavailable",
  );

  assert.throws(
    () =>
      applyServerUpdate(initial.baseline, {
        ...delta,
        snapshotSequence: 3,
      }),
    (error) =>
      error instanceof GameSessionError &&
      error.code === "deltaBaseUnavailable",
  );
});

test("resource revision changes require a matching full resource payload", () => {
  const full: ServerUpdateEnvelope = {
    protocolVersion: 1,
    sessionId: "loading-bay-1",
    connectionGeneration: 1,
    serverTick: 1,
    snapshotSequence: 1,
    acknowledgedCommandSequence: 0,
    staticRevision: resources.staticRevision,
    update: { kind: "full", state: dynamic },
    resources,
    facts: [],
    metrics,
  };
  const initial = applyServerUpdate(null, full);

  assert.throws(
    () =>
      applyServerUpdate(initial.baseline, {
        protocolVersion: 1,
        sessionId: "loading-bay-1",
        connectionGeneration: 1,
        serverTick: 2,
        snapshotSequence: 2,
        acknowledgedCommandSequence: 0,
        staticRevision: "2:changed",
        update: {
          kind: "delta",
          baseSnapshotSequence: 1,
          changes: { tick: 2 },
        },
        facts: [],
        metrics,
      }),
    (error) =>
      error instanceof GameSessionError &&
      error.code === "contentRevisionMismatch" &&
      error.retry === "resync",
  );
});

test("a replacement session reuses resources with the same static revision", () => {
  const initial = applyServerUpdate(null, {
    protocolVersion: 1,
    sessionId: "loading-bay-1",
    connectionGeneration: 1,
    serverTick: 1,
    snapshotSequence: 1,
    acknowledgedCommandSequence: 0,
    staticRevision: resources.staticRevision,
    update: { kind: "full", state: dynamic },
    resources,
    facts: [],
    metrics,
  });

  const replacement = applyServerUpdate(initial.baseline, {
    protocolVersion: 1,
    sessionId: "loading-bay-2",
    connectionGeneration: 2,
    serverTick: 0,
    snapshotSequence: 1,
    acknowledgedCommandSequence: 0,
    staticRevision: resources.staticRevision,
    update: {
      kind: "full",
      state: {
        ...dynamic,
        tick: 0,
      },
    },
    facts: [],
    metrics,
  });

  assert.equal(replacement.baseline.sessionId, "loading-bay-2");
  assert.equal(replacement.state.voxelMeshes, resources.voxelMeshes);
});

test("coalesced look remains within the authoritative input envelope", () => {
  assert.deepEqual(coalesceSessionLook([0.75, 0.75], [0.75, 0.75]), [1, 1]);
  assert.deepEqual(
    coalesceSessionLook([-0.75, -0.75], [-0.75, -0.75]),
    [-1, -1],
  );
  assert.deepEqual(
    coalesceSessionLook([0.25, -0.25], [0.125, -0.125]),
    [0.375, -0.375],
  );
});
