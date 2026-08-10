import assert from "node:assert/strict";
import test from "node:test";

import { auditActiveGuidance } from "./audit-active-guidance.mjs";

const sha = "1234567890abcdef1234567890abcdef12345678";

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
  ["Engine revision selection", `Use Engine revision ${sha} for every build.`],
  [
    "reordered Engine revision selection",
    `For every build, use revision ${sha} of Rusty Engine.`,
  ],
  ["Engine commit lock", `The Engine facade is locked to commit ${sha}.`],
  ["reordered Engine commit lock", `Commit ${sha} locks the Engine facade.`],
  ["Engine SHA resolution", `Builds must resolve Rusty Engine to SHA ${sha}.`],
  [
    "reordered Engine SHA resolution",
    `SHA ${sha} is what builds must resolve Engine to.`,
  ],
  [
    "Engine main refresh",
    "Refresh the Engine checkout from public main before building.",
  ],
  [
    "reordered Engine main refresh",
    "Before building, refresh public main in the Engine checkout.",
  ],
  [
    "Engine checkout freshness",
    "Engine checkout freshness is required before builds.",
  ],
  [
    "fresh Engine checkout",
    "The Engine checkout must be fresh before building.",
  ],
  ["Engine tag update", "Update Rusty Engine to tag v2 before building."],
  ["Engine commit deployment", `Production ships Engine commit ${sha}.`],
  [
    "reordered Engine commit deployment",
    `Engine commit ${sha} ships in production.`,
  ],
  [
    "Engine checkout match",
    "The Engine checkout must match public main before building.",
  ],
  [
    "reordered Engine checkout match",
    "Before building, public main must match the Engine checkout.",
  ],
  [
    "unrelated negative clause before Engine revision",
    `Do not change the UI; use Engine revision ${sha} for every build.`,
  ],
  [
    "unrelated no clause before Engine revision",
    `No renderer APIs are added, but production uses Engine revision ${sha}.`,
  ],
  [
    "Engine revision before unrelated negative clause",
    `Production uses Engine revision ${sha}; do not change the UI.`,
  ],
  [
    "Engine revision before unrelated no clause",
    `Production uses Engine revision ${sha}, but no renderer APIs are added.`,
  ],
  [
    "unrelated negative and Engine revision",
    `Do not change the UI and use Engine revision ${sha} for every build.`,
  ],
  [
    "Engine revision and unrelated negative",
    `Use Engine revision ${sha} for every build and do not change the UI.`,
  ],
  [
    "unrelated no and Engine commit",
    `No renderer APIs are added and production uses Engine commit ${sha}.`,
  ],
  [
    "Engine commit and unrelated no",
    `Production uses Engine commit ${sha} and no renderer APIs are added.`,
  ],
  [
    "historical and current Engine tag",
    "Historical evidence used an old renderer and production uses Engine tag v2.",
  ],
  [
    "current Engine tag and historical",
    "Production uses Engine tag v2 and historical evidence used an old renderer.",
  ],
  [
    "unrelated negative or Engine revision",
    `Do not change the UI or use Engine revision ${sha} for every build.`,
  ],
  [
    "Engine revision or unrelated negative",
    `Use Engine revision ${sha} for every build or do not change the UI.`,
  ],
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

test("active guidance permits explicit anti-ceremony rules", () => {
  for (const statement of [
    "Do not pin Engine to a revision or SHA.",
    "Do not bind Engine to a revision and SHA.",
    "The Engine checkout is not locked to a commit.",
    "Use the adjacent Engine facade without revision, freshness, or update machinery.",
    "Rusty Engine changes are fixed forward; this repo does not synchronize public main.",
  ]) {
    assert.deepEqual(
      auditActiveGuidance("docs/design.md", statement),
      [],
      statement,
    );
  }
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
  assert.deepEqual(
    auditActiveGuidance(
      "docs/weapon-authoring-contract.md",
      `The historical design was approved at Rusty Engine revision ${sha}.`,
    ),
    [],
  );
  assert.deepEqual(
    auditActiveGuidance(
      "docs/weapon-authoring-contract.md",
      `Historical Engine revision and commit provenance used ${sha}.`,
    ),
    [],
  );
});
