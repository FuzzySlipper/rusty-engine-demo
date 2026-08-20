import assert from "node:assert/strict";
import test from "node:test";

import {
  E1M1_PROP_SOURCE_ROOT,
  E1M1_REQUIRED_PROP_ASSET_IDS,
  readE1M1PropAssets,
} from "./e1m1-props.js";

test("E1M1 props form a complete donor-independent closure", () => {
  const assets = readE1M1PropAssets();
  const ids = new Set(assets.map((asset) => asset.id));
  for (const id of E1M1_REQUIRED_PROP_ASSET_IDS) {
    assert.ok(ids.has(id), `missing ${id}`);
    const asset = assets.find((candidate) => candidate.id === id);
    assert.match(
      asset.import.source.path,
      new RegExp(`^${E1M1_PROP_SOURCE_ROOT}/`),
    );
    assert.doesNotMatch(asset.import.source.path, /loading-bay/);
  }
  assert.doesNotMatch(JSON.stringify(assets), /content\/projects\//);
});
