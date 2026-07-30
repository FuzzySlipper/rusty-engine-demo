import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_PRESENTATION_EVENT_HISTORY,
  MAX_PRESENTATION_EVENT_KINDS,
  appendPresentationEvents,
  isHighFrequencyDiagnosticEvent,
  observePresentationEventKinds,
} from "./event-history.ts";

test("presentation event history retains only the newest bounded events", () => {
  const history: string[] = [];
  for (let index = 0; index < 10_000; index += 1) {
    appendPresentationEvents(history, [`event-${String(index)}`]);
    assert.ok(history.length <= MAX_PRESENTATION_EVENT_HISTORY);
  }

  assert.equal(history.length, MAX_PRESENTATION_EVENT_HISTORY);
  assert.equal(history[0], "event-9744");
  assert.equal(history.at(-1), "event-9999");
});

test("an oversized update replaces history with its bounded suffix", () => {
  const history = ["old-a", "old-b"];
  appendPresentationEvents(
    history,
    Array.from({ length: 400 }, (_, index) => `new-${String(index)}`),
  );

  assert.equal(history.length, MAX_PRESENTATION_EVENT_HISTORY);
  assert.equal(history[0], "new-144");
  assert.equal(history.at(-1), "new-399");
});

test("invalid presentation history capacities fail closed", () => {
  assert.throws(
    () => appendPresentationEvents([], ["event"], 0),
    /capacity must be positive/,
  );
});

test("distinct whole-run event evidence stays bounded independently of the history tail", () => {
  const observed = new Set<string>();
  assert.equal(
    observePresentationEventKinds(observed, [
      "EnemyDefeated",
      "PlayerMoved",
      "EnemyDefeated",
      "LevelCompleted",
    ]),
    true,
  );
  assert.deepEqual(
    [...observed],
    ["EnemyDefeated", "PlayerMoved", "LevelCompleted"],
  );

  const overflow = Array.from(
    { length: MAX_PRESENTATION_EVENT_KINDS },
    (_, index) => `event-${String(index)}`,
  );
  const full = new Set(overflow);
  assert.equal(observePresentationEventKinds(full, ["event-overflow"]), false);
  assert.equal(full.size, MAX_PRESENTATION_EVENT_KINDS);
  assert.equal(full.has("event-overflow"), false);
});

test("continuous movement diagnostics do not drive full shell projections", () => {
  for (const event of [
    "InputExpired",
    "NavigationAdvanced",
    "NavigationBlocked",
    "PlayerBlocked",
    "PlayerLookChanged",
    "PlayerMoved",
  ]) {
    assert.equal(isHighFrequencyDiagnosticEvent(event), true, event);
  }

  for (const event of [
    "DamageApplied",
    "EnemyPostureChanged",
    "PickupCollected",
    "LevelCompleted",
  ]) {
    assert.equal(isHighFrequencyDiagnosticEvent(event), false, event);
  }
});
