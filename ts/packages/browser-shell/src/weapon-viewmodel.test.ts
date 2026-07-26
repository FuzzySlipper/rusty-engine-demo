import assert from "node:assert/strict";
import test from "node:test";

import type { RenderDiff } from "@rusty-engine/render-contracts";
import { createRendererSurfaceProjection } from "@rusty-engine/renderer-host";

import type { RuntimeBrowserState } from "./projection.ts";
import { WeaponViewmodelAdapter } from "./weapon-viewmodel.ts";

test("the authoritative equipped weapon creates one bounded viewmodel hierarchy", () => {
  const adapter = new WeaponViewmodelAdapter();
  const plan = adapter.project(state());
  const creates = plan.ops.filter(
    (operation): operation is Extract<RenderDiff, { readonly op: "create" }> =>
      operation.op === "create",
  );

  assert.equal(creates.length, 7);
  assert.equal(creates[0]?.parent, null);
  assert.equal(creates[0]?.node.geometry.kind, "group");
  assert.ok(creates.every((operation) => operation.node.layer === "viewmodel"));
  assert.ok(
    creates
      .flatMap((operation) => operation.node.metadata.tags)
      .includes("weapon/arc-pistol"),
  );
  plan.commit();
  assert.deepEqual(adapter.readout(), {
    bobPhase: 0,
    impulse: "idle",
    liveNodeCount: 7,
    mounted: true,
    visible: true,
    weapon: "weapon/arc-pistol",
  });
});

test("the exact shared surface projection accepts the retained viewmodel layer", () => {
  const plan = new WeaponViewmodelAdapter().project(state());
  const projection = createRendererSurfaceProjection(plan);
  assert.equal(projection.snapshot.nodes.length, 7);
  assert.ok(
    projection.snapshot.nodes.every((node) => node.layer === "viewmodel"),
  );
  assert.equal(
    projection.snapshot.nodes.find(
      (node) => node.metadata.label === "loading-bay-viewmodel-root",
    )?.parent,
    null,
  );
});

test("three original weapons retain materially distinct descriptor silhouettes", () => {
  const adapter = new WeaponViewmodelAdapter();
  const baseline = state();
  if (baseline.inventory === null) {
    throw new Error("weapon viewmodel fixture requires authored inventory");
  }
  adapter.project(baseline).commit();

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
  const scatterUpdates = updateNodes(scatter.ops);
  assert.ok(
    scatterUpdates.some(
      (node) =>
        node.metadata?.tags.includes("weapon/breach-scattergun") === true &&
        node.transform?.scale[2] === 0.57,
    ),
  );
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
  const carbineUpdates = updateNodes(carbine.ops);
  assert.ok(
    carbineUpdates.some(
      (node) =>
        node.metadata?.tags.includes("weapon/rivet-carbine") === true &&
        node.transform?.scale[1] === 0.31,
    ),
  );
  assert.notDeepEqual(
    scatterUpdates.map((node) => node.transform),
    carbineUpdates.map((node) => node.transform),
  );
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
    [
      "destroy",
      "destroy",
      "destroy",
      "destroy",
      "destroy",
      "destroy",
      "destroy",
    ],
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

function state(
  overrides: Partial<RuntimeBrowserState> = {},
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
    voxelMeshes: [],
    generatedEnvironment: null,
    enemies: [],
    presentation: { animationStates: [], cues: [] },
    lastEvents: [],
    ...overrides,
  };
}
