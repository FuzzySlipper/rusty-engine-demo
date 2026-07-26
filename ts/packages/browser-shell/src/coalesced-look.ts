export interface CoalescedLookAction {
  readonly kind: "look";
  readonly yawDelta: number;
  readonly pitchDelta: number;
}

interface LookScheduler {
  readonly now: () => number;
  readonly schedule: (
    callback: () => void,
    delayMilliseconds: number,
  ) => unknown;
  readonly cancel: (handle: unknown) => void;
}

export interface CoalescedLookOptions {
  readonly dispatch: (action: CoalescedLookAction) => Promise<void>;
  readonly intervalMilliseconds?: number;
  readonly scheduler?: LookScheduler;
}

const browserScheduler: LookScheduler = {
  now: () => performance.now(),
  schedule: (callback, delayMilliseconds) =>
    globalThis.setTimeout(callback, delayMilliseconds),
  cancel: (handle) =>
    globalThis.clearTimeout(handle as ReturnType<typeof globalThis.setTimeout>),
};

/**
 * One in-flight look frame plus one bounded accumulated frame. Device event
 * frequency can never create a promise tail.
 */
export class CoalescedLookInput {
  readonly #dispatch: (action: CoalescedLookAction) => Promise<void>;
  readonly #intervalMilliseconds: number;
  readonly #scheduler: LookScheduler;
  #pendingYaw = 0;
  #pendingPitch = 0;
  #inFlight = false;
  #timer: unknown = null;
  #lastSentAt = Number.NEGATIVE_INFINITY;
  #generation = 0;
  #disposed = false;
  #settlement: Promise<void> | null = null;
  #resolveSettlement: (() => void) | null = null;

  constructor(options: CoalescedLookOptions) {
    this.#dispatch = options.dispatch;
    this.#intervalMilliseconds = Math.max(
      1,
      options.intervalMilliseconds ?? 1_000 / 60,
    );
    this.#scheduler = options.scheduler ?? browserScheduler;
  }

  push(yawDelta: number, pitchDelta: number): void {
    if (this.#disposed) {
      return;
    }
    this.#pendingYaw = clampLook(this.#pendingYaw + finiteUnit(yawDelta));
    this.#pendingPitch = clampLook(this.#pendingPitch + finiteUnit(pitchDelta));
    if (this.#pendingYaw === 0 && this.#pendingPitch === 0) {
      return;
    }
    this.#ensureSettlement();
    this.#schedule();
  }

  clear(): void {
    this.#generation += 1;
    this.#pendingYaw = 0;
    this.#pendingPitch = 0;
    if (this.#timer !== null) {
      this.#scheduler.cancel(this.#timer);
      this.#timer = null;
    }
    this.#finishIfIdle();
  }

  dispose(): void {
    this.#disposed = true;
    this.clear();
  }

  settled(): Promise<void> {
    return this.#settlement ?? Promise.resolve();
  }

  get pendingFrameCount(): number {
    return (
      Number(this.#inFlight) +
      Number(this.#pendingYaw !== 0 || this.#pendingPitch !== 0)
    );
  }

  #schedule(): void {
    if (
      this.#timer !== null ||
      this.#inFlight ||
      this.#disposed ||
      (this.#pendingYaw === 0 && this.#pendingPitch === 0)
    ) {
      return;
    }
    const elapsed = this.#scheduler.now() - this.#lastSentAt;
    const delay = Math.max(0, this.#intervalMilliseconds - elapsed);
    this.#timer = this.#scheduler.schedule(() => {
      this.#timer = null;
      void this.#flush();
    }, delay);
  }

  async #flush(): Promise<void> {
    if (
      this.#disposed ||
      this.#inFlight ||
      (this.#pendingYaw === 0 && this.#pendingPitch === 0)
    ) {
      this.#finishIfIdle();
      return;
    }
    const generation = this.#generation;
    const action: CoalescedLookAction = {
      kind: "look",
      yawDelta: this.#pendingYaw,
      pitchDelta: this.#pendingPitch,
    };
    this.#pendingYaw = 0;
    this.#pendingPitch = 0;
    this.#inFlight = true;
    this.#lastSentAt = this.#scheduler.now();
    try {
      await this.#dispatch(action);
    } finally {
      this.#inFlight = false;
      if (generation === this.#generation) {
        this.#schedule();
      }
      this.#finishIfIdle();
    }
  }

  #ensureSettlement(): void {
    if (this.#settlement !== null) {
      return;
    }
    this.#settlement = new Promise<void>((resolve) => {
      this.#resolveSettlement = resolve;
    });
  }

  #finishIfIdle(): void {
    if (
      this.#inFlight ||
      this.#timer !== null ||
      this.#pendingYaw !== 0 ||
      this.#pendingPitch !== 0
    ) {
      return;
    }
    this.#resolveSettlement?.();
    this.#resolveSettlement = null;
    this.#settlement = null;
  }
}

function finiteUnit(value: number): number {
  return Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0;
}

function clampLook(value: number): number {
  return Math.max(-1, Math.min(1, value));
}
