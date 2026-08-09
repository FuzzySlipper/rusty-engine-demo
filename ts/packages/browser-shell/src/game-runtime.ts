import { GameSessionError, LoadingBayGameSession } from "./game-session.js";
import type {
  RuntimeBrowserState,
  RuntimeSaveSlotId,
  RuntimeSaveSlotSummary,
} from "./projection.js";

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

export interface LoadingBayGameOptions {
  readonly onProjection?: (snapshot: LoadingBayPresentationSnapshot) => void;
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
 * Mount the browser control shell around the Rust gameplay session.
 *
 * Rendering deliberately does not happen here. The visible retained viewport
 * is owned by the Engine Rust webview adapter and is launched with
 * `pnpm run native`; this shell remains useful for accessible controls,
 * project/save inspection, and browser-host diagnostics.
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

  markNativeViewportBoundary();
  publish(current);
  session.setStateListener((state) => {
    current = state;
    publish(state);
  });
  session.setFailureListener((error) => {
    lastRejection = error.message;
    publish(current);
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

  return {
    captureAnimation: () => {
      throw new Error(
        "animated renderer capture is owned by the native Engine host",
      );
    },
    dispose: async () => {
      if (disposed) return;
      disposed = true;
      controller.abort();
      pressed.clear();
      session.neutralizeInput();
      await session.close();
      document.body.dataset.rendererLifecycle = "native-host";
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

  function publish(state: RuntimeBrowserState): void {
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

  function onKeyDown(event: KeyboardEvent): void {
    if (event.repeat || keyboardTargetOwnsInput(event.target)) return;
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
    queueMovement();
  }

  function onMouseMove(event: MouseEvent): void {
    if (document.pointerLockElement === null) return;
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
    if (event.button !== 0 || keyboardTargetOwnsInput(event.target)) return;
    const viewport = document.getElementById("viewport");
    if (
      viewport instanceof HTMLElement &&
      document.pointerLockElement === null
    ) {
      void viewport.requestPointerLock();
    }
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

function markNativeViewportBoundary(): void {
  document.body.dataset.rendererLifecycle = "native-host";
  const viewport = document.getElementById("viewport");
  if (viewport instanceof HTMLElement) {
    viewport.dataset.rendererBackend = "rusty-engine-native-webview";
    viewport.dataset.rendererOwner = "rust";
    viewport.setAttribute(
      "aria-label",
      "Engine-owned native viewport; launch pnpm run native for rendered play",
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
