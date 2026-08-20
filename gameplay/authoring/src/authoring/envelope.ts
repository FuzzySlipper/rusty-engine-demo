/**
 * Package envelope composition through the Engine's canonical binary64
 * authoring API. Provenance is computed here from the section → source-file
 * map the package entry supplies, so no catalog file ever hand-writes
 * provenance that rots on the next edit.
 *
 * The artifact's canonicalJson is the exact byte string the Engine
 * fingerprints; materialization writes it verbatim (plus a trailing
 * newline), keeping TypeScript output and Rust admission byte-identical.
 */

import { authorBinary64RulePackage } from "@rusty-engine/gameplay-rules-authoring";

import type { LoadingBayGameplayPayload, PackageInput } from "./definitions.js";

export const composePackage = (input: PackageInput) => {
  const { payload } = input;
  const sources = Object.entries(input.sources).map(([id, path]) => ({ id, path }));
  const provenance: { subject: string; source: string }[] = [];
  for (const entry of payload.items) {
    provenance.push({ subject: `item.${entry.id}`, source: "items" });
  }
  for (const entry of payload.gameplayPrograms) {
    provenance.push({ subject: `gameplayProgram.${entry.id}`, source: "programs" });
  }
  for (const entry of payload.pickupPrograms) {
    provenance.push({ subject: `pickupProgram.${entry.id}`, source: "programs" });
  }
  for (const entry of payload.playerSetupPrograms) {
    provenance.push({ subject: `playerSetupProgram.${entry.id}`, source: "programs" });
  }
  for (const entry of payload.enemyAttackPrograms) {
    provenance.push({ subject: `enemyAttackProgram.${entry.id}`, source: "programs" });
  }
  for (const entry of payload.enemyDefeatPrograms) {
    provenance.push({ subject: `enemyDefeatProgram.${entry.id}`, source: "programs" });
  }
  for (const entry of payload.hazardPrograms) {
    provenance.push({ subject: `hazardProgram.${entry.id}`, source: "environmentPrograms" });
  }
  for (const entry of payload.explosivePropPrograms) {
    provenance.push({ subject: `explosivePropProgram.${entry.id}`, source: "environmentPrograms" });
  }
  for (const entry of payload.encounterPrograms) {
    provenance.push({ subject: `encounterProgram.${entry.id}`, source: "encounterPrograms" });
  }
  for (const entry of payload.switchPrograms) {
    provenance.push({ subject: `switchProgram.${entry.id}`, source: "interactionPrograms" });
  }
  for (const entry of payload.floorActionPrograms) {
    provenance.push({ subject: `floorActionProgram.${entry.id}`, source: "interactionPrograms" });
  }
  for (const entry of payload.liftPrograms) {
    provenance.push({ subject: `liftProgram.${entry.id}`, source: "interactionPrograms" });
  }
  for (const entry of payload.secretPrograms) {
    provenance.push({ subject: `secretProgram.${entry.id}`, source: "progressionPrograms" });
  }
  for (const entry of payload.levelExitPrograms) {
    provenance.push({ subject: `levelExitProgram.${entry.id}`, source: "progressionPrograms" });
  }
  return authorBinary64RulePackage({
    domain: "loading-bay",
    package: input.packageId,
    version: input.version,
    dependencies: [],
    sources,
    provenance,
    payload,
  });
};
