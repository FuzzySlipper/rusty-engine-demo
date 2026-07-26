export interface LocalLookDelta {
  readonly yawDelta: number;
  readonly pitchDelta: number;
}

export interface LocalLookProjection {
  readonly yawDegrees: number;
  readonly pitchDegrees: number;
}

/**
 * Disposable presentation-only look offset. At most one in-flight and one
 * coalesced pending input frame can contribute, so neither axis exceeds two
 * normalized input units.
 */
export class LocalLookPresentationOffset {
  #yawUnits = 0;
  #pitchUnits = 0;

  applyPendingDelta(delta: LocalLookDelta): void {
    this.#yawUnits = clampOffset(this.#yawUnits + finite(delta.yawDelta));
    this.#pitchUnits = clampOffset(this.#pitchUnits + finite(delta.pitchDelta));
  }

  settleAcceptedFrame(delta: LocalLookDelta): void {
    this.#yawUnits = clampOffset(this.#yawUnits - finite(delta.yawDelta));
    this.#pitchUnits = clampOffset(this.#pitchUnits - finite(delta.pitchDelta));
  }

  reset(): void {
    this.#yawUnits = 0;
    this.#pitchUnits = 0;
  }

  project(
    yawDegrees: number,
    pitchDegrees: number,
    lookDegreesPerUnit: number,
  ): LocalLookProjection {
    return {
      yawDegrees: normalizeDegrees(
        yawDegrees + this.#yawUnits * lookDegreesPerUnit,
      ),
      pitchDegrees: Math.max(
        -89,
        Math.min(89, pitchDegrees + this.#pitchUnits * lookDegreesPerUnit),
      ),
    };
  }

  get pendingUnits(): readonly [number, number] {
    return [this.#yawUnits, this.#pitchUnits];
  }
}

function finite(value: number): number {
  return Number.isFinite(value) ? value : 0;
}

function clampOffset(value: number): number {
  return Math.max(-2, Math.min(2, value));
}

function normalizeDegrees(value: number): number {
  return ((((value + 180) % 360) + 360) % 360) - 180;
}
