import type { LevelExitProgram, SecretProgram } from "../authoring/definitions.js";

/** E1M1 secret sectors keep their source-order declaration here. Rust owns
 * their trigger geometry, once-only state, identities, and fact payloads. */
export const secretPrograms = [
  {
    id: "secret/e1m1-discovery",
    program: {
      kind: "when",
      predicate: "secretRegionEntered",
      thenProgram: {
        kind: "when",
        predicate: "secretUndiscovered",
        thenProgram: {
          kind: "sequence",
          steps: [
            { kind: "operation", operation: "recordDiscovery" },
            { kind: "operation", operation: "emitSecretPresentation" },
          ],
        },
      },
    },
  },
] as const satisfies readonly SecretProgram[];

/** E1M1's sole linedef-11 exit is one small state-plus-presentation program.
 * Rust retains actor/range/death admission and all exit state. */
export const levelExitPrograms = [
  {
    id: "level-exit/e1m1-completion",
    program: {
      kind: "when",
      predicate: "exitAvailable",
      thenProgram: {
        kind: "sequence",
        steps: [
          { kind: "operation", operation: "recordCompletion" },
          { kind: "operation", operation: "emitCompletionPresentation" },
        ],
      },
    },
  },
] as const satisfies readonly LevelExitProgram[];
