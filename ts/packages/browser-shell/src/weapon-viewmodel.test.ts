import assert from "node:assert/strict";
import test from "node:test";

import type {
  RenderDiff,
  StaticMeshAsset,
} from "@rusty-engine/render-contracts";
import { RenderProjection } from "@rusty-engine/render-projection";
import { createRendererSurfaceProjection } from "@rusty-engine/renderer-host";

import type { RuntimeBrowserState } from "./projection.ts";
import { WeaponViewmodelAdapter } from "./weapon-viewmodel.ts";

test("the authoritative equipped weapon creates one bounded viewmodel hierarchy", () => {
  const adapter = new WeaponViewmodelAdapter();
  const plan = adapter.project(state());
  const groups = plan.ops.filter(
    (operation): operation is Extract<RenderDiff, { readonly op: "create" }> =>
      operation.op === "create",
  );
  const meshes = plan.ops.filter(
    (
      operation,
    ): operation is Extract<
      RenderDiff,
      { readonly op: "createStaticMeshInstance" }
    > => operation.op === "createStaticMeshInstance",
  );

  assert.equal(plan.ops.length, 3);
  assert.equal(groups.length, 1);
  assert.equal(groups[0]?.parent, null);
  assert.equal(groups[0]?.node.geometry.kind, "group");
  assert.equal(groups[0]?.node.layer, "viewmodel");
  assert.equal(meshes.length, 2);
  assert.ok(
    meshes.every((operation) => operation.parent === groups[0]?.handle),
  );
  assert.ok(
    meshes
      .flatMap((operation) => operation.instance.metadata.tags)
      .includes("weapon/arc-pistol"),
  );
  assert.deepEqual(
    meshes.map((operation) => operation.instance.asset),
    ["mesh/prop-kit/arc-pistol", "mesh/prop-kit/muzzle-flash"],
  );
  plan.commit();
  assert.deepEqual(adapter.readout(), {
    bobPhase: 0,
    impulse: "idle",
    liveNodeCount: 3,
    mounted: true,
    visible: true,
    weapon: "weapon/arc-pistol",
  });
});

test("the exact shared surface projection accepts the serialized retained viewmodel layer", () => {
  const plan = new WeaponViewmodelAdapter().project(state());
  const projection = createRendererSurfaceProjection({
    schemaVersion: 1,
    ops: [...testAssetDefinitions(), ...plan.ops],
  });
  assert.equal(projection.snapshot.nodes.length, 3);
  assert.ok(
    projection.snapshot.nodes.every((node) => node.layer === "viewmodel"),
  );
  assert.deepEqual(
    projection.snapshot.nodes.flatMap((node) =>
      node.kind === "staticMesh" ? [node.asset] : [],
    ),
    ["mesh/prop-kit/arc-pistol", "mesh/prop-kit/muzzle-flash"],
  );
  assert.equal(
    projection.snapshot.nodes.find(
      (node) => node.metadata.label === "loading-bay-viewmodel-root",
    )?.parent,
    null,
  );
});

test("three original weapons select distinct serialized project assets", () => {
  const adapter = new WeaponViewmodelAdapter();
  const projection = new RenderProjection();
  projection.applyFrame({ schemaVersion: 1, ops: testAssetDefinitions() });
  const baseline = state();
  if (baseline.inventory === null) {
    throw new Error("weapon viewmodel fixture requires authored inventory");
  }
  const initial = adapter.project(baseline);
  projection.applyFrame(initial);
  initial.commit();

  const scatter = adapter.project(
    state({
      weapon: {
        ...baseline.weapon,
        item: "weapon/breach-scattergun",
        presentation: "breach-scattergun",
        ammunition: "ammo/scatter-shell",
      },
      inventory: {
        ...baseline.inventory,
        equippedWeapon: "weapon/breach-scattergun",
      },
    }),
  );
  assert.deepEqual(
    scatter.ops.map((operation) => operation.op),
    ["update", "destroy", "createStaticMeshInstance", "update"],
  );
  assert.equal(
    scatter.ops.find((operation) => operation.op === "createStaticMeshInstance")
      ?.op === "createStaticMeshInstance"
      ? scatter.ops.find(
          (operation) => operation.op === "createStaticMeshInstance",
        )?.instance.asset
      : null,
    "mesh/prop-kit/breach-scattergun",
  );
  projection.applyFrame(scatter);
  scatter.commit();

  const carbine = adapter.project(
    state({
      weapon: {
        ...baseline.weapon,
        item: "weapon/rivet-carbine",
        presentation: "rivet-carbine",
      },
      inventory: {
        ...baseline.inventory,
        equippedWeapon: "weapon/rivet-carbine",
      },
    }),
  );
  assert.equal(
    carbine.ops.find((operation) => operation.op === "createStaticMeshInstance")
      ?.op === "createStaticMeshInstance"
      ? carbine.ops.find(
          (operation) => operation.op === "createStaticMeshInstance",
        )?.instance.asset
      : null,
    "mesh/prop-kit/rivet-carbine",
  );
  projection.applyFrame(carbine);
  carbine.commit();
  assert.equal(adapter.readout().weapon, "weapon/rivet-carbine");
});

test("movement bob and Rust attack cues drive only disposable local offsets", () => {
  const accepted = state({
    playerMotionState: "moved",
    presentation: {
      animationStates: [],
      cues: [
        {
          kind: "movement",
          entity: 1,
          from: [0.5, 0.5, 0.5],
          to: [0.5, 0.5, 0.4],
        },
        {
          kind: "attack",
          attacker: 1,
          weapon: "weapon/arc-pistol",
          presentation: "arc-pistol",
          attackMode: "hitscan",
          rayCount: 1,
          origin: [0.5, 1.5, 0.5],
          direction: [0, 0, -1],
        },
      ],
    },
  });
  const authorityBefore = JSON.stringify(accepted);
  const adapter = new WeaponViewmodelAdapter();
  adapter.project(state()).commit();

  const impulse = adapter.project(accepted);
  const updates = updateNodes(impulse.ops);
  const root = updates.find((node) =>
    node.metadata?.tags.includes("weapon-viewmodel"),
  );
  const flash = updates.find(
    (node) => node.metadata?.label === "loading-bay-viewmodel-muzzle-flash",
  );
  assert.notDeepEqual(root?.transform?.translation, [0, 0, 0]);
  assert.equal(flash?.visible, true);
  impulse.commit();
  assert.equal(adapter.readout().impulse, "attack");
  assert.ok(adapter.readout().bobPhase > 0);

  const settled = adapter.clearImpulse();
  assert.equal(
    updateNodes(settled.ops).find(
      (node) => node.metadata?.label === "loading-bay-viewmodel-muzzle-flash",
    )?.visible,
    false,
  );
  settled.commit();
  assert.equal(adapter.readout().impulse, "idle");
  assert.equal(JSON.stringify(accepted), authorityBefore);
});

test("flash intensity scales only the disposable muzzle descriptor", () => {
  const accepted = state({
    presentation: {
      animationStates: [],
      cues: [
        {
          kind: "attack",
          attacker: 1,
          weapon: "weapon/arc-pistol",
          presentation: "arc-pistol",
          attackMode: "hitscan",
          rayCount: 1,
          origin: [0.5, 1.5, 0.5],
          direction: [0, 0, -1],
        },
      ],
    },
  });
  const adapter = new WeaponViewmodelAdapter();
  adapter.project(state()).commit();
  const reduced = updateNodes(adapter.project(accepted, false, 0.25).ops);
  const flash = reduced.find(
    (node) => node.metadata?.label === "loading-bay-viewmodel-muzzle-flash",
  );
  const root = reduced.find(
    (node) => node.metadata?.label === "loading-bay-viewmodel-root",
  );

  assert.equal(flash?.visible, true);
  assert.deepEqual(flash?.transform?.scale, [0.1375, 0.1375, 0.1375]);
  assert.equal(flash?.material, null);
  assert.notDeepEqual(root?.transform?.translation, [0, 0, 0]);

  const disabled = new WeaponViewmodelAdapter();
  disabled.project(state()).commit();
  const noFlash = updateNodes(disabled.project(accepted, false, 0).ops).find(
    (node) => node.metadata?.label === "loading-bay-viewmodel-muzzle-flash",
  );
  assert.equal(noFlash?.visible, false);
});

test("death hides, reset clears, and disposal destroys the retained hierarchy", () => {
  const adapter = new WeaponViewmodelAdapter();
  adapter.project(state()).commit();
  const dead = adapter.project(
    state({
      player: { ...state().player, vitalityState: "dead" },
      presentation: {
        animationStates: [],
        cues: [
          {
            kind: "attack",
            attacker: 1,
            weapon: "weapon/arc-pistol",
            presentation: "arc-pistol",
            attackMode: "hitscan",
            rayCount: 1,
            origin: [0.5, 1.5, 0.5],
            direction: [0, 0, -1],
          },
        ],
      },
    }),
  );
  dead.commit();
  assert.equal(adapter.readout().visible, false);
  assert.equal(adapter.readout().impulse, "attack");

  adapter.project(state(), true).commit();
  assert.equal(adapter.readout().visible, true);
  assert.equal(adapter.readout().impulse, "idle");
  assert.equal(adapter.readout().bobPhase, 0);

  const destroyed = adapter.destroy();
  assert.deepEqual(
    destroyed.ops.map((operation) => operation.op),
    ["destroy", "destroy", "destroy"],
  );
  destroyed.commit();
  assert.equal(adapter.readout().liveNodeCount, 0);
});

test("rejected plans leave the viewmodel baseline retryable", () => {
  const adapter = new WeaponViewmodelAdapter();
  const accepted = adapter.project(state());
  const stale = adapter.project(state());
  accepted.commit();
  assert.throws(() => stale.commit(), /stale weapon viewmodel plan/);
  assert.equal(adapter.project(state()).ops.length, 0);
});

function updateNodes(
  operations: readonly RenderDiff[],
): readonly Extract<RenderDiff, { readonly op: "update" }>[] {
  return operations.filter(
    (operation): operation is Extract<RenderDiff, { readonly op: "update" }> =>
      operation.op === "update",
  );
}

function testAssetDefinitions(): readonly RenderDiff[] {
  return [
    {
      op: "defineMaterial",
      material: {
        schemaVersion: 1,
        id: "material/viewmodel-test",
        color: [0.2, 0.4, 0.7, 1],
        texture: null,
        roughness: 0.7,
        textureTint: [1, 1, 1, 1],
        emissionColor: [0, 0, 0],
        emissionIntensity: 0,
        uvStrategy: "flat",
      },
    },
    {
      op: "defineStaticMesh",
      asset: testAsset("mesh/prop-kit/arc-pistol"),
    },
    {
      op: "defineStaticMesh",
      asset: testAsset("mesh/prop-kit/breach-scattergun"),
    },
    {
      op: "defineStaticMesh",
      asset: testAsset("mesh/prop-kit/rivet-carbine"),
    },
    {
      op: "defineStaticMesh",
      asset: testAsset("mesh/prop-kit/muzzle-flash"),
    },
  ];
}

function testAsset(asset: string): StaticMeshAsset {
  return {
    asset,
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
    materialSlots: [{ slot: 0, material: "material/viewmodel-test" }],
    collision: { kind: "visualOnly" },
  };
}

function state(
  overrides: Partial<RuntimeBrowserState> = {},
): RuntimeBrowserState {
  return {
    hostSessionId: "host-a",
    projectId: "loading-bay",
    tick: 0,
    entityRevision: 0,
    voxelRevision: 0,
    voxelAuthorityHash: "0000000000000000",
    voxelSolidCount: 0,
    voxelNavigationHash: "0000000000000000",
    voxelProbePathLength: 0,
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
      damage: 60,
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
    animatedMeshes: [],
    visualBindings: [],
    generatedEnvironment: null,
    enemies: [],
    presentation: { animationStates: [], cues: [] },
    lastEvents: [],
    ...overrides,
  };
}
