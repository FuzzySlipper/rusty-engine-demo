export interface HeldMovementBindings {
  readonly moveForward: string;
  readonly moveBackward: string;
  readonly moveLeft: string;
  readonly moveRight: string;
}

export interface ResolvedMovementAction {
  readonly kind: "move";
  readonly forward: number;
  readonly right: number;
}

export interface RepeatingScheduler {
  schedule(callback: () => void, intervalMilliseconds: number): unknown;
  cancel(handle: unknown): void;
}

export interface HeldMovementInputOptions {
  readonly bindings: () => HeldMovementBindings;
  readonly intervalMilliseconds: () => number;
  readonly dispatch: (action: ResolvedMovementAction) => Promise<void>;
  readonly scheduler?: RepeatingScheduler;
}

const browserScheduler: RepeatingScheduler = {
  schedule: (callback, intervalMilliseconds) =>
    globalThis.setInterval(callback, intervalMilliseconds),
  cancel: (handle) => {
    globalThis.clearInterval(
      handle as ReturnType<typeof globalThis.setInterval>,
    );
  },
};

/**
 * Browser-owned physical key lifecycle. It emits bounded, typed movement
 * intents; it never predicts or mutates accepted gameplay state.
 */
export class HeldMovementInput {
  readonly #heldCodes = new Set<string>();
  readonly #bindings: () => HeldMovementBindings;
  readonly #intervalMilliseconds: () => number;
  readonly #dispatch: (action: ResolvedMovementAction) => Promise<void>;
  readonly #scheduler: RepeatingScheduler;
  #intervalHandle: unknown = null;
  #dispatchPending = false;
  #dispatchAgain = false;
  readonly #settledWaiters = new Set<() => void>();

  constructor(options: HeldMovementInputOptions) {
    this.#bindings = options.bindings;
    this.#intervalMilliseconds = options.intervalMilliseconds;
    this.#dispatch = options.dispatch;
    this.#scheduler = options.scheduler ?? browserScheduler;
  }

  press(code: string): boolean {
    if (!isMovementCode(code, this.#bindings())) {
      return false;
    }
    const wasEmpty = this.#heldCodes.size === 0;
    this.#heldCodes.add(code);
    void this.#emitCurrentIntent();
    if (wasEmpty) {
      this.#intervalHandle = this.#scheduler.schedule(
        () => void this.#emitCurrentIntent(),
        boundedInterval(this.#intervalMilliseconds()),
      );
    }
    return true;
  }

  release(code: string): boolean {
    if (!isMovementCode(code, this.#bindings())) {
      return false;
    }
    this.#heldCodes.delete(code);
    void this.#emitCurrentIntent();
    if (this.#heldCodes.size === 0) {
      this.#cancelInterval();
    }
    return true;
  }

  clear(dispatch = true): void {
    const hadHeldInput = this.#heldCodes.size > 0;
    this.#heldCodes.clear();
    this.#cancelInterval();
    if (dispatch && hadHeldInput) {
      void this.#emitCurrentIntent();
    }
  }

  get active(): boolean {
    return this.#heldCodes.size > 0;
  }

  get current(): ResolvedMovementAction {
    return resolveHeldMovementAction(this.#heldCodes, this.#bindings());
  }

  settled(): Promise<void> {
    if (!this.#dispatchPending) {
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      this.#settledWaiters.add(resolve);
    });
  }

  async #emitCurrentIntent(): Promise<void> {
    if (this.#dispatchPending) {
      this.#dispatchAgain = true;
      return;
    }
    this.#dispatchPending = true;
    try {
      do {
        this.#dispatchAgain = false;
        await this.#dispatch(this.current);
      } while (this.#dispatchAgain);
    } finally {
      this.#dispatchPending = false;
      for (const resolve of this.#settledWaiters) {
        resolve();
      }
      this.#settledWaiters.clear();
    }
  }

  #cancelInterval(): void {
    if (this.#intervalHandle !== null) {
      this.#scheduler.cancel(this.#intervalHandle);
      this.#intervalHandle = null;
    }
  }
}

export function resolveHeldMovementAction(
  heldCodes: ReadonlySet<string>,
  bindings: HeldMovementBindings,
): ResolvedMovementAction {
  const forward =
    Number(heldCodes.has(bindings.moveForward)) -
    Number(heldCodes.has(bindings.moveBackward));
  const right =
    Number(heldCodes.has(bindings.moveRight)) -
    Number(heldCodes.has(bindings.moveLeft));
  return { kind: "move", forward, right };
}

function isMovementCode(code: string, bindings: HeldMovementBindings): boolean {
  return (
    code === bindings.moveForward ||
    code === bindings.moveBackward ||
    code === bindings.moveLeft ||
    code === bindings.moveRight
  );
}

function boundedInterval(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    throw new RangeError("held-movement interval must be finite and positive");
  }
  return Math.max(16, Math.min(1_000, Math.round(value)));
}
