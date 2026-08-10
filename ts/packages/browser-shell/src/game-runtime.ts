import { GameSessionError, LoadingBayGameSession } from "./game-session.js";
import type {
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
  readonly selectWeapon: readonly string[];
}

export interface LoadingBayPresentationSnapshot {
  readonly ammoCapacity: number;
  readonly ammoRemaining: number;
  readonly armor: number;
  readonly bindings: LoadingBayInputBindings;
  readonly connected: boolean;
  readonly doorState: "closed" | "open";
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
  readonly replaceFrame: boolean;
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
  const pressed = new Set<string>();
  let preferences = options.preferences ?? defaultPreferences();
  let lastRejection: string | null = null;
  let disposed = false;
  const session = await LoadingBayGameSession.connect();
  let current = session.state;
  let lastRenderFrame: Readonly<Record<string, unknown>> | null = null;
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
      pressed.clear();
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
      pressed.clear();
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
    const replaceFrame = lastRenderFrame !== state.voxelObjectFrame;
    await options.onRenderProjection?.({
      camera: derivePlayerCameraPose(state.player),
      frame: state.voxelObjectFrame,
      replaceFrame,
    });
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
    pressed.add(event.code);
    queueMovement();
    const slot = current.player.bindings.selectWeapon.indexOf(event.code);
    if (slot >= 0) {
      void session
        .sendEdge({ kind: "selectWeaponSlot", slot: slot + 1 })
        .catch(record);
    }
  }

  function onKeyUp(event: KeyboardEvent): void {
    pressed.delete(event.code);
    if (options.inputEnabled?.(event) === false) {
      session.neutralizeInput();
      return;
    }
    queueMovement();
  }

  function onMouseMove(event: MouseEvent): void {
    if (
      document.pointerLockElement === null ||
      options.inputEnabled?.(event) === false
    )
      return;
    const invert = preferences.invertY ? -1 : 1;
    session.queueInput({
      movement: movement(),
      lookDelta: [
        clamp(event.movementX * preferences.mouseSensitivity * 0.01),
        clamp(event.movementY * preferences.mouseSensitivity * 0.01 * invert),
      ],
      primaryFireHeld: false,
    });
  }

  function onMouseDown(event: MouseEvent): void {
    if (
      event.button !== 0 ||
      keyboardTargetOwnsInput(event.target) ||
      options.inputEnabled?.(event) === false
    )
      return;
    void session
      .sendInput({
        movement: movement(),
        lookDelta: [0, 0],
        primaryFireHeld: true,
      })
      .then(() => queueMovement())
      .catch(record);
  }

  function queueMovement(): void {
    session.queueInput({
      movement: movement(),
      lookDelta: [0, 0],
      primaryFireHeld: false,
    });
  }

  function movement(): readonly [number, number] {
    const bindings = current.player.bindings;
    const forward =
      Number(pressed.has(bindings.moveForward)) -
      Number(pressed.has(bindings.moveBackward));
    const right =
      Number(pressed.has(bindings.moveRight)) -
      Number(pressed.has(bindings.moveLeft));
    return [forward, right];
  }

  function record(error: unknown): void {
    lastRejection = error instanceof Error ? error.message : String(error);
    publish(current);
  }
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

function clamp(value: number): number {
  return Math.max(-1, Math.min(1, value));
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
