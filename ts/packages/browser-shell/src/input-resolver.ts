import type { RuntimePlayerBindings } from "./projection.js";

export type ResolvedPlayerAction =
  | { readonly kind: "move"; readonly forward: number; readonly right: number }
  | { readonly kind: "jump" };
export type ResolvedAttackAction = { readonly kind: "attack" };
export type ResolvedWeaponSelectionAction = {
  readonly kind: "selectWeaponSlot";
  readonly slot: number;
};
export type ResolvedInputAction =
  | ResolvedAttackAction
  | ResolvedPlayerAction
  | ResolvedWeaponSelectionAction;

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
  if (code === bindings.jump) {
    return { kind: "jump" };
  }
  const weaponSlot = bindings.selectWeapon.indexOf(code);
  if (weaponSlot >= 0) {
    return { kind: "selectWeaponSlot", slot: weaponSlot };
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
