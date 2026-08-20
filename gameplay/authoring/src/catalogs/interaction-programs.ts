/** Immutable E1M1 switch interaction composition. Rust supplies the actor,
 * target door references, availability, motion, scheduling, and events. */
import {
  floorActionOperation,
  floorActionSequence,
  floorActionWhen,
  liftOperation,
  liftSequence,
  liftWhen,
  switchOperation,
  switchSequence,
  switchWhen,
  type SwitchProgram,
  type FloorActionProgram,
  type LiftProgram,
} from "../authoring/mod.js";

const canonicalDoorSwitch = switchWhen(
  "switchAvailable",
  switchSequence(
    switchOperation("recordActivation"),
    // Preserve the established feedback-before-door-transition event order.
    switchOperation("emitInteractionFeedback"),
    switchOperation("requestOpenBoundDoor"),
  ),
);

/** E1M1 manual doors select their own explicit bound-door effects in Rust. */
export const switchPrograms = [
  { id: "switch/e1m1-door", program: canonicalDoorSwitch },
] as const satisfies readonly SwitchProgram[];

/** E1M1 linedef 308 is a one-way floor lowering action. Rust owns the
 * WAD-derived target and 59-tick interpolation; this selects its sequencing. */
export const floorActionPrograms = [
  {
    id: "floor-action/e1m1-lower",
    program: floorActionSequence(
      floorActionWhen(
        "activationEntered",
        floorActionSequence(
          floorActionOperation("recordActivation"),
          floorActionOperation("emitFloorFeedback"),
          floorActionOperation("requestLowerBoundPlatform"),
        ),
      ),
      floorActionWhen("loweringMotionTick", floorActionOperation("advanceLowering")),
    ),
  },
] as const satisfies readonly FloorActionProgram[];

/** E1M1 linedef 195 retains its lower, wait, and return cycle. Timing and
 * translations remain on the admitted Rust component. */
export const liftPrograms = [
  {
    id: "lift/e1m1-cycle",
    program: liftSequence(
      liftWhen(
        "activationEntered",
        liftSequence(
          liftOperation("recordActivation"),
          liftOperation("emitLiftFeedback"),
          liftOperation("requestLowerBoundPlatform"),
        ),
      ),
      liftWhen("loweringMotionTick", liftOperation("advanceLowering")),
      liftWhen("waitingTick", liftOperation("advanceWait")),
      liftWhen("raisingMotionTick", liftOperation("advanceRaising")),
    ),
  },
] as const satisfies readonly LiftProgram[];
