import assert from "node:assert/strict";
import test from "node:test";

import {
  HeldMovementInput,
  resolveHeldMovementAction,
  type RepeatingScheduler,
  type ResolvedMovementAction,
} from "./held-movement.ts";

const bindings = {
  moveForward: "KeyW",
  moveBackward: "KeyS",
  moveLeft: "KeyA",
  moveRight: "KeyD",
};

class ManualScheduler implements RepeatingScheduler {
  callback: (() => void) | null = null;
  intervalMilliseconds: number | null = null;
  cancelCount = 0;

  schedule(callback: () => void, intervalMilliseconds: number): unknown {
    this.callback = callback;
    this.intervalMilliseconds = intervalMilliseconds;
    return 1;
  }

  cancel(): void {
    this.callback = null;
    this.cancelCount += 1;
  }

  tick(): void {
    this.callback?.();
  }
}

test("held movement emits at a bounded cadence until keyup without key repeat", async () => {
  const scheduler = new ManualScheduler();
  const actions: ResolvedMovementAction[] = [];
  const input = new HeldMovementInput({
    bindings: () => bindings,
    intervalMilliseconds: () => 100,
    dispatch: async (action) => {
      actions.push(action);
    },
    scheduler,
  });

  assert.equal(input.press("KeyW"), true);
  assert.equal(input.active, true);
  assert.equal(scheduler.intervalMilliseconds, 100);
  assert.deepEqual(actions, [{ kind: "move", forward: 1, right: 0 }]);

  await Promise.resolve();
  scheduler.tick();
  await Promise.resolve();
  scheduler.tick();
  await Promise.resolve();
  assert.equal(actions.length, 3);

  assert.equal(input.release("KeyW"), true);
  assert.equal(input.active, false);
  assert.equal(scheduler.cancelCount, 1);
  scheduler.tick();
  assert.equal(actions.length, 3);
});

test("held movement combines physical keys and ignores unrelated input", async () => {
  const scheduler = new ManualScheduler();
  const actions: ResolvedMovementAction[] = [];
  const input = new HeldMovementInput({
    bindings: () => bindings,
    intervalMilliseconds: () => 5,
    dispatch: async (action) => {
      actions.push(action);
    },
    scheduler,
  });

  assert.equal(input.press("Space"), false);
  assert.equal(input.press("KeyW"), true);
  assert.equal(input.press("KeyD"), true);
  await Promise.resolve();
  scheduler.tick();
  await Promise.resolve();
  assert.equal(scheduler.intervalMilliseconds, 16);
  assert.deepEqual(actions.at(-1), { kind: "move", forward: 1, right: 1 });

  input.release("KeyW");
  scheduler.tick();
  await Promise.resolve();
  assert.deepEqual(actions.at(-1), { kind: "move", forward: 0, right: 1 });
  input.clear();
});

test("opposing held keys resolve to no movement intent", () => {
  assert.equal(
    resolveHeldMovementAction(new Set(["KeyW", "KeyS"]), bindings),
    null,
  );
});
