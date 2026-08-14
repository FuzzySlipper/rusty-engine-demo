import assert from "node:assert/strict";
import test from "node:test";

import type {
  RustyApplicationFrame,
  RustyApplicationUiContext,
} from "@rusty-engine/application-host";

import { claimEngineRendererRoute } from "./engine-application.ts";

test("retained gameplay diffs reach the active Engine renderer after content install", async () => {
  const applied: RustyApplicationFrame[] = [];
  let renders = 0;
  let replacements = 0;
  const application = {
    renderer: {
      applyFrame: (frame: RustyApplicationFrame) => {
        applied.push(frame);
        return { applied: true, diagnostics: [] };
      },
      clear: async () => {},
      renderOnce: () => {
        renders += 1;
      },
      replaceContent: async () => {
        replacements += 1;
        return { applied: true, diagnostics: [] };
      },
      replaceFrame: async () => ({ applied: true, diagnostics: [] }),
      setCameraPose: () => {},
    },
  } as unknown as RustyApplicationUiContext;
  const route = claimEngineRendererRoute(application);
  const camera = {
    position: [0, 0, 0],
    yawDegrees: 0,
    pitchDegrees: 0,
  } as const;
  const dynamic = {
    schemaVersion: 1,
    ops: [
      {
        op: "updateSprite",
        handle: 1,
        frame: 2,
        tint: null,
        renderOrder: null,
        visible: null,
      },
    ],
  } as unknown as RustyApplicationFrame;

  await route.publish(dynamic, camera, null, true, false);

  assert.deepEqual(applied, [dynamic]);
  assert.equal(renders, 1);
  assert.equal(replacements, 0);
});

test("unchanged complete frames are not replayed as retained diffs", async () => {
  let applications = 0;
  let replacements = 0;
  const application = {
    renderer: {
      applyFrame: () => {
        applications += 1;
        return { applied: true, diagnostics: [] };
      },
      clear: async () => {},
      renderOnce: () => {},
      replaceContent: async () => ({ applied: true, diagnostics: [] }),
      replaceFrame: async () => {
        replacements += 1;
        return { applied: true, diagnostics: [] };
      },
      setCameraPose: () => {},
    },
  } as unknown as RustyApplicationUiContext;
  const route = claimEngineRendererRoute(application);
  const camera = {
    position: [0, 0, 0],
    yawDegrees: 0,
    pitchDegrees: 0,
  } as const;
  const complete = { schemaVersion: 1, ops: [] } as unknown as RustyApplicationFrame;

  await route.publish(complete, camera, null, false, false);

  assert.equal(applications, 0);
  assert.equal(replacements, 0);
});
