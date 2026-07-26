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

test("legacy projects preserve an absent Rust inventory through browser composition", () => {
  const envelope: ServerUpdateEnvelope = {
    protocolVersion: 1,
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

    const restarted = await session.sendEdge({
      kind: "restart",
      mode: "authoredBaseline",
    });
    assert.equal(restarted.tick, 0);
    assert.equal(restarted.input.connectionGeneration, 2);
    assert.equal(session.serverTick, 0);
    assert.equal(session.snapshotSequence, 1);
    assert.equal(session.lastSnapshotCadenceMilliseconds, null);
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

class SelectionSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readyState = SelectionSocket.OPEN;

  constructor() {
    super();
    queueMicrotask(() => {
      this.#emit({
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
        protocolVersion: 1,
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

class RestartRejectionSocket extends EventTarget {
  static readonly OPEN = 1;
  readonly bufferedAmount = 0;
  readyState = RestartRejectionSocket.OPEN;
  sentRestartCount = 0;

  constructor() {
    super();
    queueMicrotask(() => {
      this.#emit({
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
          protocolVersion: 1,
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
