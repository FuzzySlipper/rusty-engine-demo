import assert from "node:assert/strict";
import test from "node:test";

import { LocalLookPresentationOffset } from "./local-look-offset.ts";

test("local look presents only bounded in-flight and pending input", () => {
  const offset = new LocalLookPresentationOffset();
  offset.applyPendingDelta({ yawDelta: 1, pitchDelta: 0.75 });
  offset.applyPendingDelta({ yawDelta: 1, pitchDelta: 0.75 });
  offset.applyPendingDelta({ yawDelta: 1, pitchDelta: 0.75 });

  assert.deepEqual(offset.pendingUnits, [2, 2]);
  assert.deepEqual(offset.project(170, 80, 12), {
    yawDegrees: -166,
    pitchDegrees: 89,
  });
});

test("acknowledgement reconciles one frame and reset drops every local offset", () => {
  const offset = new LocalLookPresentationOffset();
  offset.applyPendingDelta({ yawDelta: 0.75, pitchDelta: -0.5 });
  offset.applyPendingDelta({ yawDelta: 0.25, pitchDelta: 0.25 });
  offset.settleAcceptedFrame({ yawDelta: 0.75, pitchDelta: -0.5 });

  assert.deepEqual(offset.pendingUnits, [0.25, 0.25]);
  assert.deepEqual(offset.project(12, -4, 12), {
    yawDegrees: 15,
    pitchDegrees: -1,
  });

  offset.reset();
  assert.deepEqual(offset.pendingUnits, [0, 0]);
  assert.deepEqual(offset.project(12, -4, 12), {
    yawDegrees: 12,
    pitchDegrees: -4,
  });
});
