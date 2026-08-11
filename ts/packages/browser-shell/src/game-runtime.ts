import { GameSessionError, LoadingBayGameSession } from "./game-session.js";
import { HeldMovementInput } from "./held-movement.js";
import { resolveKeyboardAction } from "./input-resolver.js";
import { resolvePointerLook } from "./pointer-look.js";
import type {
  RuntimeApplicationContent,
  RuntimeBrowserState,
  RuntimeSaveSlotId,
  RuntimeSaveSlotSummary,
} from "./projection.js";
import { derivePlayerCameraPose } from "./projection.js";

export interface LoadingBayHostPresentationPreferences {
  readonly mouseSensitivity: number;
  readonly invertY: boolean;
  readonly sfxVolume: number;
  readonly flashIntensity: number;
  readonly telemetryVisible: boolean;
}

export type LoadingBaySaveSlotId = RuntimeSaveSlotId;
export type LoadingBaySaveSlot = RuntimeSaveSlotSummary;

export interface LoadingBayInventoryStack {
  readonly item: string;
  readonly quantity: number;
}

export interface LoadingBayWeaponSlot {
  readonly slot: number;
  readonly item: string;
  readonly owned: boolean;
  readonly selected: boolean;
  readonly ammunition: string;
  readonly ammunitionQuantity: number;
}

export interface LoadingBayInputBindings {
  readonly moveForward: string;
  readonly moveBackward: string;
  readonly moveLeft: string;
  readonly moveRight: string;
  readonly mouseLook: string;
  readonly primaryFire: string;
  readonly jump?: string | null;
  readonly selectWeapon: readonly string[];
}

export interface LoadingBayPresentationSnapshot {
  readonly ammoCapacity: number;
  readonly ammoRemaining: number;
  readonly armor: number;
  readonly bindings: LoadingBayInputBindings;
  readonly connected: boolean;
  readonly doorState: "closed" | "opening" | "open" | "closing";
  readonly equippedWeapon: string | null;
  readonly encounterState: string;
  readonly events: readonly string[];
  readonly health: number;
  readonly headingDegrees: number;
  readonly hostSessionId: string;
  readonly projectId: string;
  readonly interactionPrompt: string | null;
  readonly interactionTarget: number | null;
  readonly inventoryCapacity: number;
  readonly inventoryStacks: readonly LoadingBayInventoryStack[];
  readonly lastRejection: string | null;
  readonly maxArmor: number;
  readonly maxHealth: number;
  readonly paused: boolean;
  readonly levelComplete: boolean;
  readonly levelCompletionPresentation: string | null;
  readonly restartAvailable: boolean;
  readonly saveSlots: readonly LoadingBaySaveSlot[];
  readonly vitalityState: "alive" | "dead";
  readonly weaponItem: string;
  readonly weaponPresentation: string;
  readonly weaponSlots: readonly LoadingBayWeaponSlot[];
}

export interface LoadingBayRenderProjection {
  readonly camera: {
    readonly position: readonly [number, number, number];
    readonly pitchDegrees: number;
    readonly yawDegrees: number;
  };
  readonly frame: Readonly<Record<string, unknown>>;
  readonly content: LoadingBayApplicationContent | null;
  readonly replaceFrame: boolean;
}

export interface LoadingBayApplicationContent {
  readonly frame: Readonly<Record<string, unknown>>;
  readonly resources: readonly {
    readonly identity: string;
    readonly contentHash: string;
    readonly mediaType: string;
    readonly bytes: Uint8Array;
  }[];
}

export interface LoadingBayGameOptions {
  readonly inputEnabled?: (event: Event) => boolean;
  readonly onProjection?: (snapshot: LoadingBayPresentationSnapshot) => void;
  readonly onRenderProjection?: (
    rendering: LoadingBayRenderProjection,
  ) => void | Promise<void>;
  readonly onConnectionFailure?: (message: string) => void;
  readonly preferences?: LoadingBayHostPresentationPreferences;
}

export interface LoadingBayGameHandle {
  readonly captureAnimation: (request: unknown) => never;
  readonly dispose: () => Promise<void>;
  readonly interact: (target: number) => Promise<void>;
  readonly loadGame: (
    slot: LoadingBaySaveSlotId,
    expectedStorageRevision: string | null,
  ) => Promise<void>;
  readonly releaseInput: () => void;
  readonly restart: () => Promise<void>;
  readonly saveGame: (
    slot: LoadingBaySaveSlotId,
    overwrite: boolean,
    expectedStorageRevision: string | null,
  ) => Promise<void>;
  readonly selectWeaponSlot: (slot: number) => Promise<void>;
  readonly setPaused: (paused: boolean) => Promise<void>;
  readonly updatePreferences: (
    preferences: LoadingBayHostPresentationPreferences,
  ) => void;
  readonly useItem: (item: string) => Promise<void>;
}

/**
 * Mount the downstream control and transport owner around the Rust gameplay
 * session. Rust projects every renderer-neutral frame and camera pose; the
 * application bootstrap hands those values to Engine's bounded host port.
 */
export async function mountLoadingBayGame(
  options: LoadingBayGameOptions = {},
): Promise<LoadingBayGameHandle> {
  const controller = new AbortController();
  let preferences = options.preferences ?? defaultPreferences();
  let lastRejection: string | null = null;
  let disposed = false;
  const session = await LoadingBayGameSession.connect();
  let current = session.state;
  let primaryFireHeld = false;
  let primaryFireDriver: ReturnType<typeof globalThis.setInterval> | null = null;
  const heldMovement = new HeldMovementInput({
    bindings: () => current.player.bindings,
    intervalMilliseconds: () => 16,
    dispatch: async (action) => {
      session.queueInput({
        movement: [action.forward, action.right],
        lookDelta: [0, 0],
        primaryFireHeld,
      });
    },
  });
  let lastRenderFrame: Readonly<Record<string, unknown>> | null = null;
  let lastApplicationContent: RuntimeApplicationContent | null = null;
  let projectionQueue: Promise<void> = Promise.resolve();

  try {
    markApplicationViewportBoundary();
    await publish(current);
    session.setStateListener((state) => {
      current = state;
      projectionQueue = projectionQueue
        .then(() => publish(state))
        .catch(recordProjectionFailure);
    });
    session.setFailureListener((error) => {
      lastRejection = error.message;
      projectionQueue = projectionQueue
        .then(() => publish(current))
        .catch(recordProjectionFailure);
      if (error.retry === "reconnect") {
        options.onConnectionFailure?.(error.message);
      }
    });

    globalThis.addEventListener("keydown", onKeyDown, {
      signal: controller.signal,
    });
    globalThis.addEventListener("keyup", onKeyUp, {
      signal: controller.signal,
    });
    globalThis.addEventListener("mousemove", onMouseMove, {
      signal: controller.signal,
    });
    globalThis.addEventListener("mousedown", onMouseDown, {
      signal: controller.signal,
    });
    globalThis.addEventListener("mouseup", onMouseUp, {
      signal: controller.signal,
    });
    globalThis.addEventListener("blur", onInputLoss, {
      signal: controller.signal,
    });
    document.addEventListener("pointerlockchange", onPointerLockChange, {
      signal: controller.signal,
    });
  } catch (cause) {
    controller.abort();
    session.neutralizeInput();
    await session.close();
    throw cause;
  }

  return {
    captureAnimation: () => {
      throw new Error(
        "animated renderer capture is not exposed by the Engine application host",
      );
    },
    dispose: async () => {
      if (disposed) return;
      disposed = true;
      controller.abort();
      heldMovement.clear(false);
      stopPrimaryFire();
      session.neutralizeInput();
      await projectionQueue.catch(() => undefined);
      await session.close();
    },
    interact: async (target) => {
      await session.sendEdge({ kind: "interact", target });
    },
    loadGame: async (slot, expectedStorageRevision) => {
      await session.sendEdge({
        kind: "loadGame",
        slot,
        expectedStorageRevision,
      });
    },
    releaseInput: () => {
      heldMovement.clear(false);
      stopPrimaryFire();
      session.neutralizeInput();
    },
    restart: async () => {
      await session.sendEdge({ kind: "restart", mode: "authoredBaseline" });
    },
    saveGame: async (slot, overwrite, expectedStorageRevision) => {
      await session.sendEdge({
        kind: "saveGame",
        slot,
        overwrite,
        expectedStorageRevision,
      });
    },
    selectWeaponSlot: async (slot) => {
      await session.sendEdge({ kind: "selectWeaponSlot", slot });
    },
    setPaused: async (paused) => {
      await session.sendEdge({ kind: "setPaused", paused });
    },
    updatePreferences: (next) => {
      preferences = next;
    },
    useItem: async (item) => {
      await session.sendEdge({ kind: "useItem", item });
    },
  };

  async function publish(state: RuntimeBrowserState): Promise<void> {
    const descriptor = state.applicationContent ?? null;
    const content =
      descriptor !== null && descriptor !== lastApplicationContent
        ? await loadApplicationContent(descriptor)
        : null;
    const replaceFrame =
      descriptor === null && content === null && lastRenderFrame !== state.voxelObjectFrame;
    const frame = descriptor === null ? state.voxelObjectFrame : state.gameplayFrame;
    await options.onRenderProjection?.({
      camera: derivePlayerCameraPose(state.player),
      content,
      frame,
      replaceFrame,
    });
    if (content !== null) {
      lastApplicationContent = descriptor;
      lastRenderFrame = state.voxelObjectFrame;
    }
    if (replaceFrame) lastRenderFrame = state.voxelObjectFrame;
    const completedExit = state.levelExits.find(
      (candidate) => candidate.state === "completed",
    );
    options.onProjection?.({
      ammoCapacity: state.weapon.ammoCapacity,
      ammoRemaining: state.weapon.ammoRemaining,
      armor: state.player.armor,
      bindings: state.player.bindings,
      connected: state.input.connected,
      doorState: state.doorState,
      equippedWeapon: state.inventory?.equippedWeapon ?? null,
      encounterState: state.encounterState,
      events: state.lastEvents,
      health: state.player.currentHealth,
      headingDegrees: normalizeDegrees(state.player.yawDegrees),
      hostSessionId: state.hostSessionId,
      projectId: state.projectId,
      interactionPrompt: state.interaction?.prompt ?? null,
      interactionTarget: state.interaction?.target ?? null,
      inventoryCapacity: state.inventory?.capacitySlots ?? 0,
      inventoryStacks: state.inventory?.stacks ?? [],
      lastRejection,
      maxArmor: state.player.maxArmor,
      maxHealth: state.player.maxHealth,
      paused: state.input.paused,
      levelComplete: state.levelComplete,
      levelCompletionPresentation: completedExit?.presentation ?? null,
      restartAvailable: state.restart.authoredBaselineAvailable,
      saveSlots: state.saveSlots,
      vitalityState: state.player.vitalityState,
      weaponItem: state.weapon.item,
      weaponPresentation: state.weapon.presentation,
      weaponSlots: state.inventory?.weapons ?? [],
    });
  }

  function recordProjectionFailure(cause: unknown): void {
    if (disposed) return;
    const message = cause instanceof Error ? cause.message : String(cause);
    lastRejection = message;
    options.onConnectionFailure?.(message);
    void session.close();
  }

  function onKeyDown(event: KeyboardEvent): void {
    if (
      event.repeat ||
      keyboardTargetOwnsInput(event.target) ||
      options.inputEnabled?.(event) === false
    )
      return;
    heldMovement.press(event.code);
    const action = resolveKeyboardAction(event.code, current.player.bindings);
    if (action?.kind === "jump") {
      void session.sendEdge({ kind: "jump" }).catch(record);
    }
    if (action?.kind === "selectWeaponSlot") {
      void session
        .sendEdge({ kind: "selectWeaponSlot", slot: action.slot })
        .catch(record);
    }
  }

  function onKeyUp(event: KeyboardEvent): void {
    heldMovement.release(event.code);
    if (options.inputEnabled?.(event) === false) {
      stopPrimaryFire();
      session.neutralizeInput();
    }
  }

  function onMouseMove(event: MouseEvent): void {
    if (
      document.pointerLockElement === null ||
      options.inputEnabled?.(event) === false
    )
      return;
    const lookDelta = resolvePointerLook(
      event.movementX,
      event.movementY,
      preferences,
    );
    session.queueInput({
      movement: movement(),
      lookDelta,
      primaryFireHeld,
    });
  }

  function onMouseDown(event: MouseEvent): void {
    if (
      event.button !== 0 ||
      keyboardTargetOwnsInput(event.target) ||
      options.inputEnabled?.(event) === false
    )
      return;
    if (primaryFireHeld) return;
    primaryFireHeld = true;
    primaryFireDriver = globalThis.setInterval(() => {
      session.queueInput({
        movement: movement(),
        lookDelta: [0, 0],
        primaryFireHeld,
      });
    }, 16);
    void session
      .sendInput({
        movement: movement(),
        lookDelta: [0, 0],
        primaryFireHeld,
      })
      .catch(record);
  }

  function onMouseUp(event: MouseEvent): void {
    if (event.button !== 0 || !primaryFireHeld) return;
    stopPrimaryFire();
    void session
      .sendInput({
        movement: movement(),
        lookDelta: [0, 0],
        primaryFireHeld,
      })
      .catch(record);
  }

  function onInputLoss(): void {
    heldMovement.clear(false);
    stopPrimaryFire();
    session.neutralizeInput();
  }

  function onPointerLockChange(): void {
    if (document.pointerLockElement === null) onInputLoss();
  }

  function stopPrimaryFire(): void {
    primaryFireHeld = false;
    if (primaryFireDriver === null) return;
    globalThis.clearInterval(primaryFireDriver);
    primaryFireDriver = null;
  }

  function movement(): readonly [number, number] {
    const action = heldMovement.current;
    return [action.forward, action.right];
  }

  function record(error: unknown): void {
    lastRejection = error instanceof Error ? error.message : String(error);
    publish(current);
  }
}

async function loadApplicationContent(
  descriptor: RuntimeApplicationContent,
): Promise<LoadingBayApplicationContent> {
  const resources = await Promise.all(
    descriptor.resources.map(async (resource) => {
      const response = await fetch(resource.resourceUrl, { cache: "no-store" });
      if (!response.ok) {
        throw new Error(
          `Rust application resource ${resource.identity} returned HTTP ${String(response.status)}`,
        );
      }
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (bytes.byteLength !== resource.byteLength) {
        throw new Error(
          `Rust application resource ${resource.identity} declared ${String(resource.byteLength)} bytes but returned ${String(bytes.byteLength)}`,
        );
      }
      return {
        identity: resource.identity,
        contentHash: resource.contentHash,
        mediaType: resource.mediaType,
        bytes,
      };
    }),
  );
  return { frame: descriptor.frame, resources };
}

function markApplicationViewportBoundary(): void {
  document.body.dataset.rendererLifecycle = "mounted";
  const viewport = document.getElementById("viewport");
  if (viewport instanceof HTMLElement) {
    viewport.dataset.rendererBackend = "rusty-engine-application-host";
    viewport.dataset.rendererOwner = "engine";
    viewport.setAttribute(
      "aria-label",
      "Engine-owned rendered viewport. Click to capture gameplay input.",
    );
  }
}

function keyboardTargetOwnsInput(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

function normalizeDegrees(value: number): number {
  return ((value % 360) + 360) % 360;
}

function defaultPreferences(): LoadingBayHostPresentationPreferences {
  return {
    mouseSensitivity: 1,
    invertY: false,
    sfxVolume: 1,
    flashIntensity: 1,
    telemetryVisible: false,
  };
}

export { GameSessionError };
