import type {
  StudioEntityInspectorContext,
  StudioEntityInspectorMutationLease,
  StudioEntityInspectorMutationPort,
} from "@rusty-engine/studio-editor-shell";

import {
  LoadingBayWeaponOperationRejected,
  type LoadingBayWeaponAuthoringClient,
  type LoadingBayWeaponCandidate,
  type LoadingBayWeaponReadout,
} from "./weapon-authoring-codec.js";

export interface LoadingBayWeaponInspectorState {
  readonly loading: boolean;
  readonly saving: boolean;
  readonly weapon: LoadingBayWeaponReadout | null;
  readonly error: string | null;
  readonly status: string | null;
}

export function loadingBayWeaponInspectorContextKey(
  context: StudioEntityInspectorContext,
): string {
  return [
    String(context.ownerEntityId),
    context.componentTypeId,
    context.inspectorContract.contractId,
    String(context.inspectorContract.contractVersion),
    context.project.projectId,
    context.project.projectHash,
    String(context.projectGeneration),
    String(context.selectionGeneration),
    String(context.contractGeneration),
    context.adapterId,
  ].join("\u0000");
}

export class LoadingBayWeaponInspectorSession {
  readonly #client: LoadingBayWeaponAuthoringClient;
  readonly #context: StudioEntityInspectorContext;
  readonly #mutationPort: StudioEntityInspectorMutationPort;
  readonly #listener: (state: LoadingBayWeaponInspectorState) => void;
  #state: LoadingBayWeaponInspectorState = {
    loading: false,
    saving: false,
    weapon: null,
    error: null,
    status: null,
  };
  #generation = 0;
  #controller: AbortController | null = null;
  #lease: StudioEntityInspectorMutationLease | null = null;
  #leaseOpen = false;
  #disposed = false;

  constructor(
    client: LoadingBayWeaponAuthoringClient,
    context: StudioEntityInspectorContext,
    mutationPort: StudioEntityInspectorMutationPort,
    listener: (state: LoadingBayWeaponInspectorState) => void,
  ) {
    this.#client = client;
    this.#context = context;
    this.#mutationPort = mutationPort;
    this.#listener = listener;
  }

  get state(): LoadingBayWeaponInspectorState {
    return this.#state;
  }

  load(): void {
    if (this.#disposed || this.#state.saving) return;
    this.#controller?.abort();
    const generation = ++this.#generation;
    const controller = new AbortController();
    this.#controller = controller;
    this.#publish({
      loading: true,
      saving: false,
      weapon: null,
      error: null,
      status: null,
    });
    void this.#client
      .read(
        {
          expectedProjectHash: this.#context.project.projectHash,
          ownerEntityId: this.#context.ownerEntityId,
        },
        controller.signal,
      )
      .then((weapon) => {
        if (!this.#isCurrent(controller, generation)) return;
        this.#controller = null;
        this.#publish({
          loading: false,
          saving: false,
          weapon,
          error: null,
          status: null,
        });
      })
      .catch((error: unknown) => {
        if (!this.#isCurrent(controller, generation)) return;
        this.#controller = null;
        this.#publish({
          loading: false,
          saving: false,
          weapon: null,
          error: loadingBayWeaponOperationMessage(error),
          status: null,
        });
      });
  }

  save(candidate: LoadingBayWeaponCandidate): void {
    const weapon = this.#state.weapon;
    if (this.#disposed || this.#state.saving || weapon === null) return;

    let lease: StudioEntityInspectorMutationLease;
    try {
      lease = this.#mutationPort.acquire(this.#context);
    } catch (error: unknown) {
      this.#publish({
        ...this.#state,
        error: loadingBayWeaponOperationMessage(error),
        status: null,
      });
      return;
    }

    const generation = ++this.#generation;
    const controller = new AbortController();
    this.#controller = controller;
    this.#lease = lease;
    this.#leaseOpen = true;
    this.#publish({
      ...this.#state,
      saving: true,
      error: null,
      status: null,
    });
    void this.#client
      .replace(
        {
          expectedProjectHash: lease.context.project.projectHash,
          ownerEntityId: lease.context.ownerEntityId,
          expectedComponentRevision: weapon.componentRevision,
          candidate,
        },
        controller.signal,
      )
      .then(async ({ receipt, weapon: replacedWeapon }) => {
        if (!this.#isCurrent(controller, generation)) return;
        this.#leaseOpen = false;
        const settlement = await lease.settle({
          beforeProjectHash: receipt.projectHashBefore,
          afterProjectHash: receipt.projectHashAfter,
        });
        if (!this.#isCurrent(controller, generation)) return;
        if (settlement.kind === "accepted") {
          this.#publish({
            loading: false,
            saving: false,
            weapon: replacedWeapon,
            error: null,
            status: "Saved and reread",
          });
          return;
        }
        this.#publish({
          loading: false,
          saving: false,
          weapon: null,
          error:
            settlement.kind === "stale"
              ? "The project or selection changed before the edit settled."
              : settlement.message,
          status: null,
        });
      })
      .catch((error: unknown) => {
        if (!this.#isCurrent(controller, generation)) return;
        if (this.#leaseOpen) {
          this.#leaseOpen = false;
          lease.reject(error);
        }
        this.#publish({
          ...this.#state,
          saving: false,
          error: loadingBayWeaponOperationMessage(error),
          status: null,
        });
      })
      .finally(() => {
        if (this.#isCurrent(controller, generation)) {
          this.#controller = null;
          this.#lease = null;
          this.#leaseOpen = false;
        }
      });
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#generation += 1;
    this.#controller?.abort();
    if (this.#lease !== null && this.#leaseOpen) {
      this.#lease.reject(new Error("Weapon inspector was disposed."));
    }
    this.#controller = null;
    this.#lease = null;
    this.#leaseOpen = false;
  }

  #publish(state: LoadingBayWeaponInspectorState): void {
    if (this.#disposed) return;
    this.#state = Object.freeze(state);
    this.#listener(this.#state);
  }

  #isCurrent(controller: AbortController, generation: number): boolean {
    return (
      !this.#disposed &&
      !controller.signal.aborted &&
      this.#controller === controller &&
      this.#generation === generation
    );
  }
}

export function loadingBayWeaponOperationMessage(error: unknown): string {
  if (error instanceof LoadingBayWeaponOperationRejected) {
    const path =
      error.rejection.path === undefined ? "" : ` (${error.rejection.path})`;
    return `${error.rejection.message}${path}`;
  }
  return error instanceof Error ? error.message : String(error);
}
