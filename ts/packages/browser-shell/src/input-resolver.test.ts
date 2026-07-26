import assert from "node:assert/strict";
import test from "node:test";

import {
  resolveKeyboardAction,
  resolvePointerAction,
} from "./input-resolver.ts";
import type { RuntimePlayerBindings } from "./projection.ts";

const bindings: RuntimePlayerBindings = {
  mouseLook: "pointer",
  moveBackward: "KeyS",
  moveForward: "KeyW",
  moveLeft: "KeyA",
  moveRight: "KeyD",
  primaryFire: "Mouse0",
  selectWeapon: ["Digit1", "Digit2", "Digit3"],
};

void test("authored movement bindings resolve without UI-owned movement policy", () => {
  assert.deepEqual(resolveKeyboardAction("KeyW", bindings), {
    kind: "move",
    forward: 1,
    right: 0,
  });
  assert.deepEqual(resolveKeyboardAction("Digit1", bindings), {
    kind: "selectWeaponSlot",
    slot: 0,
  });
  assert.deepEqual(resolveKeyboardAction("Digit3", bindings), {
    kind: "selectWeaponSlot",
    slot: 2,
  });
  assert.equal(resolveKeyboardAction("Digit4", bindings), null);
});

void test("pointer deltas preserve the corrected first-person look directions", () => {
  assert.deepEqual(resolvePointerAction(10, -5, bindings), {
    kind: "look",
    yawDelta: -0.5,
    pitchDelta: 0.25,
  });
  assert.deepEqual(resolvePointerAction(-10, 5, bindings), {
    kind: "look",
    yawDelta: 0.5,
    pitchDelta: -0.25,
  });
});
