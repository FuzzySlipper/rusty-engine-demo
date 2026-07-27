import assert from "node:assert/strict";
import test from "node:test";

import {
  CONTINUE_SESSION_STORAGE_KEY,
  DEFAULT_HOST_USER_SETTINGS,
  HOST_USER_SETTINGS_STORAGE_KEY,
  hostUserSettingsRepository,
  normalizeHostUserSettings,
  type StringStoragePort,
} from "./index.ts";

test("host-user settings clamp numeric input and default malformed fields", () => {
  assert.deepEqual(
    normalizeHostUserSettings({
      mouseSensitivity: 99,
      invertY: true,
      sfxVolume: -4,
      flashIntensity: 9,
      hudVisible: "yes",
      telemetryVisible: true,
    }),
    {
      mouseSensitivity: 2,
      invertY: true,
      sfxVolume: 0,
      flashIntensity: 1,
      hudVisible: true,
      telemetryVisible: true,
    },
  );
  assert.deepEqual(normalizeHostUserSettings(null), DEFAULT_HOST_USER_SETTINGS);
});

test("host-user repository persists presentation preferences and session availability", () => {
  const values = new Map<string, string>();
  const storage: StringStoragePort = {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => {
      values.set(key, value);
    },
  };
  const repository = hostUserSettingsRepository(storage);

  const written = repository.write({
    mouseSensitivity: 1.5,
    invertY: true,
    sfxVolume: 0.4,
    flashIntensity: 0.25,
    hudVisible: false,
    telemetryVisible: true,
  });
  assert.deepEqual(repository.read(), written);
  assert.equal(values.has(HOST_USER_SETTINGS_STORAGE_KEY), true);
  assert.equal(repository.hasContinueSession("host-a"), false);
  repository.markContinueSessionAvailable("host-a");
  assert.equal(values.get(CONTINUE_SESSION_STORAGE_KEY), "host-a");
  assert.equal(repository.hasContinueSession("host-a"), true);
  assert.equal(repository.hasContinueSession("host-b"), false);
});

test("host-user repository fails safe when host storage is unavailable", () => {
  const storage: StringStoragePort = {
    getItem: () => {
      throw new Error("storage denied");
    },
    setItem: () => {
      throw new Error("storage denied");
    },
  };
  const repository = hostUserSettingsRepository(storage);

  assert.deepEqual(repository.read(), DEFAULT_HOST_USER_SETTINGS);
  assert.equal(repository.hasContinueSession("host-a"), false);
  assert.doesNotThrow(() =>
    repository.write({
      ...DEFAULT_HOST_USER_SETTINGS,
      invertY: true,
    }),
  );
  assert.doesNotThrow(() => repository.markContinueSessionAvailable("host-a"));
});
