/**
 * E1M1 core gameplay package: composes the item catalog into the canonical
 * gameplay-rules envelope. Materialization walks this directory.
 */

import { composePackage } from "../authoring/mod.js";
import { items } from "../catalogs/items.js";

export const gameplayPackage = composePackage({
  packageId: "e1m1-core",
  version: 1,
  sources: {
    items: "gameplay/authoring/src/catalogs/items.ts",
  },
  payload: {
    schemaVersion: 1,
    items,
  },
});
