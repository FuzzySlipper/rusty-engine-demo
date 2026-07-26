export interface DocumentEffectsPort {
  readonly setRootClass: (className: string, enabled: boolean) => void;
  readonly setTitle: (title: string) => void;
}

export interface HostUserSettings {
  readonly mouseSensitivity: number;
  readonly invertY: boolean;
  readonly sfxVolume: number;
  readonly hudVisible: boolean;
  readonly telemetryVisible: boolean;
}

export interface HostUserSettingsRepository {
  readonly read: () => HostUserSettings;
  readonly write: (settings: HostUserSettings) => HostUserSettings;
  readonly hasContinueSession: (hostSessionId: string) => boolean;
  readonly markContinueSessionAvailable: (hostSessionId: string) => void;
}

export interface StringStoragePort {
  readonly getItem: (key: string) => string | null;
  readonly setItem: (key: string, value: string) => void;
}

export const DEFAULT_HOST_USER_SETTINGS: HostUserSettings = {
  mouseSensitivity: 1,
  invertY: false,
  sfxVolume: 0.8,
  hudVisible: true,
  telemetryVisible: false,
};

export const HOST_USER_SETTINGS_STORAGE_KEY =
  "rusty-engine-demo.host-user-settings.v1";
export const CONTINUE_SESSION_STORAGE_KEY =
  "rusty-engine-demo.continue-session.v1";

export const browserDocumentEffects = (): DocumentEffectsPort => ({
  setRootClass: (className, enabled) => {
    document.documentElement.classList.toggle(className, enabled);
  },
  setTitle: (title) => {
    document.title = title;
  },
});

export function normalizeHostUserSettings(value: unknown): HostUserSettings {
  if (!isRecord(value)) {
    return DEFAULT_HOST_USER_SETTINGS;
  }
  return {
    mouseSensitivity: boundedNumber(
      value.mouseSensitivity,
      0.25,
      2,
      DEFAULT_HOST_USER_SETTINGS.mouseSensitivity,
    ),
    invertY: booleanOr(value.invertY, DEFAULT_HOST_USER_SETTINGS.invertY),
    sfxVolume: boundedNumber(
      value.sfxVolume,
      0,
      1,
      DEFAULT_HOST_USER_SETTINGS.sfxVolume,
    ),
    hudVisible: booleanOr(
      value.hudVisible,
      DEFAULT_HOST_USER_SETTINGS.hudVisible,
    ),
    telemetryVisible: booleanOr(
      value.telemetryVisible,
      DEFAULT_HOST_USER_SETTINGS.telemetryVisible,
    ),
  };
}

export function hostUserSettingsRepository(
  storage: StringStoragePort,
): HostUserSettingsRepository {
  return {
    read: () => {
      try {
        const stored = storage.getItem(HOST_USER_SETTINGS_STORAGE_KEY);
        return stored === null
          ? DEFAULT_HOST_USER_SETTINGS
          : normalizeHostUserSettings(JSON.parse(stored));
      } catch {
        return DEFAULT_HOST_USER_SETTINGS;
      }
    },
    write: (settings) => {
      const normalized = normalizeHostUserSettings(settings);
      try {
        storage.setItem(
          HOST_USER_SETTINGS_STORAGE_KEY,
          JSON.stringify(normalized),
        );
      } catch {
        // A denied or full host store must not prevent settings from applying
        // to the current disposable browser presentation.
      }
      return normalized;
    },
    hasContinueSession: (hostSessionId) => {
      try {
        return (
          hostSessionId.length > 0 &&
          storage.getItem(CONTINUE_SESSION_STORAGE_KEY) === hostSessionId
        );
      } catch {
        return false;
      }
    },
    markContinueSessionAvailable: (hostSessionId) => {
      if (hostSessionId.length === 0) {
        return;
      }
      try {
        storage.setItem(CONTINUE_SESSION_STORAGE_KEY, hostSessionId);
      } catch {
        // The current game remains usable when durable host preferences fail.
      }
    },
  };
}

export function browserHostUserSettingsRepository(): HostUserSettingsRepository {
  return hostUserSettingsRepository({
    getItem: (key) => globalThis.localStorage.getItem(key),
    setItem: (key, value) => {
      globalThis.localStorage.setItem(key, value);
    },
  });
}

function boundedNumber(
  value: unknown,
  minimum: number,
  maximum: number,
  fallback: number,
): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(maximum, Math.max(minimum, value))
    : fallback;
}

function booleanOr(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
