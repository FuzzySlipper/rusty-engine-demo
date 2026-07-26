import assert from "node:assert/strict";
import test from "node:test";

import {
  CoalescedLookInput,
  type CoalescedLookAction,
} from "./coalesced-look.ts";

class FakeScheduler {
  nowValue = 0;
  callback: (() => void) | null = null;

  readonly now = (): number => this.nowValue;
  readonly schedule = (callback: () => void): unknown => {
    assert.equal(this.callback, null);
    this.callback = callback;
    return callback;
  };
  readonly cancel = (): void => {
    this.callback = null;
  };

  run(): void {
    const callback = this.callback;
    this.callback = null;
    callback?.();
  }
}

test("mousemove bursts retain one in-flight and one bounded pending frame", async () => {
  const scheduler = new FakeScheduler();
  const dispatched: CoalescedLookAction[] = [];
  let release!: () => void;
  const blocked = new Promise<void>((resolve) => {
    release = resolve;
  });
  const look = new CoalescedLookInput({
    scheduler,
    dispatch: async (action) => {
      dispatched.push(action);
      await blocked;
    },
  });

  for (let index = 0; index < 1_000; index += 1) {
    look.push(0.01, -0.01);
  }
  assert.equal(look.pendingFrameCount, 1);
  scheduler.run();
  await Promise.resolve();
  assert.equal(look.pendingFrameCount, 1);
  assert.deepEqual(dispatched, [{ kind: "look", yawDelta: 1, pitchDelta: -1 }]);

  for (let index = 0; index < 1_000; index += 1) {
    look.push(-0.01, 0.01);
  }
  assert.equal(look.pendingFrameCount, 2);
  look.clear();
  assert.equal(look.pendingFrameCount, 1);
  release();
  await look.settled();
  assert.equal(look.pendingFrameCount, 0);
  assert.equal(dispatched.length, 1);
});

test("small look deltas coalesce without quantization loss", async () => {
  const scheduler = new FakeScheduler();
  const dispatched: CoalescedLookAction[] = [];
  const look = new CoalescedLookInput({
    scheduler,
    dispatch: async (action) => {
      dispatched.push(action);
    },
  });

  look.push(0.001, 0.002);
  look.push(0.002, 0.003);
  scheduler.run();
  await look.settled();

  assert.deepEqual(dispatched, [
    { kind: "look", yawDelta: 0.003, pitchDelta: 0.005 },
  ]);
});
