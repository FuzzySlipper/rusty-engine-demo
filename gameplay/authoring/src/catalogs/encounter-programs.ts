import type { EncounterProgram } from "../authoring/definitions.js";

/**
 * E1M1 encounter lifecycle source order. Rust supplies the candidate spatial
 * fact, member lifecycle, readiness cadence, exit relation, door scheduling,
 * and every mutation; this catalog only selects the closed sequencing.
 */
export const encounterPrograms = [
  {
    id: "encounter/e1m1",
    activation: {
      kind: "when",
      predicate: "activationEligible",
      thenProgram: {
        kind: "sequence",
        steps: [
          { kind: "operation", operation: "recordEncounterActivation" },
          { kind: "operation", operation: "activateBoundMembers" },
          { kind: "operation", operation: "emitEncounterFeedback" },
        ],
      },
    },
    clear: {
      kind: "when",
      predicate: "membersDefeated",
      thenProgram: {
        kind: "sequence",
        steps: [
          { kind: "operation", operation: "recordEncounterCleared" },
          { kind: "operation", operation: "openBoundExit" },
        ],
      },
    },
  },
] as const satisfies readonly EncounterProgram[];
