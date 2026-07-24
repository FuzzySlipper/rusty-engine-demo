import assert from "node:assert/strict";
import test from "node:test";

import { SerializedActionQueue } from "./action-queue.ts";

test("a rejected action is reported without poisoning later serialized input", async () => {
  const calls: string[] = [];
  const failures: unknown[] = [];
  const queue = new SerializedActionQueue((error) => failures.push(error));
  const rejection = new Error("cooldown");

  const rejected = queue.enqueue(async () => {
    calls.push("attack");
    throw rejection;
  });
  const recovered = queue.enqueue(async () => {
    calls.push("look");
  });

  await Promise.all([rejected, recovered, queue.settled()]);
  assert.deepEqual(calls, ["attack", "look"]);
  assert.deepEqual(failures, [rejection]);
});
