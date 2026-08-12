export interface PointerLookPreferences {
  readonly mouseSensitivity: number;
  readonly invertY: boolean;
}

const MAX_POINTER_LOOK_UNITS = 64;

export function resolvePointerLook(
  movementX: number,
  movementY: number,
  preferences: PointerLookPreferences,
): readonly [number, number] {
  const invert = preferences.invertY ? -1 : 1;
  return [
    clamp(movementX * preferences.mouseSensitivity * 0.01),
    clamp(-movementY * preferences.mouseSensitivity * 0.01 * invert),
  ];
}

function clamp(value: number): number {
  return Math.max(
    -MAX_POINTER_LOOK_UNITS,
    Math.min(MAX_POINTER_LOOK_UNITS, value),
  );
}
