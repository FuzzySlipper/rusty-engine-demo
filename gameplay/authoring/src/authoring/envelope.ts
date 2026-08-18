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
