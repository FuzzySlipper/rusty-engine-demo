import assert from "node:assert/strict";
import test from "node:test";

import {
  RuntimeProjectionAdapter,
  derivePlayerCameraPose,
  entityHandle,
  type RuntimeBrowserState,
  type RuntimeProjectionNode,
} from "./projection.ts";

function state(
  projection: RuntimeBrowserState["projection"],
): RuntimeBrowserState {
  return {
    hostSessionId: "host-a",
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
      damage: 100,
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
    saveSlots: [],
    extractionBeacon: null,
    doorAccess: [],
    secretRegions: [],
    levelExits: [],
    levelComplete: false,
    interaction: null,
    voxelEnvironmentRole: "visible",
    voxelMeshes: [],
    voxelObjectFrame: { schemaVersion: 1, ops: [] },
    lights: [],
    renderMaterials: [],
    staticMeshes: [],
    generatedEnvironment: null,
    enemies: [],
    presentation: { animationStates: [], cues: [] },
    lastEvents: [],
  };
}

function serializedState(
  projection: readonly RuntimeProjectionNode[],
): RuntimeBrowserState {
  return {
    ...state(projection),
    renderMaterials: [
      {
        schemaVersion: 1,
        id: "material/test-prop",
        color: [0.4, 0.5, 0.6, 1],
        texture: null,
        roughness: 0.8,
        textureTint: [1, 1, 1, 1],
        emissionColor: [0, 0, 0],
        emissionIntensity: 0,
        uvStrategy: "flat",
      },
    ],
    staticMeshes: projection.map((node) => ({
      asset: node.asset,
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
      materialSlots: [{ slot: 0, material: "material/test-prop" }],
      collision: { kind: "visualOnly" },
    })),
  };
}

test("whole Rust readouts become create update and destroy diffs", () => {
  const adapter = new RuntimeProjectionAdapter();
  const original = {
    id: 3,
    name: "exit",
    asset: "mesh/bay-rusher",
    translation: [0, 0, 8] as const,
    visible: true,
    visualState: "default" as const,
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

test("a non-actor appearance cannot fall back when its serialized mesh is absent", () => {
  const missing = {
    id: 20,
    name: "arrival-energy-cache",
    asset: "mesh/prop-kit/energy-cell",
    translation: [4.5, 1.5, 9.5] as const,
    visible: true,
    visualState: "available" as const,
  };
  assert.throws(
    () => new RuntimeProjectionAdapter().apply(state([missing])),
    /missing canonical static mesh mesh\/prop-kit\/energy-cell/,
  );
});

test("a changed serialized definition destroys live instances before redefine and recreate", () => {
  const adapter = new RuntimeProjectionAdapter();
  const node = {
    id: 20,
    name: "arrival-energy-cache",
    asset: "mesh/prop-kit/energy-cell",
    translation: [4.5, 1.5, 9.5] as const,
    visible: true,
    visualState: "available" as const,
  };
  const initial = adapter.apply(serializedState([node]));
  initial.commit();

  const nextState = serializedState([node]);
  const originalAsset = nextState.staticMeshes[0];
  assert.ok(originalAsset);
  const changed = {
    ...nextState,
    staticMeshes: [
      {
        ...originalAsset,
        payload: {
          ...originalAsset.payload,
          bounds: {
            min: [-0.2, -0.1, 0] as const,
            max: [0.2, 0.1, 0] as const,
          },
        },
      },
    ],
  };
  const replacement = adapter.apply(changed);

  assert.deepEqual(
    replacement.ops.map((operation) => operation.op),
    [
      "destroy",
      "defineStaticMesh",
      "createStaticMeshInstance",
      "setMaterialInstanceParameters",
    ],
  );
  assert.equal(
    replacement.ops[0]?.op === "destroy" ? replacement.ops[0].handle : null,
    entityHandle(node.id),
  );
  assert.equal(
    replacement.ops[2]?.op === "createStaticMeshInstance"
      ? replacement.ops[2].handle
      : null,
    entityHandle(node.id),
  );
});

test("accepted immutable mesh resources are not re-fingerprinted on dynamic frames", () => {
  const node = {
    id: 20,
    name: "arrival-energy-cache",
    asset: "mesh/prop-kit/energy-cell",
    translation: [4.5, 1.5, 9.5] as const,
    visible: true,
    visualState: "available" as const,
  };
  const source = serializedState([node]);
  let traversals = 0;
  const observedAsset = new Proxy(source.staticMeshes[0]!, {
    ownKeys(target) {
      traversals += 1;
      return Reflect.ownKeys(target);
    },
  });
  const staticMeshes = [observedAsset];
  const adapter = new RuntimeProjectionAdapter();
  const initial = adapter.apply({ ...source, staticMeshes });
  initial.commit();
  assert.ok(traversals > 0, "initial admission fingerprints the resource");
  const initialTraversals = traversals;

  const dynamic = adapter.apply({ ...source, tick: 2, staticMeshes });
  dynamic.commit();
  assert.equal(
    traversals,
    initialTraversals,
    "dynamic-only projection must not traverse immutable geometry",
  );
  assert.deepEqual(dynamic.ops, []);
});

test("enemy archetype assets project distinct silhouettes and materials", () => {
  const plan = new RuntimeProjectionAdapter().apply(
    state([
      {
        id: 4,
        name: "sentry-alpha",
        asset: "mesh/bay-rusher",
        translation: [1.5, 1.5, 6.5],
        visible: true,
        visualState: "default",
      },
      {
        id: 5,
        name: "sentry-beta",
        asset: "mesh/arc-warden",
        translation: [6.5, 1.5, 2.5],
        visible: true,
        visualState: "default",
      },
    ]),
  );
  const nodes = plan.ops.flatMap((operation) =>
    operation.op === "create" ? [operation.node] : [],
  );

  assert.equal(nodes[0]?.geometry.kind, "cube");
  assert.deepEqual(nodes[0]?.transform.scale, [1.45, 1.25, 1.45]);
  assert.deepEqual(nodes[0]?.material.color, [0.95, 0.34, 0.12, 1]);
  assert.equal(nodes[1]?.geometry.kind, "sphere");
  assert.deepEqual(nodes[1]?.transform.scale, [0.85, 2.35, 0.85]);
  assert.deepEqual(nodes[1]?.material.color, [0.55, 0.25, 0.95, 1]);
});

test("collecting a pickup destroys only its retained entity handle", () => {
  const adapter = new RuntimeProjectionAdapter();
  const door = {
    id: 3,
    name: "exit",
    asset: "mesh/bay-rusher",
    translation: [0, 0, 8] as const,
    visible: true,
    visualState: "default" as const,
  };
  const pickup = {
    id: 22,
    name: "scatter-shell-cache",
    asset: "mesh/arc-warden",
    translation: [4.5, 1.5, 2.5] as const,
    visible: true,
    visualState: "default" as const,
  };
  const created = adapter.apply(state([door, pickup]));
  created.commit();

  const collected = adapter.apply(state([door]));
  assert.deepEqual(
    collected.ops.map((operation) => [
      operation.op,
      operation.op === "destroy" ? operation.handle : null,
    ]),
    [["destroy", entityHandle(pickup.id)]],
  );
  collected.commit();
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

test("canonical voxel-object structure is installed once and cannot drift in place", () => {
  const adapter = new RuntimeProjectionAdapter();
  const material = {
    schemaVersion: 1 as const,
    id: "material/voxel-object-test",
    color: [0.3, 0.4, 0.5, 1] as const,
    texture: null,
    roughness: 0.8,
    textureTint: [1, 1, 1, 1] as const,
    emissionColor: [0, 0, 0] as const,
    emissionIntensity: 0,
    uvStrategy: "flat" as const,
  };
  const structuralState = {
    ...state([]),
    voxelEnvironmentRole: "gameplayProxy" as const,
    voxelObjectFrame: {
      schemaVersion: 1 as const,
      ops: [{ op: "defineMaterial" as const, material }],
    },
  };

  const initial = adapter.apply(structuralState);
  assert.deepEqual(initial.ops, structuralState.voxelObjectFrame.ops);
  initial.commit();

  const dynamic = adapter.apply({ ...structuralState, tick: 1 });
  assert.deepEqual(dynamic.ops, []);
  dynamic.commit();

  const reorderedMaterial = {
    uvStrategy: "flat" as const,
    textureTint: [1, 1, 1, 1] as const,
    texture: null,
    schemaVersion: 1 as const,
    roughness: 0.8,
    id: "material/voxel-object-test",
    emissionIntensity: 0,
    emissionColor: [0, 0, 0] as const,
    color: [0.3, 0.4, 0.5, 1] as const,
  };
  const reorderedFrame = {
    ops: [
      {
        material: reorderedMaterial,
        op: "defineMaterial" as const,
      },
    ],
    schemaVersion: 1 as const,
  };
  assert.notEqual(
    JSON.stringify(reorderedFrame),
    JSON.stringify(structuralState.voxelObjectFrame),
  );
  const semanticallyUnchanged = adapter.apply({
    ...structuralState,
    tick: 2,
    voxelObjectFrame: reorderedFrame,
  });
  assert.deepEqual(semanticallyUnchanged.ops, []);
  semanticallyUnchanged.commit();

  assert.throws(
    () =>
      adapter.apply({
        ...structuralState,
        voxelObjectFrame: {
          schemaVersion: 1,
          ops: [
            {
              op: "defineMaterial",
              material: { ...material, roughness: 0.2 },
            },
          ],
        },
      }),
    /changed without a renderer session replacement/,
  );
});

test("authored lights use retained shared-renderer light operations", () => {
  const adapter = new RuntimeProjectionAdapter();
  const ambient = {
    id: 80,
    translation: null,
    rotation: [0, 0, 0, 1] as const,
    light: {
      kind: "ambient" as const,
      color: [0.16, 0.2, 0.28] as const,
      intensity: 0.6,
      enabled: true,
      shadows: false,
    },
  };
  const point = {
    id: 81,
    translation: [7.5, 3.3, 9.5] as const,
    rotation: [0, 0, 0, 1] as const,
    light: {
      kind: "point" as const,
      color: [0.98, 0.68, 0.35] as const,
      intensity: 1.3,
      enabled: true,
      range: 15,
      decay: 2,
      shadows: false,
    },
  };

  const created = adapter.apply({ ...state([]), lights: [ambient, point] });
  assert.deepEqual(
    created.ops.map((operation) => operation.op),
    ["createLight", "createLight"],
  );
  assert.deepEqual(
    created.ops[1]?.op === "createLight" ? created.ops[1].light : null,
    {
      kind: "point",
      color: [0.98, 0.68, 0.35],
      intensity: 1.3,
      enabled: true,
      position: [7.5, 3.3, 9.5],
      range: 15,
      decay: 2,
      shadowIntent: "disabled",
    },
  );
  created.commit();

  const updated = adapter.apply({
    ...state([]),
    lights: [ambient, { ...point, translation: [8.5, 3.3, 9.5] as const }],
  });
  assert.deepEqual(
    updated.ops.map((operation) => operation.op),
    ["updateLight"],
  );
  updated.commit();

  const destroyed = adapter.apply({ ...state([]), lights: [] });
  assert.deepEqual(
    destroyed.ops.map((operation) => operation.op),
    ["destroy", "destroy"],
  );
  destroyed.commit();
  assert.equal(adapter.trackedLightCount, 0);
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
    asset: "mesh/player-marker",
    translation: [0.5, 0.5, 0.5] as const,
    visible: true,
    visualState: "default" as const,
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
    visualState: "standby" as const,
  };
  const standby = {
    ...serializedState([beacon]),
    extractionBeacon: {
      id: 7,
      state: "standby" as const,
      activationRadius: 2.5,
      activatedBy: null,
      activatedAtTick: null,
    },
  };

  const standbyPlan = adapter.apply(standby);
  const standbyCreated = standbyPlan.ops.find(
    (operation) => operation.op === "createStaticMeshInstance",
  );
  standbyPlan.commit();
  const activePlan = adapter.apply({
    ...standby,
    projection: [{ ...beacon, visualState: "active" as const }],
    extractionBeacon: {
      ...standby.extractionBeacon,
      state: "active",
      activatedBy: 1,
      activatedAtTick: 9,
    },
  });
  const active = activePlan.ops.find(
    (operation) => operation.op === "setMaterialInstanceParameters",
  );

  assert.deepEqual(
    standbyCreated?.op === "createStaticMeshInstance"
      ? standbyCreated.instance.asset
      : null,
    "mesh/extraction-beacon",
  );
  assert.deepEqual(
    active?.op === "setMaterialInstanceParameters"
      ? active.parameters?.emissionIntensity
      : null,
    0.35,
  );
  activePlan.commit();
});

test("rejected create update destroy and mesh plans remain retryable until commit", () => {
  const adapter = new RuntimeProjectionAdapter();
  const original = {
    id: 3,
    name: "exit",
    asset: "mesh/bay-rusher",
    translation: [0, 0, 8] as const,
    visible: true,
    visualState: "default" as const,
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
