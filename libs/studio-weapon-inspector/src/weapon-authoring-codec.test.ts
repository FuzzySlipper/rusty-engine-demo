import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  decodeLoadingBayWeaponAuthoringResponse,
  encodeReadLoadingBayWeaponRequest,
  encodeReplaceLoadingBayWeaponRequest,
  LoadingBayWeaponAuthoringClient,
  type LoadingBayWeaponCandidate,
  LoadingBayWeaponOperationRejected,
  LoadingBayWeaponProtocolError,
  MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
  MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES,
} from "./weapon-authoring-codec.ts";

const FIXTURE_ROOT = new URL(
  "../../../contracts/loading-bay-weapon-authoring-v1/",
  import.meta.url,
);

test("request encoders emit the exact frozen downstream union", () => {
  const readFixture = fixture("read-request.json");
  assert.deepEqual(
    JSON.parse(
      encodeReadLoadingBayWeaponRequest({
        requestId: readFixture.requestId as string,
        expectedProjectHash: readFixture.expectedProjectHash as string,
        ownerEntityId: readFixture.ownerEntityId as number,
      }),
    ),
    readFixture,
  );

  const replaceFixture = fixture("replace-request.json");
  assert.deepEqual(
    JSON.parse(
      encodeReplaceLoadingBayWeaponRequest({
        requestId: replaceFixture.requestId as string,
        expectedProjectHash: replaceFixture.expectedProjectHash as string,
        ownerEntityId: replaceFixture.ownerEntityId as number,
        expectedComponentRevision:
          replaceFixture.expectedComponentRevision as string,
        candidate:
          replaceFixture.candidate as unknown as LoadingBayWeaponCandidate,
      }),
    ),
    replaceFixture,
  );
});

test("decoder accepts every frozen response and preserves typed weapon meaning", () => {
  const read = decodeLoadingBayWeaponAuthoringResponse(
    fixtureText("read-response.json"),
  );
  assert.equal(read.type, "loadingBayWeaponRead");
  assert.equal(read.weapon.itemDefinitionId, "weapon/arc-pistol");
  assert.equal(read.weapon.definition.attackMode.mode, "hitscan");
  assert.equal(read.weapon.binding.inventoryOwnerEntityId, 1);

  const replaced = decodeLoadingBayWeaponAuthoringResponse(
    fixtureText("replace-response.json"),
  );
  assert.equal(replaced.type, "loadingBayWeaponReplaced");
  assert.equal(replaced.receipt.projectHashAfter, "1".repeat(64));

  const rejected = decodeLoadingBayWeaponAuthoringResponse(
    fixtureText("rejected-response.json"),
  );
  assert.equal(rejected.type, "loadingBayWeaponRejected");
  assert.equal(rejected.rejection.code, "staleComponent");
});

test("decoder fails closed on unknown fields versions identities and codes", () => {
  const base = fixture("read-response.json");
  const mutations: unknown[] = [
    { ...base, payload: {} },
    { ...base, contractVersion: 2 },
    {
      ...base,
      weapon: {
        ...(base.weapon as Record<string, unknown>),
        componentTypeId: "rusty-engine-demo.loading-bay.other",
      },
    },
    {
      type: "loadingBayWeaponRejected",
      contractVersion: 1,
      requestId: "bad-code",
      rejection: { code: "genericFailure", message: "no" },
    },
  ];

  for (const mutation of mutations) {
    assert.throws(
      () => decodeLoadingBayWeaponAuthoringResponse(JSON.stringify(mutation)),
      LoadingBayWeaponProtocolError,
    );
  }
});

test("decoder enforces its UTF-8 byte bound before parsing", () => {
  const exact = JSON.stringify({
    type: "loadingBayWeaponRejected",
    contractVersion: 1,
    requestId: "bound",
    rejection: { code: "candidateRejected", message: "" },
  });
  const emptyMessageBytes = new TextEncoder().encode(exact).byteLength;
  const bounded = exact.replace(
    '"message":""',
    `"message":"${"x".repeat(
      MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES - emptyMessageBytes,
    )}"`,
  );
  assert.equal(
    new TextEncoder().encode(bounded).byteLength,
    MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES,
  );
  assert.equal(
    decodeLoadingBayWeaponAuthoringResponse(bounded).type,
    "loadingBayWeaponRejected",
  );
  assert.throws(
    () =>
      decodeLoadingBayWeaponAuthoringResponse(
        bounded.replace('"message":"', '"message":"x'),
      ),
    LoadingBayWeaponProtocolError,
  );
});

test("replace encoder admits the exact request byte bound and rejects one over", () => {
  const replaceFixture = fixture("replace-request.json");
  const input = {
    requestId: replaceFixture.requestId as string,
    expectedProjectHash: replaceFixture.expectedProjectHash as string,
    ownerEntityId: replaceFixture.ownerEntityId as number,
    expectedComponentRevision:
      replaceFixture.expectedComponentRevision as string,
    candidate: {
      ...(replaceFixture.candidate as unknown as LoadingBayWeaponCandidate),
      presentation: "",
    },
  };
  const emptyBytes = new TextEncoder().encode(
    encodeReplaceLoadingBayWeaponRequest(input),
  ).byteLength;
  const presentation = "x".repeat(
    MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES - emptyBytes,
  );
  assert.equal(
    new TextEncoder().encode(
      encodeReplaceLoadingBayWeaponRequest({
        ...input,
        candidate: { ...input.candidate, presentation },
      }),
    ).byteLength,
    MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
  );
  assert.throws(
    () =>
      encodeReplaceLoadingBayWeaponRequest({
        ...input,
        candidate: { ...input.candidate, presentation: `${presentation}x` },
      }),
    LoadingBayWeaponProtocolError,
  );
});

test("typed client correlates read replace and rejection responses", async () => {
  const responses = [
    fixtureText("read-response.json"),
    fixtureText("replace-response.json"),
    fixtureText("rejected-response.json"),
  ];
  const requests: string[] = [];
  const client = new LoadingBayWeaponAuthoringClient(
    {
      exchange: (request) => {
        requests.push(request);
        return Promise.resolve(responses.shift() ?? "");
      },
    },
    requestIds([
      "fixture-read-arc-pistol",
      "fixture-replace-arc-pistol",
      "fixture-replace-arc-pistol",
    ]),
  );
  const signal = new AbortController().signal;
  const read = await client.read(
    {
      expectedProjectHash: "0".repeat(64),
      ownerEntityId: 88,
    },
    signal,
  );
  assert.equal(read.definition.damage, 60);
  const replaced = await client.replace(
    {
      expectedProjectHash: "0".repeat(64),
      ownerEntityId: 88,
      expectedComponentRevision: "0".repeat(64),
      candidate: read.definition,
    },
    signal,
  );
  assert.equal(replaced.receipt.projectHashAfter, "1".repeat(64));
  await assert.rejects(
    client.replace(
      {
        expectedProjectHash: "0".repeat(64),
        ownerEntityId: 88,
        expectedComponentRevision: "0".repeat(64),
        candidate: read.definition,
      },
      signal,
    ),
    LoadingBayWeaponOperationRejected,
  );
  assert.deepEqual(
    requests.map((request) => JSON.parse(request).type),
    [
      "readLoadingBayWeapon",
      "replaceLoadingBayWeapon",
      "replaceLoadingBayWeapon",
    ],
  );
});

test("typed client rejects stale correlation before exposing a response", async () => {
  const response = fixture("read-response.json");
  response.requestId = "stale-selection";
  const client = new LoadingBayWeaponAuthoringClient(
    {
      exchange: () => Promise.resolve(JSON.stringify(response)),
    },
    () => "current-selection",
  );
  await assert.rejects(
    client.read(
      {
        expectedProjectHash: "0".repeat(64),
        ownerEntityId: 88,
      },
      new AbortController().signal,
    ),
    LoadingBayWeaponProtocolError,
  );
});

function fixture(name: string): Record<string, unknown> {
  return JSON.parse(fixtureText(name)) as Record<string, unknown>;
}

function fixtureText(name: string): string {
  return readFileSync(new URL(name, FIXTURE_ROOT), "utf8");
}

function requestIds(ids: readonly string[]): () => string {
  let index = 0;
  return () => {
    const value = ids[index];
    index += 1;
    assert.notEqual(value, undefined);
    return value;
  };
}
