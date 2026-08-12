import type { RuntimeBrowserState } from "./projection.js";

export const LOADING_BAY_PROTOCOL_VERSION = 2;
export const MAX_PENDING_EDGE_COMMANDS = 32;
export const MAX_WEBSOCKET_BUFFERED_BYTES = 64 * 1024;
export const INPUT_SEND_INTERVAL_MILLISECONDS = 1_000 / 60;
export const MAX_SESSION_LOOK_UNITS = 64;
export const SESSION_CONNECT_TIMEOUT_MILLISECONDS = 5_000;
export const SESSION_CLOSE_TIMEOUT_MILLISECONDS = 5_000;

export interface SessionInputIntent {
  readonly movement: readonly [number, number];
  readonly lookDelta: readonly [number, number];
  readonly jumpHeld?: boolean;
  readonly primaryFireHeld: boolean;
}

export interface SessionStateDelivery {
  readonly sessionReplaced: boolean;
}

export type SessionRejectionCode =
  | "protocolMismatch"
  | "sessionClosed"
  | "transportLost"
  | "staleSequence"
  | "edgeQueueSaturated"
  | "deltaBaseUnavailable"
  | "contentRevisionMismatch"
  | "invalidInput"
  | "unknownTarget"
  | "notInteractable"
  | "cooldown"
  | "noAmmo"
  | "noEquippedWeapon"
  | "invalidWeaponSlot"
  | "weaponNotOwned"
  | "weaponAlreadySelected"
  | "playerDefeated"
  | "itemNotOwned"
  | "itemNotUsable"
  | "healthFull"
  | "checkpointUnavailable"
  | "saveUnavailable"
  | "saveOverwriteRequired"
  | "saveStale"
  | "snapshotCorrupt"
  | "snapshotIncompatible"
  | "paused"
  | "internalDefect";

export interface SessionMetrics {
  readonly inboundCommandCount: number;
  readonly outboundUpdateCount: number;
  readonly rejectedCommandCount: number;
  readonly lastInboundBytes: number;
  readonly lastOutboundBytes: number;
  readonly legacyWholeStateBytes: number;
  readonly bootstrapOutboundBytes: number;
  readonly staticResourceUpdateCount: number;
  readonly staticResourceLastBytes: number;
  readonly staticResourceMaxBytes: number;
  readonly steadyStateLastBytes: number;
  readonly steadyStateMaxBytes: number;
  readonly steadyStateUpdateCount: number;
  readonly maximumPendingOutboundUpdates: number;
  readonly droppedFactCount: number;
  readonly lastUpdateBuildMicroseconds: number;
  readonly maximumUpdateBuildMicroseconds: number;
}

type StaticStateKey =
  | "hostSessionId"
  | "projectId"
  | "voxelRevision"
  | "voxelAuthorityHash"
  | "voxelSolidCount"
  | "voxelNavigationHash"
  | "voxelProbePathLength"
  | "voxelEnvironmentRole"
  | "voxelMeshes"
  | "voxelObjectFrame"
  | "lights"
  | "renderMaterials"
  | "staticMeshes"
  | "animatedMeshes"
  | "visualBindings"
  | "generatedEnvironment"
  | "applicationContent";

type RuntimeDynamicState = Omit<RuntimeBrowserState, StaticStateKey>;
type RuntimeStaticResources = Pick<RuntimeBrowserState, StaticStateKey> & {
  readonly staticRevision: string;
};

interface FullStateUpdate {
  readonly kind: "full";
  readonly state: RuntimeDynamicState;
}

interface DeltaStateUpdate {
  readonly kind: "delta";
  readonly baseSnapshotSequence: number;
  readonly changes: Readonly<Record<string, unknown>>;
}

export interface ServerUpdateEnvelope {
  readonly protocolVersion: number;
  readonly sessionId: string;
  readonly connectionGeneration: number;
  readonly serverTick: number;
  readonly snapshotSequence: number;
  readonly acknowledgedCommandSequence: number;
  readonly staticRevision: string;
  readonly update: FullStateUpdate | DeltaStateUpdate;
  readonly resources?: RuntimeStaticResources;
  readonly facts: readonly {
    readonly kind: string;
    readonly code?: SessionRejectionCode;
    readonly commandSequence?: number;
  }[];
  readonly metrics: SessionMetrics;
}

interface CommandRejectionEnvelope {
  readonly protocolVersion: number;
  readonly sessionId?: string;
  readonly commandSequence?: number;
  readonly acknowledgedCommandSequence: number;
  readonly code: SessionRejectionCode;
  readonly retry: "never" | "reconnect" | "resync";
  readonly message: string;
}

type ClientGameCommand =
  | ({
      readonly kind: "setInputIntent";
    } & SessionInputIntent)
  | { readonly kind: "interact"; readonly target: number }
  | { readonly kind: "jump" }
  | { readonly kind: "selectWeaponSlot"; readonly slot: number }
  | { readonly kind: "useItem"; readonly item: string }
  | { readonly kind: "setPaused"; readonly paused: boolean }
  | {
      readonly kind: "restart";
      readonly mode: "authoredBaseline" | "checkpoint";
    }
  | {
      readonly kind: "saveGame";
      readonly slot: "checkpoint" | "slot1" | "slot2" | "slot3";
      readonly overwrite: boolean;
      readonly expectedStorageRevision: string | null;
    }
  | {
      readonly kind: "loadGame";
      readonly slot: "checkpoint" | "slot1" | "slot2" | "slot3";
      readonly expectedStorageRevision: string | null;
    };

interface RequestFullStateCommand {
  readonly kind: "requestFullState";
}

interface ClientCommandEnvelope {
  readonly protocolVersion: number;
  readonly sessionId: string;
  readonly sequence: number;
  readonly observedSnapshotSequence: number;
  readonly observedStaticRevision: string;
  readonly requestFullState: boolean;
  readonly command: ClientGameCommand | RequestFullStateCommand;
}

interface SessionBaseline {
  readonly sessionId: string;
  readonly snapshotSequence: number;
  readonly dynamic: RuntimeDynamicState;
  readonly resources: RuntimeStaticResources;
}

export class GameSessionError extends Error {
  readonly code: SessionRejectionCode;
  readonly retry: "never" | "reconnect" | "resync";

  constructor(
    code: SessionRejectionCode,
    message: string,
    retry: "never" | "reconnect" | "resync" = "never",
  ) {
    super(message);
    this.name = "GameSessionError";
    this.code = code;
    this.retry = retry;
  }
}

function factRejectionMessage(kind: string): string {
  switch (kind) {
    case "InputEdgeRejectedSwitchOutOfRange":
      return "Move closer to use this switch.";
    case "InputEdgeRejectedSwitchUnavailable":
      return "This switch is unavailable; wait for its current action to finish.";
    default:
      return kind;
  }
}

export interface AppliedServerUpdate {
  readonly baseline: SessionBaseline;
  readonly state: RuntimeBrowserState;
}

export function applyServerUpdate(
  previous: SessionBaseline | null,
  envelope: ServerUpdateEnvelope,
): AppliedServerUpdate {
  if (envelope.protocolVersion !== LOADING_BAY_PROTOCOL_VERSION) {
    throw new GameSessionError(
      "protocolMismatch",
      `server selected protocol ${String(envelope.protocolVersion)}`,
    );
  }
  const resources = envelope.resources ?? previous?.resources;
  if (
    resources === undefined ||
    resources.staticRevision !== envelope.staticRevision
  ) {
    throw new GameSessionError(
      "contentRevisionMismatch",
      "state update does not include resources for its static revision",
      "resync",
    );
  }
  if (!isRuntimeStaticResources(resources)) {
    throw new GameSessionError(
      "protocolMismatch",
      "static resources do not match the bounded Loading Bay render projection",
    );
  }

  let dynamic: RuntimeDynamicState;
  if (envelope.update.kind === "full") {
    dynamic = envelope.update.state;
  } else {
    if (
      previous === null ||
      previous.sessionId !== envelope.sessionId ||
      previous.snapshotSequence !== envelope.update.baseSnapshotSequence ||
      envelope.snapshotSequence !== envelope.update.baseSnapshotSequence + 1
    ) {
      throw new GameSessionError(
        "deltaBaseUnavailable",
        "dynamic update does not extend the accepted snapshot",
        "resync",
      );
    }
    dynamic = applyDynamicChanges(previous.dynamic, envelope.update.changes);
  }
  if (!isRuntimeDynamicState(dynamic)) {
    throw new GameSessionError(
      "protocolMismatch",
      "state update does not match the Loading Bay dynamic projection",
    );
  }

  const runtimeResources = runtimeStateResources(resources);
  return {
    baseline: {
      sessionId: envelope.sessionId,
      snapshotSequence: envelope.snapshotSequence,
      dynamic,
      resources,
    },
    state: { ...dynamic, ...runtimeResources },
  };
}

function applyDynamicChanges(
  previous: RuntimeDynamicState,
  changes: Readonly<Record<string, unknown>>,
): RuntimeDynamicState {
  const dynamic: Record<string, unknown> = { ...previous };
  for (const [owner, change] of Object.entries(changes)) {
    if (!isKeyedCollectionPatch(change)) {
      dynamic[owner] = change;
      continue;
    }
    const collection = dynamic[owner];
    if (!Array.isArray(collection)) {
      throw new GameSessionError(
        "protocolMismatch",
        `keyed collection patch has no array baseline for ${owner}`,
      );
    }
    dynamic[owner] = applyKeyedCollectionPatch(owner, collection, change);
  }
  return dynamic as unknown as RuntimeDynamicState;
}

interface KeyedCollectionPatch {
  readonly $collectionPatch: 1;
  readonly key: "id" | "slot";
  readonly upserts: readonly Record<string, unknown>[];
  readonly removed: readonly (number | string)[];
}

function applyKeyedCollectionPatch(
  owner: string,
  previous: readonly unknown[],
  patch: KeyedCollectionPatch,
): readonly unknown[] {
  const next = [...previous];
  const positions = new Map<number | string, number>();
  for (const [index, value] of next.entries()) {
    const identity = collectionIdentity(value, patch.key);
    if (identity === null || positions.has(identity)) {
      throw new GameSessionError(
        "protocolMismatch",
        `keyed collection ${owner} has an invalid ${patch.key} baseline`,
      );
    }
    positions.set(identity, index);
  }
  const removed = new Set(patch.removed);
  for (const identity of removed) {
    if (!positions.has(identity)) {
      throw new GameSessionError(
        "protocolMismatch",
        `keyed collection ${owner} removes unknown ${patch.key} ${String(identity)}`,
      );
    }
  }
  const retained = next.filter((value) => {
    const identity = collectionIdentity(value, patch.key);
    return identity !== null && !removed.has(identity);
  });
  const retainedPositions = new Map<number | string, number>(
    retained.map((value, index) => [
      collectionIdentity(value, patch.key) as number | string,
      index,
    ]),
  );
  for (const value of patch.upserts) {
    const identity = collectionIdentity(value, patch.key);
    if (identity === null) {
      throw new GameSessionError(
        "protocolMismatch",
        `keyed collection ${owner} upsert has no ${patch.key}`,
      );
    }
    const index = retainedPositions.get(identity);
    if (index === undefined) {
      retainedPositions.set(identity, retained.length);
      retained.push(value);
    } else {
      retained[index] = value;
    }
  }
  return retained;
}

function collectionIdentity(
  value: unknown,
  key: "id" | "slot",
): number | string | null {
  if (!isRecord(value)) {
    return null;
  }
  const identity = value[key];
  return typeof identity === "number" || typeof identity === "string"
    ? identity
    : null;
}

function isKeyedCollectionPatch(value: unknown): value is KeyedCollectionPatch {
  return (
    isRecord(value) &&
    value.$collectionPatch === 1 &&
    (value.key === "id" || value.key === "slot") &&
    Array.isArray(value.upserts) &&
    value.upserts.every(isRecord) &&
    Array.isArray(value.removed) &&
    value.removed.every(
      (identity) =>
        typeof identity === "number" || typeof identity === "string",
    )
  );
}

interface PendingSettlement {
  readonly promise: Promise<RuntimeBrowserState>;
  readonly resolve: (state: RuntimeBrowserState) => void;
  readonly reject: (error: Error) => void;
}

interface PendingEdge {
  readonly sequence: number;
  readonly resolve: (state: RuntimeBrowserState) => void;
  readonly reject: (error: Error) => void;
}

export class LoadingBayGameSession {
  readonly #socket: WebSocket;
  #baseline: SessionBaseline;
  #current: RuntimeBrowserState;
  #metrics: SessionMetrics;
  #serverTick: number;
  #snapshotSequence: number;
  #lastSnapshotReceivedAtMilliseconds: number;
  #lastSnapshotCadenceMilliseconds: number | null = null;
  #sequence = 0;
  #inputInFlight: number | null = null;
  #pendingInput = false;
  #pendingLook: [number, number] = [0, 0];
  #latestMovement: [number, number] = [0, 0];
  #pendingMovementEdge: [number, number] = [0, 0];
  #jumpHeld = false;
  #primaryFireHeld = false;
  #inputTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
  #lastInputSentAt = 0;
  #inputSettlement: PendingSettlement | null = null;
  #pendingEdges = new Map<number, PendingEdge>();
  #restart: PendingEdge | null = null;
  #sentAt = new Map<number, number>();
  #lastCommandRoundTripMilliseconds = 0;
  #maximumCommandRoundTripMilliseconds = 0;
  #maximumPendingInputFrameCount = 0;
  #maximumPendingEdgeCount = 0;
  #resyncInFlight: number | null = null;
  #resyncTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
  #closed = false;
  #closePromise: Promise<void> | null = null;
  #onState:
    | ((state: RuntimeBrowserState, delivery: SessionStateDelivery) => void)
    | null = null;
  #onFailure: ((error: GameSessionError) => void) | null = null;

  private constructor(
    socket: WebSocket,
    applied: AppliedServerUpdate,
    metrics: SessionMetrics,
    serverTick: number,
  ) {
    this.#socket = socket;
    this.#baseline = applied.baseline;
    this.#current = applied.state;
    this.#metrics = metrics;
    this.#serverTick = serverTick;
    this.#snapshotSequence = applied.baseline.snapshotSequence;
    this.#lastSnapshotReceivedAtMilliseconds = performance.now();
    socket.addEventListener("message", (event) => this.#receive(event.data));
    socket.addEventListener("close", () => {
      if (!this.#closed) {
        this.#failTransport(
          new GameSessionError(
            "transportLost",
            "Loading Bay game session closed",
            "reconnect",
          ),
        );
      }
    });
    socket.addEventListener("error", () => {
      this.#failTransport(
        new GameSessionError(
          "transportLost",
          "Loading Bay game session failed",
          "reconnect",
        ),
      );
    });
  }

  static async connect(): Promise<LoadingBayGameSession> {
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const socket = new WebSocket(
      `${scheme}://${location.host}/api/session`,
      `loading-bay.v${String(LOADING_BAY_PROTOCOL_VERSION)}`,
    );
    const first = await new Promise<ServerUpdateEnvelope>((resolve, reject) => {
      const cleanup = (): void => {
        globalThis.clearTimeout(timeout);
        socket.removeEventListener("message", received);
        socket.removeEventListener("error", failed);
        socket.removeEventListener("close", failed);
      };
      const received = (event: MessageEvent): void => {
        try {
          const value = decodeMessage(event.data);
          if (!isServerUpdate(value)) {
            throw new GameSessionError(
              "protocolMismatch",
              "first session message was not a full state update",
            );
          }
          cleanup();
          resolve(value);
        } catch (error) {
          cleanup();
          socket.close(1002, "invalid session bootstrap");
          reject(error);
        }
      };
      const failed = (): void => {
        cleanup();
        reject(
          new GameSessionError(
            "transportLost",
            "could not open Loading Bay game session",
            "reconnect",
          ),
        );
      };
      const timeout = globalThis.setTimeout(() => {
        cleanup();
        socket.close();
        reject(
          new GameSessionError(
            "transportLost",
            "Loading Bay game session bootstrap timed out",
            "reconnect",
          ),
        );
      }, SESSION_CONNECT_TIMEOUT_MILLISECONDS);
      socket.addEventListener("message", received);
      socket.addEventListener("error", failed);
      socket.addEventListener("close", failed);
    });
    if (first.update.kind !== "full") {
      socket.close(1002, "full bootstrap required");
      throw new GameSessionError(
        "deltaBaseUnavailable",
        "session bootstrap did not contain a full projection",
      );
    }
    try {
      return new LoadingBayGameSession(
        socket,
        applyServerUpdate(null, first),
        first.metrics,
        first.serverTick,
      );
    } catch (error) {
      socket.close(1002, "invalid session bootstrap");
      throw error;
    }
  }

  get state(): RuntimeBrowserState {
    return this.#current;
  }

  get metrics(): SessionMetrics {
    return this.#metrics;
  }

  get serverTick(): number {
    return this.#serverTick;
  }

  get snapshotSequence(): number {
    return this.#snapshotSequence;
  }

  get lastSnapshotCadenceMilliseconds(): number | null {
    return this.#lastSnapshotCadenceMilliseconds;
  }

  get pendingEdgeCount(): number {
    return this.#pendingEdges.size + Number(this.#restart !== null);
  }

  get pendingInputFrameCount(): number {
    return Number(this.#inputInFlight !== null) + Number(this.#pendingInput);
  }

  get lastCommandRoundTripMilliseconds(): number {
    return this.#lastCommandRoundTripMilliseconds;
  }

  get maximumCommandRoundTripMilliseconds(): number {
    return this.#maximumCommandRoundTripMilliseconds;
  }

  get maximumPendingInputFrameCount(): number {
    return this.#maximumPendingInputFrameCount;
  }

  get maximumPendingEdgeCount(): number {
    return this.#maximumPendingEdgeCount;
  }

  setStateListener(
    listener: (
      state: RuntimeBrowserState,
      delivery: SessionStateDelivery,
    ) => void,
  ): void {
    this.#onState = listener;
  }

  setFailureListener(listener: (error: GameSessionError) => void): void {
    this.#onFailure = listener;
  }

  queueInput(intent: SessionInputIntent): void {
    if (this.#closed) {
      return;
    }
    const nextMovement: [number, number] = [
      clampUnit(intent.movement[0]),
      clampUnit(intent.movement[1]),
    ];
    this.#pendingMovementEdge = preserveMovementPress(
      this.#latestMovement,
      nextMovement,
      this.#pendingMovementEdge,
    );
    this.#latestMovement = nextMovement;
    this.#primaryFireHeld = intent.primaryFireHeld;
    this.#jumpHeld = intent.jumpHeld === true;
    this.#pendingLook = coalesceSessionLook(
      this.#pendingLook,
      intent.lookDelta,
    );
    this.#pendingInput = true;
    this.#maximumPendingInputFrameCount = Math.max(
      this.#maximumPendingInputFrameCount,
      this.pendingInputFrameCount,
    );
    this.#scheduleInput();
  }

  neutralizeInput(): void {
    if (this.#closed) {
      return;
    }
    this.#latestMovement = [0, 0];
    this.#pendingMovementEdge = [0, 0];
    this.#pendingLook = [0, 0];
    this.#jumpHeld = false;
    this.#primaryFireHeld = false;
    this.#pendingInput = true;
    this.#maximumPendingInputFrameCount = Math.max(
      this.#maximumPendingInputFrameCount,
      this.pendingInputFrameCount,
    );
    this.#scheduleInput();
  }

  discardInputForSessionReplacement(): void {
    if (this.#closed) {
      return;
    }
    this.#clearBufferedInput();
  }

  sendInput(intent: SessionInputIntent): Promise<RuntimeBrowserState> {
    this.queueInput(intent);
    return this.#ensureInputSettlement().promise;
  }

  sendEdge(
    command: Exclude<ClientGameCommand, { readonly kind: "setInputIntent" }>,
  ): Promise<RuntimeBrowserState> {
    if (this.#closed) {
      return Promise.reject(
        new GameSessionError(
          "transportLost",
          "cannot send on a closed game session",
          "reconnect",
        ),
      );
    }
    if (this.pendingEdgeCount >= MAX_PENDING_EDGE_COMMANDS) {
      return Promise.reject(
        new GameSessionError(
          "edgeQueueSaturated",
          `client edge queue capacity ${String(MAX_PENDING_EDGE_COMMANDS)} reached`,
        ),
      );
    }
    if (this.#restart !== null) {
      return Promise.reject(
        new GameSessionError(
          "edgeQueueSaturated",
          "the current session is waiting for authoritative replacement",
        ),
      );
    }
    if (this.#socket.bufferedAmount > MAX_WEBSOCKET_BUFFERED_BYTES) {
      return Promise.reject(
        new GameSessionError(
          "transportLost",
          "session transport is not accepting bounded edge delivery",
          "reconnect",
        ),
      );
    }
    const sequence = this.#nextSequence();
    return new Promise<RuntimeBrowserState>((resolve, reject) => {
      const pending = { sequence, resolve, reject };
      if (replacesSession(command)) {
        this.#restart = pending;
      } else {
        this.#pendingEdges.set(sequence, pending);
      }
      this.#maximumPendingEdgeCount = Math.max(
        this.#maximumPendingEdgeCount,
        this.pendingEdgeCount,
      );
      try {
        this.#sendEnvelope(sequence, command);
      } catch (error) {
        this.#pendingEdges.delete(sequence);
        if (this.#restart?.sequence === sequence) {
          this.#restart = null;
        }
        this.#sentAt.delete(sequence);
        reject(
          error instanceof GameSessionError
            ? error
            : new GameSessionError(
                "transportLost",
                error instanceof Error ? error.message : String(error),
                "reconnect",
              ),
        );
      }
    });
  }

  async close(): Promise<void> {
    if (this.#closePromise !== null) {
      return this.#closePromise;
    }
    this.#closed = true;
    if (this.#inputTimer !== null) {
      globalThis.clearTimeout(this.#inputTimer);
      this.#inputTimer = null;
    }
    this.#clearResync();
    this.#clearBufferedInput();
    this.#rejectPending(
      new GameSessionError(
        "sessionClosed",
        "Loading Bay game session disposed",
      ),
    );
    this.#closePromise = new Promise<void>((resolve, reject) => {
      const cleanup = (): void => {
        globalThis.clearTimeout(timeout);
        this.#socket.removeEventListener("close", closed);
        this.#socket.removeEventListener("error", failed);
      };
      const closed = (): void => {
        cleanup();
        resolve();
      };
      const failed = (): void => {
        cleanup();
        reject(
          new GameSessionError(
            "transportLost",
            "Loading Bay game session failed while retiring its transport",
            "reconnect",
          ),
        );
      };
      const timeout = globalThis.setTimeout(() => {
        cleanup();
        reject(
          new GameSessionError(
            "transportLost",
            "Loading Bay game session transport retirement timed out",
            "reconnect",
          ),
        );
      }, SESSION_CLOSE_TIMEOUT_MILLISECONDS);
      this.#socket.addEventListener("close", closed);
      this.#socket.addEventListener("error", failed);
      if (this.#socket.readyState === 3) {
        closed();
        return;
      }
      try {
        this.#socket.close(1000, "route disposed");
      } catch (error) {
        cleanup();
        reject(
          error instanceof Error
            ? error
            : new GameSessionError(
                "transportLost",
                "Loading Bay game session transport retirement failed",
                "reconnect",
              ),
        );
      }
    });
    return this.#closePromise;
  }

  #receive(data: unknown): void {
    try {
      const value = decodeMessage(data);
      if (isCommandRejection(value)) {
        this.#receiveRejection(value);
        return;
      }
      if (!isServerUpdate(value)) {
        throw new GameSessionError(
          "protocolMismatch",
          "session message has no recognized envelope",
        );
      }
      const replaced = value.sessionId !== this.#baseline.sessionId;
      let applied: AppliedServerUpdate;
      try {
        applied = applyServerUpdate(this.#baseline, value);
      } catch (error) {
        if (error instanceof GameSessionError && error.retry === "resync") {
          this.#requestResync();
          return;
        }
        throw error;
      }
      if (value.update.kind === "full") {
        this.#clearResync();
      }
      this.#baseline = applied.baseline;
      this.#current = applied.state;
      this.#metrics = value.metrics;
      const receivedAtMilliseconds = performance.now();
      this.#serverTick = value.serverTick;
      this.#snapshotSequence = value.snapshotSequence;
      this.#lastSnapshotCadenceMilliseconds = replaced
        ? null
        : Math.max(
            0,
            receivedAtMilliseconds - this.#lastSnapshotReceivedAtMilliseconds,
          );
      this.#lastSnapshotReceivedAtMilliseconds = receivedAtMilliseconds;
      if (replaced) {
        const restart = this.#restart;
        if (restart !== null) {
          this.#recordRoundTrip(restart.sequence);
        }
        this.#sequence = 0;
        this.#inputInFlight = null;
        this.#clearBufferedInput();
        const replacedError = new GameSessionError(
          "sessionClosed",
          "pending work belonged to the replaced session",
        );
        this.#inputSettlement?.reject(replacedError);
        this.#inputSettlement = null;
        for (const edge of this.#pendingEdges.values()) {
          edge.reject(replacedError);
        }
        this.#pendingEdges.clear();
        this.#restart = null;
        restart?.resolve(this.#current);
        this.#sentAt.clear();
      }
      this.#receiveFactRejections(value.facts);
      this.#acceptAcknowledgement(value.acknowledgedCommandSequence);
      this.#onState?.(this.#current, { sessionReplaced: replaced });
    } catch (error) {
      this.#failTransport(
        error instanceof GameSessionError
          ? error
          : new GameSessionError(
              "protocolMismatch",
              error instanceof Error ? error.message : String(error),
            ),
      );
    }
  }

  #receiveRejection(rejection: CommandRejectionEnvelope): void {
    const sequence = rejection.commandSequence;
    const belongsToTrackedWork =
      sequence === undefined ||
      sequence === this.#resyncInFlight ||
      sequence === this.#inputInFlight ||
      this.#pendingEdges.has(sequence) ||
      this.#restart?.sequence === sequence ||
      this.#sentAt.has(sequence);
    if (
      rejection.code === "sessionClosed" &&
      sequence !== undefined &&
      !belongsToTrackedWork
    ) {
      // An authoritative restart/load replaces the session id and clears all
      // old pending work. The host can still drain one already-buffered old
      // envelope afterwards; its rejection belongs to the retired session and
      // must not take the successfully replaced browser session offline.
      return;
    }
    const error = new GameSessionError(
      rejection.code,
      rejection.message,
      rejection.retry,
    );
    if (rejection.commandSequence === this.#resyncInFlight) {
      this.#clearResync();
      this.#failTransport(
        new GameSessionError(
          rejection.code,
          `full-state recovery was rejected: ${rejection.message}`,
          "reconnect",
        ),
      );
      return;
    }
    if (rejection.commandSequence === this.#inputInFlight) {
      this.#inputInFlight = null;
      this.#inputSettlement?.reject(error);
      this.#inputSettlement = null;
    }
    if (rejection.commandSequence !== undefined) {
      this.#recordRoundTrip(rejection.commandSequence);
      const edge = this.#pendingEdges.get(rejection.commandSequence);
      edge?.reject(error);
      this.#pendingEdges.delete(rejection.commandSequence);
      if (this.#restart?.sequence === rejection.commandSequence) {
        this.#restart.reject(error);
        this.#restart = null;
      }
    }
    this.#onFailure?.(error);
    if (this.#pendingInput) {
      this.#scheduleInput();
    }
  }

  #receiveFactRejections(facts: ServerUpdateEnvelope["facts"]): void {
    for (const fact of facts) {
      if (fact.code === undefined) {
        continue;
      }
      const error = new GameSessionError(
        fact.code,
        factRejectionMessage(fact.kind),
      );
      if (fact.commandSequence !== undefined) {
        const edge = this.#pendingEdges.get(fact.commandSequence);
        edge?.reject(error);
        this.#pendingEdges.delete(fact.commandSequence);
        if (this.#restart?.sequence === fact.commandSequence) {
          this.#restart.reject(error);
          this.#restart = null;
        }
      }
      this.#onFailure?.(error);
    }
  }

  #acceptAcknowledgement(acknowledged: number): void {
    const consumed = this.#current.input.consumedSequence;
    const settledThrough = Math.min(consumed, acknowledged);
    for (const sequence of this.#sentAt.keys()) {
      if (sequence <= settledThrough) {
        this.#recordRoundTrip(sequence);
      }
    }
    if (this.#inputInFlight !== null && consumed >= this.#inputInFlight) {
      this.#inputInFlight = null;
    }
    for (const [sequence, edge] of this.#pendingEdges) {
      if (consumed >= sequence && acknowledged >= sequence) {
        edge.resolve(this.#current);
        this.#pendingEdges.delete(sequence);
      }
    }
    if (this.#pendingInput) {
      this.#scheduleInput();
    } else if (this.#inputInFlight === null && this.#inputSettlement !== null) {
      this.#inputSettlement.resolve(this.#current);
      this.#inputSettlement = null;
    }
  }

  #scheduleInput(): void {
    if (
      this.#closed ||
      !this.#pendingInput ||
      this.#inputInFlight !== null ||
      this.#inputTimer !== null
    ) {
      return;
    }
    const elapsed = performance.now() - this.#lastInputSentAt;
    const delay = Math.max(0, INPUT_SEND_INTERVAL_MILLISECONDS - elapsed);
    this.#inputTimer = globalThis.setTimeout(() => {
      this.#inputTimer = null;
      this.#flushInput();
    }, delay);
  }

  #flushInput(): void {
    if (this.#closed || !this.#pendingInput || this.#inputInFlight !== null) {
      return;
    }
    if (this.#socket.bufferedAmount > MAX_WEBSOCKET_BUFFERED_BYTES) {
      this.#scheduleInput();
      return;
    }
    const sequence = this.#nextSequence();
    const { movement, mustFollowWithLatestMovement } = resolveMovementSample(
      this.#latestMovement,
      this.#pendingMovementEdge,
    );
    const command: ClientGameCommand = {
      kind: "setInputIntent",
      movement,
      lookDelta: this.#pendingLook,
      jumpHeld: this.#jumpHeld,
      primaryFireHeld: this.#primaryFireHeld,
    };
    this.#pendingInput = mustFollowWithLatestMovement;
    this.#pendingMovementEdge = [0, 0];
    this.#pendingLook = [0, 0];
    this.#inputInFlight = sequence;
    this.#lastInputSentAt = performance.now();
    try {
      this.#sendEnvelope(sequence, command);
    } catch (error) {
      this.#inputInFlight = null;
      this.#sentAt.delete(sequence);
      this.#failTransport(
        error instanceof GameSessionError
          ? error
          : new GameSessionError(
              "transportLost",
              error instanceof Error ? error.message : String(error),
              "reconnect",
            ),
      );
    }
  }

  #requestResync(): void {
    if (
      this.#closed ||
      this.#resyncInFlight !== null ||
      this.#resyncTimer !== null
    ) {
      return;
    }
    if (this.#socket.bufferedAmount > MAX_WEBSOCKET_BUFFERED_BYTES) {
      this.#resyncTimer = globalThis.setTimeout(() => {
        this.#resyncTimer = null;
        this.#requestResync();
      }, INPUT_SEND_INTERVAL_MILLISECONDS);
      return;
    }
    const sequence = this.#nextSequence();
    this.#resyncInFlight = sequence;
    try {
      this.#sendEnvelope(
        sequence,
        { kind: "requestFullState" },
        { requestFullState: true, trackRoundTrip: false },
      );
    } catch (error) {
      this.#resyncInFlight = null;
      this.#failTransport(
        error instanceof GameSessionError
          ? error
          : new GameSessionError(
              "transportLost",
              error instanceof Error ? error.message : String(error),
              "reconnect",
            ),
      );
    }
  }

  #sendEnvelope(
    sequence: number,
    command: ClientGameCommand | RequestFullStateCommand,
    options: {
      readonly requestFullState?: boolean;
      readonly trackRoundTrip?: boolean;
    } = {},
  ): void {
    if (this.#socket.readyState !== WebSocket.OPEN) {
      throw new GameSessionError(
        "transportLost",
        "Loading Bay game session is not open",
        "reconnect",
      );
    }
    const envelope: ClientCommandEnvelope = {
      protocolVersion: LOADING_BAY_PROTOCOL_VERSION,
      sessionId: this.#baseline.sessionId,
      sequence,
      observedSnapshotSequence: this.#baseline.snapshotSequence,
      observedStaticRevision: this.#baseline.resources.staticRevision,
      requestFullState: options.requestFullState ?? false,
      command,
    };
    if (options.trackRoundTrip !== false) {
      this.#sentAt.set(sequence, performance.now());
    }
    this.#socket.send(JSON.stringify(envelope));
  }

  #nextSequence(): number {
    this.#sequence += 1;
    return this.#sequence;
  }

  #ensureInputSettlement(): PendingSettlement {
    if (this.#inputSettlement !== null) {
      return this.#inputSettlement;
    }
    let resolve!: (state: RuntimeBrowserState) => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<RuntimeBrowserState>(
      (resolveValue, rejectValue) => {
        resolve = resolveValue;
        reject = rejectValue;
      },
    );
    this.#inputSettlement = { promise, resolve, reject };
    return this.#inputSettlement;
  }

  #failTransport(error: GameSessionError): void {
    if (this.#closed) {
      return;
    }
    this.#closed = true;
    if (this.#inputTimer !== null) {
      globalThis.clearTimeout(this.#inputTimer);
      this.#inputTimer = null;
    }
    this.#clearResync();
    this.#clearBufferedInput();
    this.#rejectPending(error);
    this.#onFailure?.(error);
    try {
      this.#socket.close(1002, "session protocol failure");
    } catch {
      // A transport that has already failed may reject its own close handshake.
    }
  }

  #rejectPending(error: Error): void {
    this.#inputSettlement?.reject(error);
    this.#inputSettlement = null;
    for (const edge of this.#pendingEdges.values()) {
      edge.reject(error);
    }
    this.#pendingEdges.clear();
    this.#restart?.reject(error);
    this.#restart = null;
    this.#sentAt.clear();
  }

  #clearBufferedInput(): void {
    if (this.#inputTimer !== null) {
      globalThis.clearTimeout(this.#inputTimer);
      this.#inputTimer = null;
    }
    this.#pendingInput = false;
    this.#pendingLook = [0, 0];
    this.#latestMovement = [0, 0];
    this.#pendingMovementEdge = [0, 0];
    this.#jumpHeld = false;
    this.#primaryFireHeld = false;
  }

  #clearResync(): void {
    if (this.#resyncTimer !== null) {
      globalThis.clearTimeout(this.#resyncTimer);
      this.#resyncTimer = null;
    }
    this.#resyncInFlight = null;
  }

  #recordRoundTrip(sequence: number): void {
    const sentAt = this.#sentAt.get(sequence);
    if (sentAt === undefined) {
      return;
    }
    this.#sentAt.delete(sequence);
    const elapsed = performance.now() - sentAt;
    this.#lastCommandRoundTripMilliseconds = elapsed;
    this.#maximumCommandRoundTripMilliseconds = Math.max(
      this.#maximumCommandRoundTripMilliseconds,
      elapsed,
    );
  }
}

function decodeMessage(data: unknown): unknown {
  if (typeof data !== "string") {
    throw new GameSessionError(
      "protocolMismatch",
      "Loading Bay session messages must be JSON text",
    );
  }
  return JSON.parse(data) as unknown;
}

function isServerUpdate(value: unknown): value is ServerUpdateEnvelope {
  if (!isRecord(value) || !isRecord(value.update)) {
    return false;
  }
  const update =
    value.update.kind === "full"
      ? isRecord(value.update.state)
      : value.update.kind === "delta"
        ? isFiniteNumber(value.update.baseSnapshotSequence) &&
          isRecord(value.update.changes)
        : false;
  return (
    update &&
    isFiniteNumber(value.protocolVersion) &&
    typeof value.sessionId === "string" &&
    isFiniteNumber(value.connectionGeneration) &&
    isFiniteNumber(value.serverTick) &&
    isFiniteNumber(value.snapshotSequence) &&
    isFiniteNumber(value.acknowledgedCommandSequence) &&
    typeof value.staticRevision === "string" &&
    (value.resources === undefined ||
      isRuntimeStaticResources(value.resources)) &&
    Array.isArray(value.facts) &&
    value.facts.every(isSessionFact) &&
    isSessionMetrics(value.metrics)
  );
}

function isCommandRejection(value: unknown): value is CommandRejectionEnvelope {
  return (
    isRecord(value) &&
    isFiniteNumber(value.protocolVersion) &&
    (value.sessionId === undefined || typeof value.sessionId === "string") &&
    (value.commandSequence === undefined ||
      isFiniteNumber(value.commandSequence)) &&
    isFiniteNumber(value.acknowledgedCommandSequence) &&
    isSessionRejectionCode(value.code) &&
    (value.retry === "never" ||
      value.retry === "reconnect" ||
      value.retry === "resync") &&
    typeof value.message === "string"
  );
}

function clampUnit(value: number): number {
  return Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isSessionRejectionCode(value: unknown): value is SessionRejectionCode {
  return (
    value === "protocolMismatch" ||
    value === "sessionClosed" ||
    value === "transportLost" ||
    value === "staleSequence" ||
    value === "edgeQueueSaturated" ||
    value === "deltaBaseUnavailable" ||
    value === "contentRevisionMismatch" ||
    value === "invalidInput" ||
    value === "unknownTarget" ||
    value === "notInteractable" ||
    value === "cooldown" ||
    value === "noAmmo" ||
    value === "noEquippedWeapon" ||
    value === "invalidWeaponSlot" ||
    value === "weaponNotOwned" ||
    value === "weaponAlreadySelected" ||
    value === "playerDefeated" ||
    value === "itemNotOwned" ||
    value === "itemNotUsable" ||
    value === "healthFull" ||
    value === "checkpointUnavailable" ||
    value === "saveUnavailable" ||
    value === "saveOverwriteRequired" ||
    value === "saveStale" ||
    value === "snapshotCorrupt" ||
    value === "snapshotIncompatible" ||
    value === "paused" ||
    value === "internalDefect"
  );
}

function isSessionFact(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.kind === "string" &&
    (value.code === undefined || isSessionRejectionCode(value.code)) &&
    (value.commandSequence === undefined ||
      isFiniteNumber(value.commandSequence))
  );
}

function isSessionMetrics(value: unknown): value is SessionMetrics {
  if (!isRecord(value)) {
    return false;
  }
  return [
    "inboundCommandCount",
    "outboundUpdateCount",
    "rejectedCommandCount",
    "lastInboundBytes",
    "lastOutboundBytes",
    "legacyWholeStateBytes",
    "bootstrapOutboundBytes",
    "staticResourceUpdateCount",
    "staticResourceLastBytes",
    "staticResourceMaxBytes",
    "steadyStateLastBytes",
    "steadyStateMaxBytes",
    "steadyStateUpdateCount",
    "maximumPendingOutboundUpdates",
    "droppedFactCount",
    "lastUpdateBuildMicroseconds",
    "maximumUpdateBuildMicroseconds",
  ].every((key) => isFiniteNumber(value[key]));
}

function isRuntimeStaticResources(
  value: unknown,
): value is RuntimeStaticResources {
  if (
    !isRecord(value) ||
    typeof value.staticRevision !== "string" ||
    typeof value.hostSessionId !== "string" ||
    typeof value.projectId !== "string" ||
    !isFiniteNumber(value.voxelRevision) ||
    typeof value.voxelAuthorityHash !== "string" ||
    !isFiniteNumber(value.voxelSolidCount) ||
    typeof value.voxelNavigationHash !== "string" ||
    !isFiniteNumber(value.voxelProbePathLength) ||
    (value.voxelEnvironmentRole !== "visible" &&
      value.voxelEnvironmentRole !== "gameplayProxy" &&
      value.voxelEnvironmentRole !== "none") ||
    !Array.isArray(value.voxelMeshes) ||
    !isRecord(value.voxelObjectFrame) ||
    !Array.isArray(value.lights) ||
    !Array.isArray(value.renderMaterials) ||
    !Array.isArray(value.staticMeshes) ||
    !Array.isArray(value.animatedMeshes) ||
    !Array.isArray(value.visualBindings) ||
    (value.applicationContent !== undefined &&
      value.applicationContent !== null &&
      !isRuntimeApplicationContent(value.applicationContent)) ||
    (value.generatedEnvironment !== null &&
      !isRecord(value.generatedEnvironment))
  ) {
    return false;
  }
  return true;
}

function isRuntimeApplicationContent(value: unknown): boolean {
  if (
    !isRecord(value) ||
    !isRecord(value.frame) ||
    !Array.isArray(value.resources) ||
    value.resources.length > 512
  ) {
    return false;
  }
  const identities = new Set<string>();
  return value.resources.every((resource) => {
    if (
      !isRecord(resource) ||
      typeof resource.identity !== "string" ||
      !/^(mesh|texture)-resource\/[0-9a-f]{64}$/u.test(resource.identity) ||
      identities.has(resource.identity) ||
      typeof resource.contentHash !== "string" ||
      !/^sha256:[0-9a-f]{64}$/u.test(resource.contentHash) ||
      (resource.mediaType !== "application/octet-stream" &&
        resource.mediaType !== "image/png") ||
      !isFiniteNumber(resource.byteLength) ||
      resource.byteLength <= 0 ||
      typeof resource.resourceUrl !== "string" ||
      !/^\/api\/application-resource\/\d+$/u.test(resource.resourceUrl)
    ) {
      return false;
    }
    identities.add(resource.identity);
    return true;
  });
}

function isRuntimePlayerState(value: unknown): boolean {
  if (!isRecord(value) || !isRecord(value.bindings)) {
    return false;
  }
  const bindings = value.bindings;
  return (
    isFiniteNumber(value.id) &&
    isFiniteVector3(value.position) &&
    isFiniteNumber(value.yawDegrees) &&
    isFiniteNumber(value.pitchDegrees) &&
    isFiniteNumber(value.moveStepSeconds) &&
    isFiniteNumber(value.lookDegreesPerUnit) &&
    (value.grounded === undefined || typeof value.grounded === "boolean") &&
    (value.verticalVelocity === undefined ||
      isFiniteNumber(value.verticalVelocity)) &&
    (value.eyeHeight === undefined || isFiniteNumber(value.eyeHeight)) &&
    isFiniteNumber(value.currentHealth) &&
    isFiniteNumber(value.maxHealth) &&
    isFiniteNumber(value.armor) &&
    isFiniteNumber(value.maxArmor) &&
    (value.vitalityState === "alive" || value.vitalityState === "dead") &&
    [
      "moveForward",
      "moveBackward",
      "moveLeft",
      "moveRight",
      "mouseLook",
      "primaryFire",
    ].every((key) => typeof bindings[key] === "string") &&
    (bindings.jump === undefined ||
      bindings.jump === null ||
      typeof bindings.jump === "string") &&
    Array.isArray(bindings.selectWeapon) &&
    bindings.selectWeapon.every((binding) => typeof binding === "string")
  );
}

function isRuntimeWeaponState(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.item === "string" &&
    typeof value.presentation === "string" &&
    isFiniteNumber(value.damage) &&
    typeof value.ammunition === "string" &&
    isFiniteNumber(value.ammunitionCost) &&
    isFiniteNumber(value.ammoRemaining) &&
    isFiniteNumber(value.ammoCapacity) &&
    isFiniteNumber(value.readyAtTick)
  );
}

function isRuntimeInventoryState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.owner) &&
    isFiniteNumber(value.capacitySlots) &&
    (value.equippedWeapon === null ||
      typeof value.equippedWeapon === "string") &&
    Array.isArray(value.stacks) &&
    value.stacks.every(
      (stack) =>
        isRecord(stack) &&
        typeof stack.item === "string" &&
        isFiniteNumber(stack.quantity),
    ) &&
    Array.isArray(value.weapons) &&
    value.weapons.every(
      (weapon) =>
        isRecord(weapon) &&
        isFiniteNumber(weapon.slot) &&
        typeof weapon.item === "string" &&
        typeof weapon.owned === "boolean" &&
        typeof weapon.selected === "boolean" &&
        typeof weapon.ammunition === "string" &&
        isFiniteNumber(weapon.ammunitionQuantity),
    )
  );
}

function isRuntimeHazardState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.id) &&
    isFiniteNumber(value.damage) &&
    isFiniteNumber(value.cooldownTicks) &&
    isFiniteNumber(value.readyAtTick)
  );
}

function isRuntimeDoorAccessState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.id) &&
    (["closed", "opening", "open", "closing"] as const).includes(
      value.state as "closed" | "opening" | "open" | "closing",
    ) &&
    typeof value.requiredKey === "string" &&
    (value.keyPolicy === "retain" || value.keyPolicy === "consume") &&
    isFiniteNumber(value.activationRadius) &&
    typeof value.deniedPresentation === "string"
  );
}

function isRuntimeSecretRegionState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.id) &&
    (value.state === "undiscovered" || value.state === "discovered") &&
    typeof value.presentation === "string"
  );
}

function isRuntimeFloorActionState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.id) &&
    isFiniteNumber(value.targetPlatform) &&
    (value.state === "armed" ||
      value.state === "lowering" ||
      value.state === "lowered") &&
    isSafeNonNegativeInteger(value.motionElapsedTicks) &&
    typeof value.prompt === "string" &&
    typeof value.presentation === "string" &&
    typeof value.source === "string"
  );
}

function isRuntimeLiftState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.id) &&
    isFiniteNumber(value.targetPlatform) &&
    (value.state === "raised" ||
      value.state === "lowering" ||
      value.state === "waiting" ||
      value.state === "raising") &&
    isSafeNonNegativeInteger(value.motionElapsedTicks) &&
    isSafeNonNegativeInteger(value.waitElapsedTicks) &&
    typeof value.prompt === "string" &&
    typeof value.presentation === "string" &&
    typeof value.source === "string"
  );
}

function isRuntimeLevelExitState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.id) &&
    (value.state === "available" || value.state === "completed") &&
    isFiniteNumber(value.activationRadius) &&
    typeof value.presentation === "string" &&
    (value.completedBy === null || isFiniteNumber(value.completedBy)) &&
    (value.completedAtTick === null || isFiniteNumber(value.completedAtTick))
  );
}

function isRuntimeInteractionState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.target) &&
    typeof value.prompt === "string"
  );
}

function isRuntimeEnemyState(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.id) &&
    typeof value.name === "string" &&
    (value.state === "alive" || value.state === "defeated") &&
    isFiniteVector3(value.position) &&
    isFiniteNumber(value.currentHealth) &&
    isFiniteNumber(value.maxHealth) &&
    (value.combatPosture === null ||
      value.combatPosture === "sleeping" ||
      value.combatPosture === "alert" ||
      value.combatPosture === "pursuing" ||
      value.combatPosture === "attacking" ||
      value.combatPosture === "dead") &&
    (value.attackKind === null ||
      value.attackKind === "melee" ||
      value.attackKind === "rangedHitscan" ||
      value.attackKind === "projectile")
  );
}

function isSafeNonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isFiniteVector3(
  value: unknown,
): value is readonly [number, number, number] {
  return (
    Array.isArray(value) && value.length === 3 && value.every(isFiniteNumber)
  );
}

function isFiniteVector4(
  value: unknown,
): value is readonly [number, number, number, number] {
  return (
    Array.isArray(value) && value.length === 4 && value.every(isFiniteNumber)
  );
}

function isRuntimeDynamicState(value: unknown): value is RuntimeDynamicState {
  return (
    isRecord(value) &&
    isFiniteNumber(value.tick) &&
    isFiniteNumber(value.entityRevision) &&
    isRecord(value.gameplayFrame) &&
    Array.isArray(value.projection) &&
    value.projection.every(isRuntimeProjectionNode) &&
    typeof value.doorState === "string" &&
    typeof value.encounterState === "string" &&
    typeof value.motionState === "string" &&
    typeof value.navigationState === "string" &&
    typeof value.playerMotionState === "string" &&
    typeof value.combatState === "string" &&
    isRecord(value.input) &&
    isRuntimePlayerState(value.player) &&
    isRuntimeWeaponState(value.weapon) &&
    (value.inventory === null || isRuntimeInventoryState(value.inventory)) &&
    Array.isArray(value.pickups) &&
    Array.isArray(value.hazards) &&
    value.hazards.every(isRuntimeHazardState) &&
    isRecord(value.restart) &&
    typeof value.restart.authoredBaselineAvailable === "boolean" &&
    typeof value.restart.checkpointAvailable === "boolean" &&
    Array.isArray(value.saveSlots) &&
    value.saveSlots.every(isRuntimeSaveSlotSummary) &&
    (value.extractionBeacon === null || isRecord(value.extractionBeacon)) &&
    Array.isArray(value.doorAccess) &&
    value.doorAccess.every(isRuntimeDoorAccessState) &&
    Array.isArray(value.secretRegions) &&
    value.secretRegions.every(isRuntimeSecretRegionState) &&
    Array.isArray(value.floorActions) &&
    value.floorActions.every(isRuntimeFloorActionState) &&
    Array.isArray(value.lifts) &&
    value.lifts.every(isRuntimeLiftState) &&
    Array.isArray(value.levelExits) &&
    value.levelExits.every(isRuntimeLevelExitState) &&
    typeof value.levelComplete === "boolean" &&
    (value.interaction === null ||
      isRuntimeInteractionState(value.interaction)) &&
    Array.isArray(value.enemies) &&
    value.enemies.every(isRuntimeEnemyState) &&
    isRecord(value.presentation) &&
    Array.isArray(value.lastEvents) &&
    value.lastEvents.every((event) => typeof event === "string")
  );
}

function isRuntimeProjectionNode(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.id) &&
    typeof value.name === "string" &&
    typeof value.asset === "string" &&
    (value.translation === null || isFiniteVector3(value.translation)) &&
    isFiniteVector4(value.rotation) &&
    isFiniteVector3(value.scale) &&
    typeof value.visible === "boolean" &&
    isRuntimeBoundVisualState(value.visualState)
  );
}

function isRuntimeBoundVisualState(value: unknown): boolean {
  return (
    typeof value === "string" &&
    [
      "default",
      "open",
      "opening",
      "closed",
      "closing",
      "active",
      "inactive",
      "standby",
      "available",
      "dormant",
      "collected",
      "cooling",
      "completed",
      "idle",
      "moving",
      "alert",
      "attacking",
      "hit",
      "defeated",
    ].includes(value)
  );
}

function isRuntimeSaveSlotSummary(value: unknown): boolean {
  return (
    isRecord(value) &&
    (value.slot === "checkpoint" ||
      value.slot === "slot1" ||
      value.slot === "slot2" ||
      value.slot === "slot3") &&
    (value.compatibility === "empty" ||
      value.compatibility === "available" ||
      value.compatibility === "corrupt" ||
      value.compatibility === "incompatible") &&
    (value.storageRevision === null ||
      typeof value.storageRevision === "string") &&
    (value.metadata === null || isRuntimeSaveGameMetadata(value.metadata)) &&
    (value.project === null || isRuntimeSaveProjectIdentity(value.project)) &&
    (value.diagnostic === null || typeof value.diagnostic === "string")
  );
}

function isRuntimeSaveGameMetadata(value: unknown): boolean {
  return (
    isRecord(value) &&
    isFiniteNumber(value.revision) &&
    isFiniteNumber(value.savedAtUnixMilliseconds) &&
    typeof value.displayName === "string" &&
    isFiniteNumber(value.tick) &&
    isFiniteNumber(value.snapshotSchemaVersion) &&
    (value.playerState === "alive" ||
      value.playerState === "dead" ||
      value.playerState === "unavailable") &&
    typeof value.levelComplete === "boolean"
  );
}

function isRuntimeSaveProjectIdentity(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.projectId === "string" &&
    typeof value.entryScene === "string" &&
    isFiniteNumber(value.playerEntity) &&
    isFiniteNumber(value.projectSchemaVersion) &&
    typeof value.contentRevision === "string"
  );
}

function replacesSession(command: ClientGameCommand): boolean {
  return command.kind === "restart" || command.kind === "loadGame";
}

export function coalesceSessionLook(
  current: readonly [number, number],
  incoming: readonly [number, number],
): [number, number] {
  return [
    clampLook(current[0] + clampLook(incoming[0])),
    clampLook(current[1] + clampLook(incoming[1])),
  ];
}

function clampLook(value: number): number {
  return Math.max(
    -MAX_SESSION_LOOK_UNITS,
    Math.min(MAX_SESSION_LOOK_UNITS, value),
  );
}

export function preserveMovementPress(
  previous: readonly [number, number],
  next: readonly [number, number],
  pending: readonly [number, number],
): [number, number] {
  return [0, 1].map((axis) =>
    previous[axis] === 0 && next[axis] !== 0 ? next[axis] : pending[axis],
  ) as [number, number];
}

export function resolveMovementSample(
  latest: readonly [number, number],
  pendingPress: readonly [number, number],
): {
  readonly movement: [number, number];
  readonly mustFollowWithLatestMovement: boolean;
} {
  const movement: [number, number] = [
    latest[0] || pendingPress[0],
    latest[1] || pendingPress[1],
  ];
  return {
    movement,
    mustFollowWithLatestMovement:
      movement[0] !== latest[0] || movement[1] !== latest[1],
  };
}

function runtimeStateResources(
  resources: RuntimeStaticResources,
): Pick<RuntimeBrowserState, StaticStateKey> {
  return {
    hostSessionId: resources.hostSessionId,
    projectId: resources.projectId,
    voxelRevision: resources.voxelRevision,
    voxelAuthorityHash: resources.voxelAuthorityHash,
    voxelSolidCount: resources.voxelSolidCount,
    voxelNavigationHash: resources.voxelNavigationHash,
    voxelProbePathLength: resources.voxelProbePathLength,
    voxelEnvironmentRole: resources.voxelEnvironmentRole,
    voxelMeshes: resources.voxelMeshes,
    voxelObjectFrame: resources.voxelObjectFrame,
    lights: resources.lights,
    renderMaterials: resources.renderMaterials,
    staticMeshes: resources.staticMeshes,
    animatedMeshes: resources.animatedMeshes,
    visualBindings: resources.visualBindings,
    generatedEnvironment: resources.generatedEnvironment,
    ...(resources.applicationContent === undefined
      ? {}
      : { applicationContent: resources.applicationContent }),
  };
}
