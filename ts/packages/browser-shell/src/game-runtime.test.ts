import assert from "node:assert/strict";
import test from "node:test";

import { resolvePointerLook } from "./pointer-look.ts";

test("pointer look follows first-person yaw and pitch directions", () => {
  const preferences = { mouseSensitivity: 1, invertY: false };

  assert.deepEqual(resolvePointerLook(10, -5, preferences), [-0.1, 0.05]);
  assert.deepEqual(resolvePointerLook(-10, 5, preferences), [0.1, -0.05]);
});

test("pointer look applies sensitivity and explicit Y inversion", () => {
  assert.deepEqual(
    resolvePointerLook(10, -5, {
      mouseSensitivity: 2,
      invertY: true,
    }),
    [-0.2, -0.1],
  );
});
