import assert from "node:assert/strict";
import test from "node:test";

import {
  HttpLoadingBayWeaponAuthoringPort,
  LoadingBayWeaponTransportError,
  MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES,
  MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES,
} from "./weapon-authoring-codec.js";

test("HTTP port posts one abortable bounded downstream request", async () => {
  const calls: Array<{ input: string; init: RequestInit }> = [];
  const port = new HttpLoadingBayWeaponAuthoringPort(
    "/api/studio-adapter",
    (input, init) => {
      calls.push({ input, init });
      return Promise.resolve(response('{"type":"ok"}'));
    },
  );
  const controller = new AbortController();
  assert.equal(
    await port.exchange('{"request":true}', controller.signal),
    '{"type":"ok"}',
  );
  assert.equal(calls.length, 1);
  assert.equal(calls[0]?.input, "/api/studio-adapter");
  assert.equal(calls[0]?.init.method, "POST");
  assert.equal(calls[0]?.init.body, '{"request":true}');
  assert.equal(calls[0]?.init.signal, controller.signal);
});

test("HTTP port rejects request and response one-over bounds before exposure", async () => {
  let calls = 0;
  const port = new HttpLoadingBayWeaponAuthoringPort("/weapon", () => {
    calls += 1;
    return Promise.resolve(
      response("x".repeat(MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES + 1)),
    );
  });
  await assert.rejects(
    port.exchange(
      "x".repeat(MAX_LOADING_BAY_WEAPON_AUTHORING_REQUEST_BYTES + 1),
      new AbortController().signal,
    ),
    LoadingBayWeaponTransportError,
  );
  assert.equal(calls, 0);
  await assert.rejects(
    port.exchange("{}", new AbortController().signal),
    LoadingBayWeaponTransportError,
  );
  assert.equal(calls, 1);
});

test("HTTP port rejects declared oversize and preserves typed host messages", async () => {
  const oversized = new HttpLoadingBayWeaponAuthoringPort("/weapon", () =>
    Promise.resolve(
      response("", {
        declaredLength: MAX_LOADING_BAY_WEAPON_AUTHORING_RESPONSE_BYTES + 1,
      }),
    ),
  );
  await assert.rejects(
    oversized.exchange("{}", new AbortController().signal),
    LoadingBayWeaponTransportError,
  );

  const rejected = new HttpLoadingBayWeaponAuthoringPort("/weapon", () =>
    Promise.resolve(
      response('{"message":"adapter unavailable"}', { ok: false, status: 503 }),
    ),
  );
  await assert.rejects(
    rejected.exchange("{}", new AbortController().signal),
    (error: unknown) =>
      error instanceof LoadingBayWeaponTransportError &&
      error.message === "adapter unavailable",
  );
});

function response(
  body: string,
  options: {
    readonly declaredLength?: number;
    readonly ok?: boolean;
    readonly status?: number;
  } = {},
): Pick<Response, "headers" | "ok" | "status" | "text"> {
  const headers = new Headers();
  if (options.declaredLength !== undefined) {
    headers.set("content-length", String(options.declaredLength));
  }
  return {
    headers,
    ok: options.ok ?? true,
    status: options.status ?? 200,
    text: () => Promise.resolve(body),
  };
}
