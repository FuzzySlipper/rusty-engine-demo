export type ActionFailureReporter = (error: unknown) => void;

/**
 * Serializes authoritative input requests without letting an expected rejected
 * action prevent later controls from reaching the host.
 */
export class SerializedActionQueue {
  readonly #reportFailure: ActionFailureReporter;
  #tail: Promise<void> = Promise.resolve();

  constructor(reportFailure: ActionFailureReporter) {
    this.#reportFailure = reportFailure;
  }

  enqueue(action: () => Promise<void>): Promise<void> {
    const attempted = this.#tail.then(action);
    this.#tail = attempted.catch((error: unknown) => {
      try {
        this.#reportFailure(error);
      } catch (reportingError: unknown) {
        console.error("could not report rejected input action", reportingError);
      }
    });
    return this.#tail;
  }

  settled(): Promise<void> {
    return this.#tail;
  }
}
