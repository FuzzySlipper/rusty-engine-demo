import type { RuntimePlayerBindings } from "./projection.js";

export type ResolvedPlayerAction =
  | {
      readonly kind: "look";
      readonly pitchDelta: number;
      readonly yawDelta: number;
    }
  | { readonly kind: "move"; readonly forward: number; readonly right: number };
export type ResolvedAttackAction = { readonly kind: "attack" };
export type ResolvedInputAction = ResolvedAttackAction | ResolvedPlayerAction;

export function resolveKeyboardAction(
  code: string,
  bindings: RuntimePlayerBindings,
): ResolvedInputAction | null {
  if (code === bindings.moveForward) {
    return { kind: "move", forward: 1, right: 0 };
  }
  if (code === bindings.moveBackward) {
    return { kind: "move", forward: -1, right: 0 };
  }
  if (code === bindings.moveLeft) {
    return { kind: "move", forward: 0, right: -1 };
  }
  if (code === bindings.moveRight) {
    return { kind: "move", forward: 0, right: 1 };
  }
  if (code === bindings.primaryFire) {
    return { kind: "attack" };
  }
  return null;
}

export function resolvePointerButtonAction(
  button: number,
  bindings: RuntimePlayerBindings,
): ResolvedAttackAction | null {
  return bindings.primaryFire === `Mouse${String(button)}`
    ? { kind: "attack" }
    : null;
}

export function resolvePointerAction(
  movementX: number,
  movementY: number,
  bindings: RuntimePlayerBindings,
): ResolvedPlayerAction | null {
  if (
    bindings.mouseLook !== "pointer" ||
    (movementX === 0 && movementY === 0)
  ) {
    return null;
  }
  return {
    kind: "look",
    yawDelta: clampInputUnit(-movementX / 20),
    pitchDelta: clampInputUnit(-movementY / 20),
  };
}

export function clampInputUnit(value: number): number {
  return Math.max(-1, Math.min(1, value));
}
