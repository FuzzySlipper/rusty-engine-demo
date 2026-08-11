export interface PointerLookPreferences {
  readonly mouseSensitivity: number;
  readonly invertY: boolean;
}

export function resolvePointerLook(
  movementX: number,
  movementY: number,
  preferences: PointerLookPreferences,
): readonly [number, number] {
  const invert = preferences.invertY ? -1 : 1;
  return [
    clamp(-movementX * preferences.mouseSensitivity * 0.01),
    clamp(-movementY * preferences.mouseSensitivity * 0.01 * invert),
  ];
}

function clamp(value: number): number {
  return Math.max(-1, Math.min(1, value));
}
