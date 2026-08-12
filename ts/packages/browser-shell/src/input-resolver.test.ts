import assert from "node:assert/strict";
import test from "node:test";

import {
  resolveKeyboardAction,
  resolvePointerButtonAction,
} from "./input-resolver.ts";
import type { RuntimePlayerBindings } from "./projection.ts";

const bindings: RuntimePlayerBindings = {
  mouseLook: "pointer",
  moveBackward: "KeyS",
  moveForward: "KeyW",
  moveLeft: "KeyA",
  moveRight: "KeyD",
  primaryFire: "Mouse0",
  jump: "Space",
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

test("keyboard resolver emits the authored semantic jump action", () => {
  assert.deepEqual(resolveKeyboardAction("Space", bindings), { kind: "jump" });
});

test("pointer fire follows the authored button binding", () => {
  assert.deepEqual(resolvePointerButtonAction(0, bindings), { kind: "attack" });
  assert.equal(resolvePointerButtonAction(2, bindings), null);
});
