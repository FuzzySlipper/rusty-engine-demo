import type { RuntimeFeedbackCue } from "./projection.js";

const DOOM_TICKS_PER_SECOND = 35;
const MAX_DAMAGE_COUNT = 100;
const MAX_RED_PALETTE = 7;

export interface PlayerHurtReaction {
  readonly amount: number;
  readonly fatal: boolean;
  readonly direction: "front" | "left" | "right";
  readonly healthBand: number;
  readonly intensity: number;
  readonly palette: number;
  readonly remaining: number;
  readonly sequence: string;
  readonly visibleForMilliseconds: number;
}

/**
 * Disposable Doom-style HUD state. Accepted Rust damage cues add to the
 * remaining flash count; wall-clock time only decays that presentation.
 */
export class PlayerHurtFeedback {
  private damageCount = 0;
  private lastUpdateMilliseconds: number | null = null;
  private sequence = 0;

  apply(
    cue: RuntimeFeedbackCue,
    playerEntity: number,
    tick: number,
    flashIntensity: number,
    nowMilliseconds: number,
  ): PlayerHurtReaction | null {
    if (
      cue.kind !== "damage" ||
      cue.target !== playerEntity ||
      cue.amount <= 0
    ) {
      return null;
    }

    this.decay(nowMilliseconds);
    this.damageCount = Math.min(
      MAX_DAMAGE_COUNT,
      this.damageCount + cue.amount,
    );
    this.lastUpdateMilliseconds = nowMilliseconds;
    this.sequence += 1;

    const palette = Math.min(MAX_RED_PALETTE, (this.damageCount + 7) >> 3);
    const preference = Math.max(0, Math.min(1, flashIntensity));
    return {
      amount: cue.amount,
      direction: cue.direction,
      fatal: cue.remaining === 0,
      healthBand: Math.min(
        4,
        Math.floor(((100 - Math.min(cue.remaining, 100)) * 5) / 101),
      ),
      intensity: preference * (0.18 + (palette / MAX_RED_PALETTE) * 0.58),
      palette,
      remaining: cue.remaining,
      sequence: `${tick}:${this.sequence}`,
      visibleForMilliseconds:
        (this.damageCount * 1_000) / DOOM_TICKS_PER_SECOND,
    };
  }

  reset(): void {
    this.damageCount = 0;
    this.lastUpdateMilliseconds = null;
  }

  private decay(nowMilliseconds: number): void {
    if (this.lastUpdateMilliseconds === null) return;
    const elapsedTicks = Math.floor(
      ((nowMilliseconds - this.lastUpdateMilliseconds) *
        DOOM_TICKS_PER_SECOND) /
        1_000,
    );
    this.damageCount = Math.max(0, this.damageCount - elapsedTicks);
  }
}
