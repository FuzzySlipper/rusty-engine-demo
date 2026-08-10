import assert from "node:assert/strict";
import test from "node:test";

import { auditActiveGuidance } from "./audit-active-guidance.mjs";

for (const [name, statement] of [
  ["pinned provider", "Use the pinned Engine gameplay provider."],
  ["reverse pinned provider", "The Rusty Engine renderer is pinned here."],
  [
    "exact revision",
    "Consume the exact-revision shared Rusty Engine renderer.",
  ],
  ["exact pinned", "Use an exact-pinned Engine spatial provider."],
  ["floating revision", "Floating Engine revisions are forbidden."],
  [
    "sibling path prohibition",
    "Sibling paths are wrong for normal development.",
  ],
  ["reverse sibling path prohibition", "Do not use a sibling path for Engine."],
]) {
  test(`active guidance rejects ${name}`, () => {
    assert.notDeepEqual(
      auditActiveGuidance("docs/extension-recipes.md", statement),
      [],
    );
  });
}

test("active guidance permits the adjacent sibling facade", () => {
  assert.deepEqual(
    auditActiveGuidance(
      "docs/design.md",
      "Use one Cargo path to the complete adjacent sibling Engine facade.",
    ),
    [],
  );
});

test("historical provenance may retain exact revision language", () => {
  assert.deepEqual(
    auditActiveGuidance(
      "docs/source-provenance.md",
      "Historical evidence used an exact-pinned Rusty Engine revision.",
    ),
    [],
  );
  assert.deepEqual(
    auditActiveGuidance(
      "docs/visual-content-pipeline.md",
      "This frozen predecessor baseline used pinned Engine packages.",
    ),
    [],
  );
});
