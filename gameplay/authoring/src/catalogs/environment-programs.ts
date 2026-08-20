/**
 * Immutable E1M1 environmental programs. Rust supplies all dynamic overlap,
 * radial-query, occlusion, damage, and scheduling facts; this catalog only
 * selects source order over the closed family-local vocabularies.
 */
import {
  explosivePropOperation,
  explosivePropSequence,
  explosivePropWhen,
  hazardOperation,
  hazardSequence,
  hazardWhen,
  type ExplosivePropProgram,
  type HazardProgram,
} from "../authoring/mod.js";

const canonicalHazard = hazardWhen(
  "playerOverlapping",
  hazardWhen(
    "playerEligible",
    hazardWhen(
      "cooldownReady",
      hazardSequence(
        hazardOperation("applyHazardDamage"),
        hazardOperation("scheduleHazardCooldown"),
      ),
    ),
  ),
);

/** E1M1 nukage uses its WAD-derived numbers with this closed policy. */
export const hazardPrograms = [
  { id: "hazard/nukage", program: canonicalHazard },
] as const satisfies readonly HazardProgram[];

/** E1M1 barrels retain radial damage and chained pending-prop resolution. */
export const explosivePropPrograms = [
  {
    id: "explosive-prop/barrel",
    program: explosivePropWhen(
      "explosionPending",
      explosivePropSequence(
        explosivePropOperation("selectRadialTargets"),
        explosivePropOperation("applyScaledDamage"),
        explosivePropOperation("resolveExplosion"),
      ),
    ),
  },
] as const satisfies readonly ExplosivePropProgram[];
