/**
 * E1M1's one typed product extension over the standard gameplay route.
 *
 * Common actor/destructible structure is selected in Rust through public
 * presets. Doom's caps stay a narrow named extension, not a generic grammar.
 */
import {
  authorStandardExtension,
  declareStandardExtensionSchema,
} from "@rusty-engine/gameplay-standard-authoring";

const doomVitality = declareStandardExtensionSchema<{
  readonly maximumHealth: number;
  readonly maximumArmor: number;
}>("loading-bay.vitality", 1);

export const gameplayPackage = authorStandardExtension({
  domain: "loading-bay",
  package: "e1m1-standard-vitality",
  version: 1,
  sources: [
    {
      id: "doom-vitality",
      path: "gameplay/authoring/src/packages/e1m1-standard-vitality.ts",
    },
  ],
  provenance: [{ subject: "doom-e1m1-vitality", source: "doom-vitality" }],
  schema: doomVitality,
  kind: "doom.vitality-policy",
  subject: "doom-e1m1-vitality",
  source: "doom-vitality",
  payload: { maximumHealth: 1_000_000, maximumArmor: 1_000_000 },
});
