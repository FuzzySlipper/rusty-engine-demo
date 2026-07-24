import assert from "node:assert/strict";
import test from "node:test";

import {
  entityId,
  renderHandle,
  type RenderFrameDiff,
} from "./index.js";

test("the retained frame union carries only direct presentation operations", () => {
  const frame: RenderFrameDiff = {
    ops: [
      {
        op: "create",
        handle: renderHandle(7),
        parent: null,
        node: {
          geometry: { shape: "cube" },
          material: { color: [0.25, 0.5, 0.75, 1], wireframe: false },
          transform: {
            translation: [1, 2, 3],
            rotation: [0, 0, 0, 1],
            scale: [1, 1, 1],
          },
          visible: true,
          layer: "scene",
          metadata: { source: entityId(11), tags: [], label: "fixture" },
        },
      },
    ],
  };

  assert.equal(frame.ops[0]?.op, "create");
  assert.equal(frame.ops[0]?.handle, 7);
});
