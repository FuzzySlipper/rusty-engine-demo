/**
 * E1M1 core gameplay package: composes the item catalog into the canonical
 * gameplay-rules envelope. Materialization walks this directory.
 */

import { composePackage } from "../authoring/mod.js";
import { items } from "../catalogs/items.js";
import { explosivePropPrograms, hazardPrograms } from "../catalogs/environment-programs.js";
import { encounterPrograms } from "../catalogs/encounter-programs.js";
import { floorActionPrograms, liftPrograms, switchPrograms } from "../catalogs/interaction-programs.js";
import { levelExitPrograms, secretPrograms } from "../catalogs/progression-programs.js";
import {
  enemyAttackPrograms,
  enemyDefeatPrograms,
  gameplayPrograms,
  playerSetupPrograms,
  pickupPrograms,
} from "../catalogs/programs.js";

export const gameplayPackage = composePackage({
  packageId: "e1m1-core",
  version: 1,
  sources: {
    items: "gameplay/authoring/src/catalogs/items.ts",
    programs: "gameplay/authoring/src/catalogs/programs.ts",
    environmentPrograms: "gameplay/authoring/src/catalogs/environment-programs.ts",
    interactionPrograms: "gameplay/authoring/src/catalogs/interaction-programs.ts",
    encounterPrograms: "gameplay/authoring/src/catalogs/encounter-programs.ts",
    progressionPrograms: "gameplay/authoring/src/catalogs/progression-programs.ts",
  },
  payload: {
    schemaVersion: 1,
    items,
    gameplayPrograms,
    pickupPrograms,
    playerSetupPrograms,
    enemyAttackPrograms,
    enemyDefeatPrograms,
    hazardPrograms,
    explosivePropPrograms,
    encounterPrograms,
    switchPrograms,
    floorActionPrograms,
    liftPrograms,
    secretPrograms,
    levelExitPrograms,
  },
});
