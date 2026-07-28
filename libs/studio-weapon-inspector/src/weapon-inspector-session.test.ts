import assert from "node:assert/strict";
import test from "node:test";

import type {
  StudioEntityInspectorContext,
  StudioEntityInspectorMutationPort,
} from "@rusty-engine/studio-editor-shell";

import {
  LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
  LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
  LoadingBayWeaponAuthoringClient,
  type LoadingBayWeaponAuthoringPort,
  type LoadingBayWeaponCandidate,
  type LoadingBayWeaponReadout,
} from "./weapon-authoring-codec.js";
import {
  loadingBayWeaponInspectorContextKey,
  LoadingBayWeaponInspectorSession,
  type LoadingBayWeaponInspectorState,
} from "./weapon-inspector-session.js";

const HASH_A = "a".repeat(64);
const HASH_B = "b".repeat(64);
const REVISION_A = "c".repeat(64);
const REVISION_B = "d".repeat(64);

test("context identity ignores host busy posture but changes for stale project or selection", () => {
  const current = context();
  const key = loadingBayWeaponInspectorContextKey(current);
  assert.equal(
    loadingBayWeaponInspectorContextKey({ ...current, busy: true }),
    key,
  );
  assert.notEqual(
    loadingBayWeaponInspectorContextKey({
      ...current,
      selectionGeneration: current.selectionGeneration + 1,
    }),
    key,
  );
  assert.notEqual(
    loadingBayWeaponInspectorContextKey({
      ...current,
      project: { ...current.project, projectHash: HASH_B },
    }),
    key,
  );
});

test("session reads, replaces through one lease, and exposes the settled Rust readout", async () => {
  const requests: Array<Record<string, unknown>> = [];
  const port = scriptedPort((request) => {
    requests.push(request);
    if (request.type === "readLoadingBayWeapon") {
      return readResponse(String(request.requestId), weapon());
    }
    return replaceResponse(
      String(request.requestId),
      weapon({
        componentRevision: REVISION_B,
        definition: { ...weapon().definition, damage: 61 },
      }),
    );
  });
  let settleCalls = 0;
  const states: LoadingBayWeaponInspectorState[] = [];
  const session = new LoadingBayWeaponInspectorSession(
    client(port),
    context(),
    mutationPort({
      settle: (receipt) => {
        settleCalls += 1;
        assert.deepEqual(receipt, {
          beforeProjectHash: HASH_A,
          afterProjectHash: HASH_B,
        });
        return Promise.resolve({ kind: "accepted", projectHash: HASH_B });
      },
    }),
    (state) => states.push(state),
  );

  session.load();
  await waitFor(() => session.state.weapon !== null);
  const candidate = {
    ...(session.state.weapon as LoadingBayWeaponReadout).definition,
    damage: 61,
  };
  session.save(candidate);
  await waitFor(() => session.state.status === "Saved and reread");

  assert.equal(requests.length, 2);
  assert.equal(requests[1]?.expectedProjectHash, HASH_A);
  assert.equal(requests[1]?.expectedComponentRevision, REVISION_A);
  assert.equal(session.state.weapon?.definition.damage, 61);
  assert.equal(settleCalls, 1);
  assert.equal(
    states.some((state) => state.saving),
    true,
  );
});

test("session releases a rejected edit and preserves the editable readout", async () => {
  const port = scriptedPort((request) =>
    request.type === "readLoadingBayWeapon"
      ? readResponse(String(request.requestId), weapon())
      : JSON.stringify({
          type: "loadingBayWeaponRejected",
          contractVersion: 1,
          requestId: request.requestId,
          rejection: {
            code: "candidateRejected",
            message: "damage exceeds the authored weapon bound",
            path: "candidate.damage",
          },
        }),
  );
  let rejected = 0;
  const session = new LoadingBayWeaponInspectorSession(
    client(port),
    context(),
    mutationPort({
      reject: () => {
        rejected += 1;
        return {
          kind: "rejected",
          message: "damage exceeds the authored weapon bound",
        };
      },
    }),
    () => undefined,
  );

  session.load();
  await waitFor(() => session.state.weapon !== null);
  session.save({
    ...(session.state.weapon as LoadingBayWeaponReadout).definition,
    damage: 999,
  });
  await waitFor(() => session.state.error !== null);

  assert.equal(rejected, 1);
  assert.equal(session.state.weapon?.definition.damage, 60);
  assert.match(session.state.error ?? "", /candidate\.damage/u);
});

test("session reports lease contention without sending a mutation", async () => {
  let requests = 0;
  const port = scriptedPort((request) => {
    requests += 1;
    return readResponse(String(request.requestId), weapon());
  });
  const session = new LoadingBayWeaponInspectorSession(
    client(port),
    context(),
    {
      acquire: () => {
        throw new Error("Entity inspector mutation is busy.");
      },
    },
    () => undefined,
  );

  session.load();
  await waitFor(() => session.state.weapon !== null);
  session.save({
    ...(session.state.weapon as LoadingBayWeaponReadout).definition,
    damage: 61,
  });

  assert.equal(requests, 1);
  assert.equal(session.state.error, "Entity inspector mutation is busy.");
  assert.equal(session.state.weapon?.definition.damage, 60);
});

test("session clears stale settlement and ignores a read completed after disposal", async () => {
  const immediate = scriptedPort((request) =>
    request.type === "readLoadingBayWeapon"
      ? readResponse(String(request.requestId), weapon())
      : replaceResponse(
          String(request.requestId),
          weapon({
            componentRevision: REVISION_B,
            definition: { ...weapon().definition, damage: 61 },
          }),
        ),
  );
  const staleSession = new LoadingBayWeaponInspectorSession(
    client(immediate),
    context(),
    mutationPort({
      settle: () => Promise.resolve({ kind: "stale" }),
    }),
    () => undefined,
  );
  staleSession.load();
  await waitFor(() => staleSession.state.weapon !== null);
  staleSession.save({
    ...(staleSession.state.weapon as LoadingBayWeaponReadout).definition,
    damage: 61,
  });
  await waitFor(() => staleSession.state.error !== null);
  assert.equal(staleSession.state.weapon, null);
  assert.match(staleSession.state.error ?? "", /project or selection changed/u);

  let resolveRead: ((response: string) => void) | null = null;
  const delayed = client({
    exchange: () =>
      new Promise((resolve) => {
        resolveRead = resolve;
      }),
  });
  const observed: LoadingBayWeaponInspectorState[] = [];
  const disposed = new LoadingBayWeaponInspectorSession(
    delayed,
    context(),
    mutationPort(),
    (state) => observed.push(state),
  );
  disposed.load();
  assert.equal(observed.length, 1);
  disposed.dispose();
  const completeRead: unknown = resolveRead;
  if (typeof completeRead !== "function") {
    throw new Error("delayed read was not started");
  }
  completeRead(readResponse("request-1", weapon()));
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(observed.length, 1);
});

function client(
  port: LoadingBayWeaponAuthoringPort,
): LoadingBayWeaponAuthoringClient {
  let request = 0;
  return new LoadingBayWeaponAuthoringClient(
    port,
    () => `request-${String(++request)}`,
  );
}

function scriptedPort(
  respond: (request: Record<string, unknown>) => string,
): LoadingBayWeaponAuthoringPort {
  return {
    exchange: (input) =>
      Promise.resolve(respond(JSON.parse(input) as Record<string, unknown>)),
  };
}

function context(): StudioEntityInspectorContext {
  return {
    ownerEntityId: 9,
    componentTypeId: LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
    inspectorContract: {
      contractId: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
      contractVersion: 1,
    },
    project: {
      projectId: "loading-bay",
      name: "Loading Bay",
      entryScene: "scene/loading-bay",
      sourceSchemaVersion: 21,
      currentSchemaVersion: 21,
      projectHash: HASH_A,
      sceneRevision: 1,
      relativeProjectFile: "content/projects/loading-bay.project.json",
    },
    projectGeneration: 1,
    selectionGeneration: 2,
    contractGeneration: 3,
    adapterId: "rusty-engine-demo.loading-bay",
    busy: false,
  };
}

function mutationPort(
  overrides: {
    readonly settle?: (
      receipt: Readonly<{
        beforeProjectHash: string;
        afterProjectHash: string;
      }>,
    ) => Promise<
      | { readonly kind: "accepted"; readonly projectHash: string }
      | { readonly kind: "rejected"; readonly message: string }
      | { readonly kind: "stale" }
    >;
    readonly reject?: (
      error?: unknown,
    ) =>
      | { readonly kind: "rejected"; readonly message: string }
      | { readonly kind: "stale" };
  } = {},
): StudioEntityInspectorMutationPort {
  return {
    acquire: (leaseContext) => ({
      context: leaseContext,
      settle:
        overrides.settle ??
        (() => Promise.resolve({ kind: "accepted", projectHash: HASH_B })),
      reject:
        overrides.reject ??
        (() => ({
          kind: "rejected",
          message: "rejected",
        })),
    }),
  };
}

function weapon(
  overrides: Partial<LoadingBayWeaponReadout> = {},
): LoadingBayWeaponReadout {
  return {
    componentTypeId: LOADING_BAY_WEAPON_COMPONENT_TYPE_ID,
    contractId: LOADING_BAY_WEAPON_AUTHORING_CONTRACT_ID,
    contractVersion: 1,
    ownerEntityId: 9,
    componentRevision: REVISION_A,
    itemDefinitionId: "weapon/arc-pistol",
    binding: {
      inventoryOwnerEntityId: 1,
      slotIndex: 0,
      startingQuantity: 1,
      initiallyEquipped: true,
    },
    definition: candidate(),
    ...overrides,
  };
}

function candidate(): LoadingBayWeaponCandidate {
  return {
    attackMode: { mode: "hitscan" },
    damage: 60,
    maxDistance: 24,
    cooldownTicks: 12,
    ammunitionItemId: "ammo/cells",
    ammunitionCost: 1,
    muzzleOffset: [0.2, -0.1, -0.4],
    presentation: "weapon/arc-pistol",
  };
}

function readResponse(
  requestId: string,
  readout: LoadingBayWeaponReadout,
): string {
  return JSON.stringify({
    type: "loadingBayWeaponRead",
    contractVersion: 1,
    requestId,
    weapon: readout,
  });
}

function replaceResponse(
  requestId: string,
  readout: LoadingBayWeaponReadout,
): string {
  return JSON.stringify({
    type: "loadingBayWeaponReplaced",
    contractVersion: 1,
    requestId,
    receipt: {
      ownerEntityId: 9,
      itemDefinitionId: "weapon/arc-pistol",
      projectHashBefore: HASH_A,
      projectHashAfter: HASH_B,
      componentRevisionBefore: REVISION_A,
      componentRevisionAfter: REVISION_B,
    },
    weapon: readout,
  });
}

async function waitFor(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 40; attempt += 1) {
    if (predicate()) return;
    await new Promise<void>((resolve) => setImmediate(resolve));
  }
  assert.fail("condition did not settle");
}
