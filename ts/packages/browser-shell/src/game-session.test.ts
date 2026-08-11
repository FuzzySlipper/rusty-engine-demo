import assert from "node:assert/strict";
import test from "node:test";

import {
  GameSessionError,
  LoadingBayGameSession,
  applyServerUpdate,
  coalesceSessionLook,
  type ServerUpdateEnvelope,
} from "./game-session.ts";

const dynamic = {
  tick: 1,
  entityRevision: 1,
  gameplayFrame: { schemaVersion: 1, ops: [] },
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
    damage: 20,
    ammunition: "ammo/energy-cell",
    ammunitionCost: 1,
    ammoRemaining: 8,
    ammoCapacity: 8,
    readyAtTick: 0,
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
        ammunitionQuantity: 8,
      },
    ],
  },
  pickups: [],
  hazards: [],
  restart: {
    authoredBaselineAvailable: true,
    checkpointAvailable: false,
  },
  saveSlots: [
    {
      slot: "checkpoint",
      compatibility: "empty",
      storageRevision: null,
      metadata: null,
      project: null,
      diagnostic: null,
    },
    {
      slot: "slot1",
      compatibility: "empty",
      storageRevision: null,
      metadata: null,
      project: null,
      diagnostic: null,
    },
    {
      slot: "slot2",
      compatibility: "empty",
      storageRevision: null,
      metadata: null,
      project: null,
      diagnostic: null,
    },
    {
      slot: "slot3",
      compatibility: "empty",
      storageRevision: null,
      metadata: null,
      project: null,
      diagnostic: null,
    },
  ],
  extractionBeacon: null,
  doorAccess: [],
  secretRegions: [],
  floorActions: [],
  lifts: [],
  levelExits: [],
  levelComplete: false,
  interaction: null,
  enemies: [],
  presentation: { animationStates: [], cues: [] },
  lastEvents: [],
} as const;

const resources = {
  hostSessionId: "host-a",
  projectId: "loading-bay",
  staticRevision: "1:abc",
  voxelRevision: 1,
  voxelAuthorityHash: "abc",
  voxelSolidCount: 1,
  voxelNavigationHash: "def",
  voxelProbePathLength: 2,
  voxelEnvironmentRole: "visible",
  voxelMeshes: [],
  voxelObjectFrame: { schemaVersion: 1, ops: [] },
  lights: [],
  renderMaterials: [],
  staticMeshes: [],
  animatedMeshes: [],
  visualBindings: [],
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
    protocolVersion: 2,
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

test("legacy projects preserve an absent Rust inventory through browser composition", () => {
  const envelope: ServerUpdateEnvelope = {
    protocolVersion: 2,
    sessionId: "legacy-project-1",
    connectionGeneration: 1,
    serverTick: 1,
    snapshotSequence: 1,
    acknowledgedCommandSequence: 0,
    staticRevision: resources.staticRevision,
    update: { kind: "full", state: { ...dynamic, inventory: null } },
    resources,
    facts: [],
    metrics,
  };

  const applied = applyServerUpdate(null, envelope);
  assert.equal(applied.state.inventory, null);
});

test("dynamic projection rejects malformed hazard authority at the browser boundary", () => {
  const malformed = {
    ...dynamic,
    hazards: [
      {
        id: 27,
        damage: "20",
        cooldownTicks: 60,
        readyAtTick: 1,
      },
    ],
  };
  assert.throws(
    () =>
      applyServerUpdate(null, {
        protocolVersion: 2,
        sessionId: "loading-bay-1",
        connectionGeneration: 1,
        serverTick: 1,
        snapshotSequence: 1,
        acknowledgedCommandSequence: 0,
        staticRevision: resources.staticRevision,
        update: { kind: "full", state: malformed as never },
        resources,
        facts: [],
        metrics,
      }),
    (error) =>
      error instanceof GameSessionError && error.code === "protocolMismatch",
  );
});

test("dynamic deltas retain cold resources and reject a sequence gap", () => {
  const full: ServerUpdateEnvelope = {
    protocolVersion: 2,
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
    protocolVersion: 2,
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

test("dynamic deltas patch only changed keyed collection members", () => {
  const projection = [
    {
      id: 3,
      name: "bulkhead",
      asset: "mesh/security-door",
      translation: [3, 4, 5] as const,
      rotation: [0, 0, 0, 1] as const,
      scale: [1, 1, 1] as const,
      visible: true,
      visualState: "closed" as const,
    },
    {
      id: 4,
      name: "cargo-loader",
      asset: "enemy/cargo-loader",
      translation: [6, 7, 8] as const,
      rotation: [0, 0, 0, 1] as const,
      scale: [1, 1, 1] as const,
      visible: true,
      visualState: "default" as const,
    },
  ];
  const initial = applyServerUpdate(null, {
    protocolVersion: 2,
    sessionId: "loading-bay-1",
    connectionGeneration: 1,
    serverTick: 1,
    snapshotSequence: 1,
    acknowledgedCommandSequence: 0,
    staticRevision: resources.staticRevision,
    update: {
      kind: "full",
      state: { ...dynamic, projection },
    },
    resources,
    facts: [],
    metrics,
  });

  const applied = applyServerUpdate(initial.baseline, {
    protocolVersion: 2,
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
        projection: {
          $collectionPatch: 1,
          key: "id",
          upserts: [
            {
              ...projection[1],
              translation: [6, 7, 9],
            },
          ],
          removed: [],
        },
      },
    },
    facts: [],
    metrics,
  });

  assert.equal(applied.state.projection[0], projection[0]);
  assert.deepEqual(applied.state.projection[1]?.translation, [6, 7, 9]);
  assert.throws(
    () =>
      applyServerUpdate(initial.baseline, {
        protocolVersion: 2,
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
            projection: {
              $collectionPatch: 1,
              key: "id",
              upserts: [],
              removed: [99],
            },
          },
        },
        facts: [],
        metrics,
      }),
    (error) =>
      error instanceof GameSessionError && error.code === "protocolMismatch",
  );
});

test("resource revision changes require a matching full resource payload", () => {
  const full: ServerUpdateEnvelope = {
    protocolVersion: 2,
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
        protocolVersion: 2,
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
    protocolVersion: 2,
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
    protocolVersion: 2,
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
        weapon: {
          item: "weapon/breach-scattergun",
          presentation: "breach-scattergun",
          damage: 90,
          ammunition: "ammo/scatter-shell",
          ammunitionCost: 2,
          ammoRemaining: 8,
          ammoCapacity: 50,
          readyAtTick: 36,
        },
        inventory: {
          ...dynamic.inventory,
          equippedWeapon: "weapon/breach-scattergun",
          weapons: [
            {
              ...dynamic.inventory.weapons[0],
              selected: false,
            },
            {
              slot: 1,
              item: "weapon/breach-scattergun",
              owned: true,
              selected: true,
              ammunition: "ammo/scatter-shell",
              ammunitionQuantity: 8,
            },
          ],
        },
      },
    },
    facts: [],
    metrics,
  });

  assert.equal(replacement.baseline.sessionId, "loading-bay-2");
  assert.equal(replacement.state.voxelMeshes, resources.voxelMeshes);
  assert.equal(replacement.state.weapon.item, "weapon/breach-scattergun");
  assert.equal(
    replacement.state.inventory?.equippedWeapon,
    "weapon/breach-scattergun",
  );
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

test("session close settles only after the WebSocket close event", async () => {
  const originalLocation = Object.getOwnPropertyDescriptor(
    globalThis,
    "location",
  );
  const originalWebSocket = Object.getOwnPropertyDescriptor(
    globalThis,
    "WebSocket",
  );
  const sockets: HeldCloseSocket[] = [];

  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: { host: "loading-bay.test", protocol: "http:" },
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: class extends HeldCloseSocket {
      constructor() {
        super();
        sockets.push(this);
      }
    },
  });

  try {
    const session = await LoadingBayGameSession.connect();
    let retired = false;
    const closing = session.close().then(() => {
      retired = true;
    });
    await Promise.resolve();
    assert.equal(sockets[0]?.closeInvoked, true);
    assert.equal(retired, false);
    sockets[0]?.releaseClose();
    await closing;
    assert.equal(retired, true);
  } finally {
    restoreGlobal("location", originalLocation);
    restoreGlobal("WebSocket", originalWebSocket);
  }
});

test("session replacement preparation cancels unsent transient input", async () => {
  const originalLocation = Object.getOwnPropertyDescriptor(
    globalThis,
    "location",
  );
  const originalWebSocket = Object.getOwnPropertyDescriptor(
    globalThis,
    "WebSocket",
  );
  const sockets: InputDiscardSocket[] = [];

  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: { host: "loading-bay.test", protocol: "http:" },
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: class extends InputDiscardSocket {
      constructor() {
        super();
        sockets.push(this);
      }
    },
  });

  try {
    const session = await LoadingBayGameSession.connect();
    session.queueInput({
      movement: [1, 0],
      lookDelta: [0.25, -0.25],
      primaryFireHeld: true,
    });
    session.discardInputForSessionReplacement();
    await new Promise((resolve) => setTimeout(resolve, 25));
    assert.deepEqual(sockets[0]?.sentCommands, []);
    assert.equal(session.pendingInputFrameCount, 0);
    await session.close();
  } finally {
    restoreGlobal("location", originalLocation);
    restoreGlobal("WebSocket", originalWebSocket);
  }
});

test("a rejected delta requests an independent full state and settles input recovery", async () => {
  const originalLocation = Object.getOwnPropertyDescriptor(
    globalThis,
    "location",
  );
  const originalWebSocket = Object.getOwnPropertyDescriptor(
    globalThis,
    "WebSocket",
  );
  const sockets: ResyncRecoverySocket[] = [];

  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: { host: "loading-bay.test", protocol: "http:" },
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: class extends ResyncRecoverySocket {
      constructor() {
        super();
        sockets.push(this);
      }
    },
  });

  try {
    const session = await LoadingBayGameSession.connect();
    const recovered = await session.sendInput({
      movement: [1, 0],
      lookDelta: [0.25, -0.25],
      primaryFireHeld: false,
    });
    assert.equal(recovered.tick, 4);
    assert.equal(session.snapshotSequence, 4);
    assert.equal(session.pendingInputFrameCount, 0);
    assert.deepEqual(
      sockets[0]?.sentEnvelopes.map((envelope) => ({
        requestFullState: envelope.requestFullState,
        kind: envelope.command.kind,
      })),
      [
        { requestFullState: false, kind: "setInputIntent" },
        { requestFullState: true, kind: "requestFullState" },
      ],
    );

    const paused = await session.sendEdge({ kind: "setPaused", paused: true });
    assert.equal(paused.input.paused, true);
    assert.equal(session.pendingEdgeCount, 0);
    await session.close();
  } finally {
    restoreGlobal("location", originalLocation);
    restoreGlobal("WebSocket", originalWebSocket);
  }
});

test("a fixed-tick restart rejection settles and releases the restart slot", async () => {
  const originalLocation = Object.getOwnPropertyDescriptor(
    globalThis,
    "location",
  );
  const originalWebSocket = Object.getOwnPropertyDescriptor(
    globalThis,
    "WebSocket",
  );

  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: { host: "loading-bay.test", protocol: "http:" },
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: RestartRejectionSocket,
  });

  try {
    const session = await LoadingBayGameSession.connect();
    await assert.rejects(
      session.sendEdge({ kind: "restart", mode: "authoredBaseline" }),
      (error) => error instanceof GameSessionError && error.code === "paused",
    );
    assert.equal(session.serverTick, 2);
    assert.equal(session.snapshotSequence, 2);
    assert.notEqual(session.lastSnapshotCadenceMilliseconds, null);
    const replacementFailures: string[] = [];
    session.setFailureListener((error) => {
      replacementFailures.push(error.code);
    });

    const restarted = await session.sendEdge({
      kind: "restart",
      mode: "authoredBaseline",
    });
    assert.equal(restarted.tick, 0);
    assert.equal(restarted.input.connectionGeneration, 2);
    assert.equal(session.serverTick, 0);
    assert.equal(session.snapshotSequence, 1);
    assert.equal(session.lastSnapshotCadenceMilliseconds, null);
    await Promise.resolve();
    await Promise.resolve();
    assert.deepEqual(replacementFailures, []);
    await session.close();
  } finally {
    restoreGlobal("location", originalLocation);
    restoreGlobal("WebSocket", originalWebSocket);
  }
});

test("load commands settle only from the replacement authoritative session", async () => {
  const originalLocation = Object.getOwnPropertyDescriptor(
    globalThis,
    "location",
  );
  const originalWebSocket = Object.getOwnPropertyDescriptor(
    globalThis,
    "WebSocket",
  );

  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: { host: "loading-bay.test", protocol: "http:" },
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: LoadReplacementSocket,
  });

  try {
    const session = await LoadingBayGameSession.connect();
    const deliveries: boolean[] = [];
    session.setStateListener((_state, delivery) => {
      deliveries.push(delivery.sessionReplaced);
    });
    const loading = session.sendEdge({
      kind: "loadGame",
      slot: "slot1",
      expectedStorageRevision: "fnv1a64:save",
    });
    await assert.rejects(
      session.sendEdge({
        kind: "saveGame",
        slot: "slot2",
        overwrite: false,
        expectedStorageRevision: null,
      }),
      (error) =>
        error instanceof GameSessionError &&
        error.code === "edgeQueueSaturated",
    );
    const loaded = await loading;
    assert.equal(loaded.tick, 37);
    assert.equal(loaded.input.connectionGeneration, 2);
    assert.equal(session.pendingEdgeCount, 0);
    assert.deepEqual(deliveries, [true]);
    await session.close();
  } finally {
    restoreGlobal("location", originalLocation);
    restoreGlobal("WebSocket", originalWebSocket);
  }
});

test("save rejection codes settle from immediate and fixed-tick paths without closing the session", async () => {
  const originalLocation = Object.getOwnPropertyDescriptor(
    globalThis,
    "location",
  );
  const originalWebSocket = Object.getOwnPropertyDescriptor(
    globalThis,
    "WebSocket",
  );

  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: { host: "loading-bay.test", protocol: "http:" },
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: SaveRejectionSocket,
  });

  try {
    const session = await LoadingBayGameSession.connect();
    const immediateCodes = [
      "saveUnavailable",
      "saveStale",
      "snapshotCorrupt",
      "snapshotIncompatible",
    ] as const;
    for (const code of immediateCodes) {
      await assert.rejects(
        session.sendEdge({
          kind: "loadGame",
          slot: "slot1",
          expectedStorageRevision: "fnv1a64:observed",
        }),
        (error) => error instanceof GameSessionError && error.code === code,
      );
      assert.equal(session.pendingEdgeCount, 0);
    }
    await assert.rejects(
      session.sendEdge({
        kind: "saveGame",
        slot: "slot1",
        overwrite: false,
        expectedStorageRevision: "fnv1a64:observed",
      }),
      (error) =>
        error instanceof GameSessionError &&
        error.code === "saveOverwriteRequired",
    );
    assert.equal(session.pendingEdgeCount, 0);

    const paused = await session.sendEdge({ kind: "setPaused", paused: true });
    assert.equal(paused.input.paused, true);
    assert.equal(session.pendingEdgeCount, 0);
    await session.close();
  } finally {
    restoreGlobal("location", originalLocation);
    restoreGlobal("WebSocket", originalWebSocket);
  }
});

test("typed weapon-slot edges settle from the authoritative equipped projection", async () => {
  const originalLocation = Object.getOwnPropertyDescriptor(
    globalThis,
    "location",
  );
  const originalWebSocket = Object.getOwnPropertyDescriptor(
    globalThis,
    "WebSocket",
  );

  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: { host: "loading-bay.test", protocol: "http:" },
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: SelectionSocket,
  });

  try {
    const session = await LoadingBayGameSession.connect();
    const selected = await session.sendEdge({
      kind: "selectWeaponSlot",
      slot: 1,
    });
    assert.equal(selected.weapon.item, "weapon/breach-scattergun");
    assert.equal(selected.weapon.ammunition, "ammo/scatter-shell");
    assert.equal(
      selected.inventory?.equippedWeapon,
      "weapon/breach-scattergun",
    );
    assert.equal(
      selected.inventory?.weapons.find((weapon) => weapon.slot === 1)?.selected,
      true,
    );
    await session.close();
  } finally {
    restoreGlobal("location", originalLocation);
    restoreGlobal("WebSocket", originalWebSocket);
  }
});

test("typed item rejections settle without closing the live session", async () => {
  const originalLocation = Object.getOwnPropertyDescriptor(
    globalThis,
    "location",
  );
  const originalWebSocket = Object.getOwnPropertyDescriptor(
    globalThis,
    "WebSocket",
  );

  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: { host: "loading-bay.test", protocol: "http:" },
  });
  Object.defineProperty(globalThis, "WebSocket", {
    configurable: true,
    value: HealthFullRejectionSocket,
  });

  try {
    const session = await LoadingBayGameSession.connect();
    await assert.rejects(
      session.sendEdge({ kind: "useItem", item: "supply/med-patch" }),
      (error) =>
        error instanceof GameSessionError && error.code === "healthFull",
    );
    assert.equal(session.state.tick, 2);
    assert.equal(session.state.input.connected, true);
    assert.equal(session.pendingEdgeCount, 0);
    await session.close();
  } finally {
    restoreGlobal("location", originalLocation);
    restoreGlobal("WebSocket", originalWebSocket);
  }
});

class HeldCloseSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readyState = HeldCloseSocket.OPEN;
  closeInvoked = false;

  constructor() {
    super();
    queueMicrotask(() => {
      this.dispatchEvent(
        new MessageEvent("message", {
          data: JSON.stringify({
            protocolVersion: 2,
            sessionId: "loading-bay-held-close",
            connectionGeneration: 1,
            serverTick: 1,
            snapshotSequence: 1,
            acknowledgedCommandSequence: 0,
            staticRevision: resources.staticRevision,
            update: { kind: "full", state: dynamic },
            resources,
            facts: [],
            metrics,
          } satisfies ServerUpdateEnvelope),
        }),
      );
    });
  }

  close(): void {
    this.closeInvoked = true;
  }

  releaseClose(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }
}

class SelectionSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readyState = SelectionSocket.OPEN;

  constructor() {
    super();
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
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
    });
  }

  send(payload: string): void {
    const envelope = JSON.parse(payload) as {
      readonly sequence: number;
      readonly command: { readonly kind: string; readonly slot?: number };
    };
    assert.deepEqual(envelope.command, {
      kind: "selectWeaponSlot",
      slot: 1,
    });
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
        sessionId: "loading-bay-1",
        connectionGeneration: 1,
        serverTick: 2,
        snapshotSequence: 2,
        acknowledgedCommandSequence: envelope.sequence,
        staticRevision: resources.staticRevision,
        update: {
          kind: "delta",
          baseSnapshotSequence: 1,
          changes: {
            tick: 2,
            input: {
              ...dynamic.input,
              acknowledgedSequence: envelope.sequence,
              consumedSequence: envelope.sequence,
            },
            weapon: {
              item: "weapon/breach-scattergun",
              presentation: "breach-scattergun",
              damage: 90,
              ammunition: "ammo/scatter-shell",
              ammunitionCost: 2,
              ammoRemaining: 8,
              ammoCapacity: 50,
              readyAtTick: 0,
            },
            inventory: {
              ...dynamic.inventory,
              stacks: [
                ...dynamic.inventory.stacks,
                { item: "weapon/breach-scattergun", quantity: 1 },
                { item: "ammo/scatter-shell", quantity: 8 },
              ],
              equippedWeapon: "weapon/breach-scattergun",
              weapons: [
                {
                  ...dynamic.inventory.weapons[0],
                  selected: false,
                },
                {
                  slot: 1,
                  item: "weapon/breach-scattergun",
                  owned: true,
                  selected: true,
                  ammunition: "ammo/scatter-shell",
                  ammunitionQuantity: 8,
                },
              ],
            },
          },
        },
        facts: [{ kind: "InventoryWeaponSelected" }],
        metrics,
      });
    });
  }

  close(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }

  #emit(envelope: ServerUpdateEnvelope): void {
    this.dispatchEvent(
      new MessageEvent("message", { data: JSON.stringify(envelope) }),
    );
  }
}

class HealthFullRejectionSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readyState = HealthFullRejectionSocket.OPEN;

  constructor() {
    super();
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
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
    });
  }

  send(payload: string): void {
    const envelope = JSON.parse(payload) as {
      readonly sequence: number;
      readonly command: { readonly kind: string; readonly item?: string };
    };
    assert.deepEqual(envelope.command, {
      kind: "useItem",
      item: "supply/med-patch",
    });
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
        sessionId: "loading-bay-1",
        connectionGeneration: 1,
        serverTick: 2,
        snapshotSequence: 2,
        acknowledgedCommandSequence: envelope.sequence,
        staticRevision: resources.staticRevision,
        update: {
          kind: "delta",
          baseSnapshotSequence: 1,
          changes: {
            tick: 2,
            input: {
              ...dynamic.input,
              acknowledgedSequence: envelope.sequence,
              consumedSequence: envelope.sequence,
            },
          },
        },
        facts: [
          {
            kind: "InputEdgeRejectedHealthFull",
            code: "healthFull",
            commandSequence: envelope.sequence,
          },
        ],
        metrics,
      });
    });
  }

  close(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }

  #emit(envelope: ServerUpdateEnvelope): void {
    this.dispatchEvent(
      new MessageEvent("message", { data: JSON.stringify(envelope) }),
    );
  }
}

class InputDiscardSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readonly sentCommands: string[] = [];
  readyState = InputDiscardSocket.OPEN;

  constructor() {
    super();
    queueMicrotask(() => {
      this.dispatchEvent(
        new MessageEvent("message", {
          data: JSON.stringify({
            protocolVersion: 2,
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
          } satisfies ServerUpdateEnvelope),
        }),
      );
    });
  }

  send(payload: string): void {
    const envelope = JSON.parse(payload) as {
      readonly command: { readonly kind: string };
    };
    this.sentCommands.push(envelope.command.kind);
  }

  close(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }
}

class ResyncRecoverySocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readonly sentEnvelopes: {
    readonly sequence: number;
    readonly requestFullState: boolean;
    readonly command: { readonly kind: string };
  }[] = [];
  readyState = ResyncRecoverySocket.OPEN;

  constructor() {
    super();
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
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
    });
  }

  send(payload: string): void {
    const envelope = JSON.parse(payload) as {
      readonly sequence: number;
      readonly requestFullState: boolean;
      readonly command: { readonly kind: string };
    };
    this.sentEnvelopes.push(envelope);
    if (envelope.command.kind === "setInputIntent") {
      queueMicrotask(() => {
        this.#emit({
          protocolVersion: 2,
          sessionId: "loading-bay-1",
          connectionGeneration: 1,
          serverTick: 3,
          snapshotSequence: 3,
          acknowledgedCommandSequence: envelope.sequence,
          staticRevision: resources.staticRevision,
          update: {
            kind: "delta",
            baseSnapshotSequence: 2,
            changes: {
              tick: 3,
              input: {
                ...dynamic.input,
                acknowledgedSequence: envelope.sequence,
                consumedSequence: envelope.sequence,
              },
            },
          },
          facts: [],
          metrics,
        });
      });
      return;
    }
    if (envelope.command.kind === "requestFullState") {
      assert.equal(envelope.requestFullState, true);
      queueMicrotask(() => {
        this.#emit({
          protocolVersion: 2,
          sessionId: "loading-bay-1",
          connectionGeneration: 1,
          serverTick: 4,
          snapshotSequence: 4,
          acknowledgedCommandSequence: 1,
          staticRevision: resources.staticRevision,
          update: {
            kind: "full",
            state: {
              ...dynamic,
              tick: 4,
              input: {
                ...dynamic.input,
                acknowledgedSequence: 1,
                consumedSequence: 1,
              },
            },
          },
          facts: [],
          metrics,
        });
      });
      return;
    }

    assert.equal(envelope.command.kind, "setPaused");
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
        sessionId: "loading-bay-1",
        connectionGeneration: 1,
        serverTick: 5,
        snapshotSequence: 5,
        acknowledgedCommandSequence: envelope.sequence,
        staticRevision: resources.staticRevision,
        update: {
          kind: "delta",
          baseSnapshotSequence: 4,
          changes: {
            tick: 5,
            input: {
              ...dynamic.input,
              acknowledgedSequence: envelope.sequence,
              consumedSequence: envelope.sequence,
              paused: true,
            },
          },
        },
        facts: [],
        metrics,
      });
    });
  }

  close(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }

  #emit(envelope: ServerUpdateEnvelope): void {
    this.dispatchEvent(
      new MessageEvent("message", { data: JSON.stringify(envelope) }),
    );
  }
}

class RestartRejectionSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readyState = RestartRejectionSocket.OPEN;
  sentRestartCount = 0;

  constructor() {
    super();
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
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
    });
  }

  send(payload: string): void {
    const command = JSON.parse(payload) as {
      readonly sequence: number;
      readonly command: { readonly kind: string };
    };
    assert.equal(command.command.kind, "restart");
    this.sentRestartCount += 1;

    if (this.sentRestartCount === 1) {
      queueMicrotask(() => {
        this.#emit({
          protocolVersion: 2,
          sessionId: "loading-bay-1",
          connectionGeneration: 1,
          serverTick: 2,
          snapshotSequence: 2,
          acknowledgedCommandSequence: command.sequence,
          staticRevision: resources.staticRevision,
          update: {
            kind: "delta",
            baseSnapshotSequence: 1,
            changes: {
              tick: 2,
              input: {
                ...dynamic.input,
                acknowledgedSequence: command.sequence,
                consumedSequence: command.sequence,
                paused: true,
              },
            },
          },
          facts: [
            {
              kind: "InputEdgeRejectedPaused",
              code: "paused",
              commandSequence: command.sequence,
            },
          ],
          metrics,
        });
      });
      return;
    }

    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
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
            input: {
              ...dynamic.input,
              connectionGeneration: 2,
            },
          },
        },
        facts: [],
        metrics,
      });
      queueMicrotask(() => {
        this.#emit({
          protocolVersion: 2,
          sessionId: "loading-bay-2",
          commandSequence: command.sequence + 1,
          acknowledgedCommandSequence: 0,
          code: "sessionClosed",
          retry: "reconnect",
          message: "command belongs to a replaced session",
        });
      });
    });
  }

  close(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }

  #emit(envelope: object): void {
    this.dispatchEvent(
      new MessageEvent("message", { data: JSON.stringify(envelope) }),
    );
  }
}

class LoadReplacementSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readyState = LoadReplacementSocket.OPEN;

  constructor() {
    super();
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
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
    });
  }

  send(payload: string): void {
    const command = JSON.parse(payload) as {
      readonly sequence: number;
      readonly command: {
        readonly kind: string;
        readonly slot: string;
        readonly expectedStorageRevision: string | null;
      };
    };
    assert.deepEqual(command.command, {
      kind: "loadGame",
      slot: "slot1",
      expectedStorageRevision: "fnv1a64:save",
    });
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
        sessionId: "loading-bay-2",
        connectionGeneration: 2,
        serverTick: 37,
        snapshotSequence: 1,
        acknowledgedCommandSequence: 0,
        staticRevision: resources.staticRevision,
        update: {
          kind: "full",
          state: {
            ...dynamic,
            tick: 37,
            input: {
              ...dynamic.input,
              connectionGeneration: 2,
            },
          },
        },
        facts: [],
        metrics,
      });
    });
  }

  close(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }

  #emit(envelope: ServerUpdateEnvelope): void {
    this.dispatchEvent(
      new MessageEvent("message", { data: JSON.stringify(envelope) }),
    );
  }
}

class SaveRejectionSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readyState = SaveRejectionSocket.OPEN;
  #commandCount = 0;
  #snapshotSequence = 1;

  constructor() {
    super();
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
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
    });
  }

  send(payload: string): void {
    const envelope = JSON.parse(payload) as {
      readonly sequence: number;
      readonly command: { readonly kind: string };
    };
    this.#commandCount += 1;
    const immediateCodes = [
      "saveUnavailable",
      "saveStale",
      "snapshotCorrupt",
      "snapshotIncompatible",
    ] as const;
    const immediateCode = immediateCodes[this.#commandCount - 1];
    if (immediateCode !== undefined) {
      assert.equal(envelope.command.kind, "loadGame");
      queueMicrotask(() => {
        this.#emit({
          protocolVersion: 2,
          sessionId: "loading-bay-1",
          commandSequence: envelope.sequence,
          acknowledgedCommandSequence: 0,
          code: immediateCode,
          retry: "never",
          message: immediateCode,
        });
      });
      return;
    }

    this.#snapshotSequence += 1;
    if (this.#commandCount === 5) {
      assert.equal(envelope.command.kind, "saveGame");
      queueMicrotask(() => {
        this.#emit({
          protocolVersion: 2,
          sessionId: "loading-bay-1",
          connectionGeneration: 1,
          serverTick: 2,
          snapshotSequence: this.#snapshotSequence,
          acknowledgedCommandSequence: envelope.sequence,
          staticRevision: resources.staticRevision,
          update: {
            kind: "delta",
            baseSnapshotSequence: this.#snapshotSequence - 1,
            changes: {
              tick: 2,
              input: {
                ...dynamic.input,
                acknowledgedSequence: envelope.sequence,
                consumedSequence: envelope.sequence,
              },
            },
          },
          facts: [
            {
              kind: "SaveRejectedOverwriteRequired",
              code: "saveOverwriteRequired",
              commandSequence: envelope.sequence,
            },
          ],
          metrics,
        });
      });
      return;
    }

    assert.equal(envelope.command.kind, "setPaused");
    queueMicrotask(() => {
      this.#emit({
        protocolVersion: 2,
        sessionId: "loading-bay-1",
        connectionGeneration: 1,
        serverTick: 3,
        snapshotSequence: this.#snapshotSequence,
        acknowledgedCommandSequence: envelope.sequence,
        staticRevision: resources.staticRevision,
        update: {
          kind: "delta",
          baseSnapshotSequence: this.#snapshotSequence - 1,
          changes: {
            tick: 3,
            input: {
              ...dynamic.input,
              acknowledgedSequence: envelope.sequence,
              consumedSequence: envelope.sequence,
              paused: true,
            },
          },
        },
        facts: [],
        metrics,
      });
    });
  }

  close(): void {
    this.readyState = 3;
    this.dispatchEvent(new Event("close"));
  }

  #emit(value: unknown): void {
    this.dispatchEvent(
      new MessageEvent("message", { data: JSON.stringify(value) }),
    );
  }
}

function staticResourceFixture() {
  return {
    ...resources,
    renderMaterials: [
      {
        schemaVersion: 1,
        id: "material/prop-kit/test-prop",
        color: [0.3, 0.4, 0.5, 1],
        texture: null,
        roughness: 0.8,
        textureTint: [1, 1, 1, 1],
        emissionColor: [0, 0, 0],
        emissionIntensity: 0,
        uvStrategy: "flat",
      },
    ],
    staticMeshes: [
      {
        asset: "mesh/prop-kit/test-prop",
        payload: {
          layout: {
            vertexCount: 3,
            indexCount: 3,
            indexWidth: "u32",
            attributes: [
              { name: "position", components: 3, kind: "f32" },
              { name: "normal", components: 3, kind: "f32" },
            ],
          },
          groups: [{ materialSlot: 0, start: 0, count: 3 }],
          bounds: { min: [-0.1, -0.1, 0], max: [0.1, 0.1, 0] },
          source: {
            kind: "inline",
            positions: [-0.1, -0.1, 0, 0.1, -0.1, 0, 0, 0.1, 0],
            normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
            indices: [0, 1, 2],
          },
          provenance: "generated",
        },
        materialSlots: [{ slot: 0, material: "material/prop-kit/test-prop" }],
        collision: { kind: "visualOnly" },
      },
    ],
  } as const;
}

function restoreGlobal(
  name: "location" | "WebSocket",
  descriptor: PropertyDescriptor | undefined,
): void {
  if (descriptor === undefined) {
    Reflect.deleteProperty(globalThis, name);
  } else {
    Object.defineProperty(globalThis, name, descriptor);
  }
}
