import assert from "node:assert/strict";
import test from "node:test";

import { PlayerHurtFeedback } from "./player-hurt-feedback.ts";

const damage = (amount: number, remaining: number) => ({
  kind: "damage" as const,
  attacker: 9,
  target: 1,
  amount,
  remaining,
  direction: "front" as const,
});

test("maps accepted health damage to Doom palette, duration, and health band", () => {
  const feedback = new PlayerHurtFeedback();
  const reaction = feedback.apply(damage(10, 90), 1, 42, 1, 1_000);

  assert.deepEqual(reaction, {
    amount: 10,
    direction: "front",
    fatal: false,
    healthBand: 0,
    intensity: 0.3457142857142857,
    palette: 2,
    remaining: 90,
    sequence: "42:1",
    visibleForMilliseconds: 285.7142857142857,
  });
});

test("repeated hits add to the undecayed count, cap at 100, and retrigger", () => {
  const feedback = new PlayerHurtFeedback();
  feedback.apply(damage(60, 40), 1, 7, 1, 0);
  const accumulated = feedback.apply(damage(60, 0), 1, 8, 1, 500);

  assert.equal(accumulated?.palette, 7);
  assert.equal(accumulated?.visibleForMilliseconds, 100_000 / 35);
  assert.equal(accumulated?.fatal, true);
  assert.equal(accumulated?.healthBand, 4);
  assert.equal(accumulated?.sequence, "8:2");
});

test("ignores non-player and zero-health-damage cues", () => {
  const feedback = new PlayerHurtFeedback();

  assert.equal(feedback.apply(damage(10, 90), 2, 1, 1, 0), null);
  assert.equal(feedback.apply(damage(0, 90), 1, 1, 1, 0), null);
});
