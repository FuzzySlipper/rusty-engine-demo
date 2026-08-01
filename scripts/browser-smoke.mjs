import { spawn } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const chromium = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
const FULL_CAMPAIGN_TIMEOUT_MILLISECONDS = 300_000;
const CONVERTED_CAMPAIGN_TIMEOUT_MILLISECONDS = 90_000;
const CHROMIUM_STARTUP_TIMEOUT_MILLISECONDS = 45_000;
const convertedOnly = process.env.RUSTY_BROWSER_SMOKE_PHASE === "converted";
if (!existsSync(chromium)) {
  throw new Error(
    `Chromium is required for the product smoke (${chromium} not found)`,
  );
}

const bundleDirectory = resolve(repoRoot, "dist/apps/loading-bay/browser");
const browserBundle = readdirSync(bundleDirectory)
  .filter((name) => name.endsWith(".js"))
  .map((name) => readFileSync(resolve(bundleDirectory, name), "utf8"))
  .join("\n");
const forbiddenRuntimeSurface = [
  ["Gameplay", "RuntimeHost"].join(""),
  ["Gameplay", "Fabric"].join(""),
  ["Native", "RuntimeBridge"].join(""),
  ["Runtime", "Session"].join(""),
  "VoxelConversionRequest",
  "rusty-engine.mesh-to-voxel",
  "voxel-convert",
  "planVoxelConversion",
  "previewVoxelConversion",
  "applyVoxelConversion",
  ["VoxelReplay", "Record"].join(""),
  "GenericAssetProvider",
  "ProjectBundleFacade",
];
const bundledRuntimeSurface = forbiddenRuntimeSurface.filter((name) =>
  browserBundle.includes(name),
);
if (bundledRuntimeSurface.length > 0) {
  throw new Error(
    `browser bundle imported old runtime surface: ${bundledRuntimeSurface.join(", ")}`,
  );
}

const proofDirectory = mkdtempSync(
  join(tmpdir(), "rusty-engine-browser-smoke-"),
);
try {
  const persistedProject = resolve(proofDirectory, "loading-bay.project.json");
  const convertedProject = resolve(
    proofDirectory,
    "converted-wall.project.json",
  );
  const migratedProject = resolve(proofDirectory, "migrated-v6.project.json");
  if (!convertedOnly) {
    const currentReceipt = await persistProject(
      resolve(repoRoot, "content/projects/loading-bay.project.json"),
      persistedProject,
    );
    if (
      !currentReceipt.includes("sourceSchema=24") ||
      !currentReceipt.includes("currentSchema=24")
    ) {
      throw new Error(
        `current project persistence receipt was incomplete\n${currentReceipt}`,
      );
    }
    cpSync(
      resolve(repoRoot, "content/assets/actor-kit"),
      resolve(proofDirectory, "content/assets/actor-kit"),
      { recursive: true },
    );
    await runFullBrowserProduct(persistedProject);
    await runPersistedVoxelEditProduct(persistedProject);
  }

  const convertedReceipt = await persistProject(
    resolve(repoRoot, "content/projects/converted-wall.project.json"),
    convertedProject,
  );
  if (
    !convertedReceipt.includes("sourceSchema=11") ||
    !convertedReceipt.includes("currentSchema=24")
  ) {
    throw new Error(
      `converted project persistence receipt was incomplete\n${convertedReceipt}`,
    );
  }
  const convertedAssetRoot = resolve(proofDirectory, "content/assets/prop-kit");
  cpSync(resolve(repoRoot, "content/assets/prop-kit"), convertedAssetRoot, {
    recursive: true,
  });
  const convertedAuthoring = await run("node", [
    "scripts/author-prop-kit.mjs",
    convertedProject,
    resolve(proofDirectory, "converted-wall-prop-authoring.json"),
    "converted-wall",
    proofDirectory,
  ]);
  if (convertedAuthoring.code !== 0) {
    throw new Error(
      `converted prop authoring exited ${String(convertedAuthoring.code)}\n${convertedAuthoring.stderr}`,
    );
  }
  await runConvertedBrowserProduct(convertedProject);
  await runPersistedConvertedVoxelEditProduct(convertedProject);

  if (!convertedOnly) {
    const migrationReceipt = await persistProject(
      resolve(repoRoot, "content/generated/encounter-gate.project.json"),
      migratedProject,
    );
    if (
      !migrationReceipt.includes("sourceSchema=6") ||
      !migrationReceipt.includes("currentSchema=24")
    ) {
      throw new Error(`migration receipt was incomplete\n${migrationReceipt}`);
    }
    await runMigratedBrowserProduct(migratedProject);
    const exportSaveRoot = process.env.RUSTY_BROWSER_SMOKE_EXPORT_SAVE_ROOT;
    if (exportSaveRoot !== undefined) {
      const sourceSaveRoot = resolve(proofDirectory, "save-slots");
      const destinationSaveRoot = resolve(exportSaveRoot);
      mkdirSync(destinationSaveRoot, { recursive: true });
      for (const entry of readdirSync(sourceSaveRoot)) {
        cpSync(
          resolve(sourceSaveRoot, entry),
          resolve(destinationSaveRoot, entry),
          { recursive: true, force: true },
        );
      }
    }
  }

  console.log(
    convertedOnly
      ? "browser smoke passed: schema-11 converted asset -> Studio-authored serialized appearances -> retained WebGL, collision, navigation, and live edits"
      : "browser smoke passed: persisted projects + converted asset + v6 migration -> accepted gameplay -> shared Rusty Engine retained renderer + shared disposable hosts -> fresh-page posture rebuild",
  );
} finally {
  rmSync(proofDirectory, { recursive: true, force: true });
}

function durableBrowserAuthority(state) {
  const {
    tick: _tick,
    input: _input,
    lastEvents: _lastEvents,
    ...durable
  } = state;
  return JSON.stringify(durable);
}

function bodyDataNumber(html, attribute) {
  const value = Number(bodyDataValue(html, attribute));
  if (!Number.isFinite(value)) {
    throw new Error(`browser smoke did not publish numeric ${attribute}`);
  }
  return value;
}

function bodyDataValue(html, attribute) {
  const match = html.match(new RegExp(`${attribute}="([^"]+)"`));
  if (match?.[1] === undefined) {
    throw new Error(`browser smoke did not publish ${attribute}`);
  }
  return match[1];
}

function bodyJsonScript(html, id) {
  const match = html.match(
    new RegExp(`<script(?=[^>]*id="${id}")[^>]*>([\\s\\S]*?)<\\/script>`),
  );
  if (match?.[1] === undefined) {
    throw new Error(`browser smoke did not publish JSON script #${id}`);
  }
  return JSON.parse(match[1]);
}

function assertRendererStatisticsProof(proof) {
  if (
    proof?.kind !== "loading_bay_renderer_statistics_proof.v1" ||
    proof.schemaVersion !== 1 ||
    proof.contentRichAssetCount !== 4 ||
    proof.contentRichInstanceCount !== 32
  ) {
    throw new Error(
      `renderer statistics proof identity was invalid\n${JSON.stringify(proof)}`,
    );
  }
  const deltas = {
    drawCallCount: ["perSubmission", 32],
    renderHandleCount: ["liveResident", 33],
    geometryResourceCount: ["liveResident", 4],
    materialResourceCount: ["liveResident", 4],
    textureResourceCount: ["liveResident", 0],
    animatedInstanceCount: ["liveResident", 0],
    triangleCount: ["perSubmission", 64],
  };
  const retainedAfterCleanup = new Set([
    "geometryResourceCount",
    "materialResourceCount",
  ]);
  for (const [counter, [scope, delta]] of Object.entries(deltas)) {
    const placeholder = proof.placeholder?.statistics?.[counter];
    const contentRich = proof.contentRich?.statistics?.[counter];
    const restored = proof.restored?.statistics?.[counter];
    if (
      placeholder?.status !== "available" ||
      contentRich?.status !== "available" ||
      restored?.status !== "available" ||
      placeholder.scope !== scope ||
      contentRich.scope !== scope ||
      restored.scope !== scope ||
      contentRich.value - placeholder.value !== delta ||
      restored.value !==
        (retainedAfterCleanup.has(counter)
          ? contentRich.value
          : placeholder.value)
    ) {
      throw new Error(
        `renderer statistics proof failed for ${counter}\n${JSON.stringify({ placeholder, contentRich, restored, delta })}`,
      );
    }
  }
  if (
    proof.placeholder.source !== "explicit" ||
    proof.contentRich.source !== "explicit" ||
    proof.restored.source !== "explicit" ||
    proof.contentRich.renderSequence !== proof.placeholder.renderSequence + 1 ||
    proof.restored.renderSequence !== proof.contentRich.renderSequence + 1
  ) {
    throw new Error(
      `renderer statistics proof added scheduling or lost submission order\n${JSON.stringify(proof)}`,
    );
  }
}

async function persistProject(input, output) {
  const result = await run("cargo", [
    "run",
    "-q",
    "-p",
    "loading-bay-game",
    "--bin",
    "project-store",
    "--",
    "--input",
    input,
    "--output",
    output,
  ]);
  if (result.code !== 0) {
    throw new Error(
      `project-store exited ${String(result.code)}\n${result.stderr}`,
    );
  }
  return result.stdout;
}

async function runFullBrowserProduct(project) {
  const expectedAssetCount = storedProjectAssetCount(project);
  const expectedEntityCount = storedProjectEntityCount(project);
  const running = await launchHost(project);
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const result = await runChromiumSmoke(
      `http://${running.address}/?smoke=1&renderer-statistics-proof=1#/game`,
      "document.body?.dataset.smokeStatus === 'pass' || document.body?.dataset.smokeStatus === 'fail'",
      FULL_CAMPAIGN_TIMEOUT_MILLISECONDS,
    );
    if (result.code !== 0) {
      throw new Error(
        [
          `Chromium exited ${String(result.code)}`,
          result.stdout.match(/<body[^>]*>/)?.[0] ?? "body tag unavailable",
          result.stderr.slice(-4_000),
        ].join("\n"),
      );
    }
    const required = [
      'data-smoke-status="pass"',
      'data-status="pass"',
      'data-campaign-baseline="pass"',
      'data-campaign-arrival="pass"',
      'data-campaign-storage="pass"',
      'data-campaign-locked-door="pass"',
      'data-campaign-generator="pass"',
      'data-campaign-loopback="pass"',
      'data-campaign-finale="pass"',
      'data-campaign-weapons="pass"',
      'data-progression-route="pass"',
      'data-checkpoint-replacement="pass"',
      'data-completed-save="pass"',
      'data-enemy-archetypes="pass"',
      'data-enemy-drops="pass"',
      'data-beacon-activation="pass"',
      'data-session-transport="pass"',
      'data-session-protocol="1"',
      'data-session-pending-outbound-max="1"',
      'data-session-dropped-facts="0"',
      'data-session-pending-input="0"',
      'data-session-pending-edges="0"',
      'data-event-history-bounded="pass"',
      'data-event-kinds-bounded="pass"',
      'data-renderer-telemetry="pass"',
      'data-renderer-single-loop="pass"',
      'data-renderer-telemetry-refresh="pass"',
      'data-renderer-statistics-proof="pass"',
      'data-weapon-viewmodel="pass"',
      'data-weapon-viewmodel-layer="viewmodel"',
      "PASS · Original Loading Bay campaign completed through Rust authority",
      "EnemyDefeated",
      "EncounterCleared",
      "DoorOpened",
      "PlayerMoved",
      "PlayerLookChanged",
      "CombatHit",
      "DamageApplied",
      "CombatEnemyDefeated",
      "ExtractionBeaconActivated",
      "LevelCompleted",
    ];
    const missing = required.filter(
      (marker) => !result.stdout.includes(marker),
    );
    if (missing.length > 0) {
      throw new Error(
        `browser smoke missing ${missing.join(", ")}\n${result.stdout.match(/<body[^>]*>/)?.[0] ?? "body tag unavailable"}\n${result.stdout.slice(-6_000)}`,
      );
    }
    const sessionEvidence = {
      legacyBytes: bodyDataNumber(result.stdout, "data-session-legacy-bytes"),
      bootstrapBytes: bodyDataNumber(
        result.stdout,
        "data-session-bootstrap-bytes",
      ),
      staticUpdates: bodyDataNumber(
        result.stdout,
        "data-session-static-updates",
      ),
      staticMaxBytes: bodyDataNumber(
        result.stdout,
        "data-session-static-max-bytes",
      ),
      steadyBytes: bodyDataNumber(result.stdout, "data-session-steady-bytes"),
      steadyMaxBytes: bodyDataNumber(
        result.stdout,
        "data-session-steady-max-bytes",
      ),
      pendingOutboundMax: bodyDataNumber(
        result.stdout,
        "data-session-pending-outbound-max",
      ),
      pendingInputMax: bodyDataNumber(
        result.stdout,
        "data-session-pending-input-max",
      ),
      pendingEdgesMax: bodyDataNumber(
        result.stdout,
        "data-session-pending-edges-max",
      ),
      droppedFacts: bodyDataNumber(result.stdout, "data-session-dropped-facts"),
      buildMaxMicroseconds: bodyDataNumber(
        result.stdout,
        "data-session-build-max-microseconds",
      ),
      roundTripMaxMilliseconds: bodyDataNumber(
        result.stdout,
        "data-session-rtt-max-milliseconds",
      ),
      eventHistoryCount: bodyDataNumber(
        result.stdout,
        "data-event-history-count",
      ),
      eventHistoryCapacity: bodyDataNumber(
        result.stdout,
        "data-event-history-capacity",
      ),
      eventKindCount: bodyDataNumber(result.stdout, "data-event-kind-count"),
      eventKindCapacity: bodyDataNumber(
        result.stdout,
        "data-event-kind-capacity",
      ),
    };
    if (
      sessionEvidence.legacyBytes <= 0 ||
      sessionEvidence.bootstrapBytes <= 0 ||
      sessionEvidence.staticUpdates !== 0 ||
      sessionEvidence.staticMaxBytes !== 0 ||
      sessionEvidence.steadyBytes >= sessionEvidence.legacyBytes / 2 ||
      sessionEvidence.steadyMaxBytes >= sessionEvidence.legacyBytes / 2 ||
      sessionEvidence.pendingOutboundMax !== 1 ||
      sessionEvidence.pendingInputMax > 2 ||
      sessionEvidence.pendingEdgesMax > 32 ||
      sessionEvidence.droppedFacts !== 0 ||
      sessionEvidence.eventHistoryCapacity !== 256 ||
      sessionEvidence.eventHistoryCount >
        sessionEvidence.eventHistoryCapacity ||
      sessionEvidence.eventKindCapacity !== 64 ||
      sessionEvidence.eventKindCount > sessionEvidence.eventKindCapacity ||
      sessionEvidence.roundTripMaxMilliseconds <= 0 ||
      sessionEvidence.roundTripMaxMilliseconds >= 2_000
    ) {
      throw new Error(
        `game-session measurements violated the product budget\n${JSON.stringify(sessionEvidence)}`,
      );
    }
    console.log(`game-session proof ${JSON.stringify(sessionEvidence)}`);
    const rendererEvidence = {
      timingSource: bodyDataValue(result.stdout, "data-renderer-timing-source"),
      frameIntervalStatus: bodyDataValue(
        result.stdout,
        "data-renderer-frame-interval-status",
      ),
      backendSubmissionStatus: bodyDataValue(
        result.stdout,
        "data-renderer-backend-submission-status",
      ),
      renderSequence: bodyDataNumber(
        result.stdout,
        "data-renderer-render-sequence",
      ),
      frameIntervalMilliseconds: bodyDataNumber(
        result.stdout,
        "data-renderer-frame-interval-milliseconds",
      ),
      backendSubmissionMilliseconds: bodyDataNumber(
        result.stdout,
        "data-renderer-backend-submission-milliseconds",
      ),
      entityCount: bodyDataNumber(result.stdout, "data-renderer-entity-count"),
      residentChunkCount: bodyDataNumber(
        result.stdout,
        "data-renderer-resident-chunk-count",
      ),
      renderDiffCount: bodyDataNumber(
        result.stdout,
        "data-renderer-render-diff-count",
      ),
    };
    if (
      rendererEvidence.timingSource !== "animationFrame" ||
      rendererEvidence.frameIntervalStatus !== "available" ||
      rendererEvidence.backendSubmissionStatus !== "available" ||
      rendererEvidence.renderSequence <= 1 ||
      rendererEvidence.frameIntervalMilliseconds <= 0 ||
      rendererEvidence.backendSubmissionMilliseconds < 0 ||
      rendererEvidence.entityCount <= 0 ||
      rendererEvidence.residentChunkCount !== 0 ||
      rendererEvidence.renderDiffCount < 0
    ) {
      throw new Error(
        `shared-renderer telemetry was not live\n${JSON.stringify(rendererEvidence)}`,
      );
    }
    console.log(
      `shared-renderer correctness proof ${JSON.stringify(rendererEvidence)} (headless SwiftShader; not GPU performance evidence)`,
    );
    const rendererStatisticsProof = bodyJsonScript(
      result.stdout,
      "renderer-statistics-proof",
    );
    assertRendererStatisticsProof(rendererStatisticsProof);
    console.log(
      `renderer-statistics proof ${JSON.stringify(rendererStatisticsProof)} (headless SwiftShader; exact counts, not GPU performance evidence)`,
    );
    const beforeReloadResponse = await fetch(
      `http://${running.address}/api/state`,
    );
    const beforeReload = await beforeReloadResponse.json();
    if (
      !beforeReloadResponse.ok ||
      beforeReload.encounterState !== "cleared" ||
      beforeReload.doorState !== "open" ||
      beforeReload.extractionBeacon?.state !== "active" ||
      !beforeReload.enemies?.every((enemy) => enemy.state === "defeated") ||
      ![33, 34].every((pickupId) => {
        const state = beforeReload.pickups?.find(
          (pickup) => pickup.id === pickupId,
        )?.state;
        return state === "available" || state === "collected";
      }) ||
      beforeReload.presentation?.cues?.length !== 0
    ) {
      throw new Error(
        `browser reload baseline was not retained defeated/open authority\n${JSON.stringify(beforeReload)}`,
      );
    }
    const reloadResult = await runChromiumSmoke(
      `http://${running.address}/?reload-smoke=1#/game`,
      "document.body?.dataset.smokeStatus === 'pass' || document.body?.dataset.smokeStatus === 'fail'",
      30_000,
    );
    if (reloadResult.code !== 0) {
      throw new Error(
        `Reload Chromium exited ${String(reloadResult.code)}\n${reloadResult.stderr.slice(-4_000)}`,
      );
    }
    const reloadRequired = [
      'data-smoke-status="pass"',
      'data-status="pass"',
      'data-feedback-page-reload="pass"',
      'data-reload-posture="pass"',
      'data-reload-cues="pass"',
      'data-reload-pulses="pass"',
      'data-reload-dom-targets="0"',
      'data-reload-audio-targets="0"',
      'data-posture="open"',
      'data-posture="defeated"',
      'data-posture="active"',
      "PASS · Page reload rebuilt posture without transient feedback",
    ];
    const missingReload = reloadRequired.filter(
      (marker) => !reloadResult.stdout.includes(marker),
    );
    if (missingReload.length > 0) {
      throw new Error(
        `browser reload smoke missing ${missingReload.join(", ")}\n${reloadResult.stdout.slice(-6_000)}`,
      );
    }
    const afterReloadResponse = await fetch(
      `http://${running.address}/api/state`,
    );
    const afterReload = await afterReloadResponse.json();
    if (
      !afterReloadResponse.ok ||
      durableBrowserAuthority(afterReload) !==
        durableBrowserAuthority(beforeReload)
    ) {
      throw new Error(
        `browser reload changed durable authority\nbefore=${durableBrowserAuthority(beforeReload)}\nafter=${durableBrowserAuthority(afterReload)}`,
      );
    }
    const lifecycleResult = await runChromiumSmoke(
      `http://${running.address}/?lifecycle-smoke=1#/game`,
      "document.body?.dataset.routeDisposal === 'pass' || document.body?.dataset.smokeStatus === 'fail'",
      30_000,
    );
    if (lifecycleResult.code !== 0) {
      throw new Error(
        `Lifecycle Chromium exited ${String(lifecycleResult.code)}\n${lifecycleResult.stderr.slice(-4_000)}`,
      );
    }
    const lifecycleRequired = [
      'data-renderer-lifecycle="disposed"',
      'data-weapon-viewmodel-lifecycle="disposed"',
      'data-route-disposal="pass"',
      'data-smoke-status="pass"',
      "Shared surface released",
    ];
    const missingLifecycle = lifecycleRequired.filter(
      (marker) => !lifecycleResult.stdout.includes(marker),
    );
    if (missingLifecycle.length > 0) {
      throw new Error(
        `browser lifecycle smoke missing ${missingLifecycle.join(", ")}\n${lifecycleResult.stdout.slice(-6_000)}`,
      );
    }
    await runViewmodelResizeProof(project);
    await runIsolatedGameShellProof(project, {
      width: 1440,
      height: 900,
      label: "desktop",
    });
    await runIsolatedGameShellProof(project, {
      width: 390,
      height: 844,
      label: "narrow",
    });
    await runDeadDialogFocusProof(project);
    await runCampaignSaveReconnectProof(project, running);
    await runHostReplacementContinueProof(project);
    const startup = running.output();
    for (const marker of [
      "project id=loading-bay",
      "sourceSchema=24",
      "currentSchema=24",
      "entryScene=scene/loading-bay",
      `assets=${String(expectedAssetCount)}`,
      "scenes=1",
      `entities=${String(expectedEntityCount)}`,
    ]) {
      if (!startup.includes(marker)) {
        throw new Error(`browser host startup missing ${marker}\n${startup}`);
      }
    }
  } finally {
    await stopHost(running.host);
  }
}

async function runCampaignSaveReconnectProof(project, completedHost) {
  const saveRoot = resolve(proofDirectory, "save-slots");
  const completed = await waitForHostState(
    completedHost.address,
    (state) =>
      state.levelComplete === true &&
      state.saveSlots?.some(
        (slot) =>
          slot.slot === "slot3" &&
          slot.compatibility === "available" &&
          slot.metadata?.levelComplete === true,
      ) === true,
    "completed cold-root campaign before fresh host",
  );
  const running = await launchHost(project, undefined, saveRoot);
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const fresh = await waitForHostState(
      running.address,
      (state) =>
        state.hostSessionId !== completed.hostSessionId &&
        state.levelComplete === false &&
        state.saveSlots?.some(
          (slot) =>
            slot.slot === "slot3" &&
            slot.compatibility === "available" &&
            slot.metadata?.levelComplete === true,
        ) === true,
      "fresh host with compatible campaign save",
    );
    if (
      fresh.player.position.join(",") === completed.player.position.join(",")
    ) {
      throw new Error("fresh host started from saved runtime before load");
    }
    const restore = await runChromiumSmoke(
      `http://${running.address}/#/`,
      "document.body?.dataset.completedSaveRestore === 'pass' || document.body?.dataset.completedSaveRestore === 'fail'",
      30_000,
      {
        viewport: { width: 1440, height: 900 },
        interactiveSetup: async (client) => {
          await waitForCdp(
            client,
            `document.querySelector("red-main-menu") !== null`,
            "fresh-host campaign main menu",
          );
          await waitForCdp(
            client,
            `(() => {
              const button = [...document.querySelectorAll("button")].find(
                (candidate) => candidate.textContent?.trim() === "Continue",
              );
              return button?.disabled === false &&
                document.querySelector(".availability")?.textContent?.includes(
                  "Rust-owned storage",
                ) === true;
            })()`,
            "persisted campaign Continue availability",
          );
          await client.send("Runtime.evaluate", {
            expression: `[...document.querySelectorAll("button")].find(
              (button) => button.textContent?.trim() === "Continue",
            )?.click()`,
          });
          await waitForHostState(
            running.address,
            (state) =>
              state.levelComplete === true &&
              state.levelExits?.some(
                (exit) => exit.id === 32 && exit.state === "completed",
              ) === true &&
              state.player.position.join(",") ===
                completed.player.position.join(","),
            "completed campaign save restoration",
          );
          await waitForCdp(
            client,
            `document.querySelector(".game-state-overlay")?.textContent?.includes("LOADING BAY COMPLETE") === true`,
            "restored campaign completion dialog",
          );
          await client.send("Runtime.evaluate", {
            expression: `document.body.dataset.completedSaveRestore = "pass"`,
          });
        },
      },
    );
    if (
      restore.code !== 0 ||
      !restore.stdout.includes('data-completed-save-restore="pass"')
    ) {
      throw new Error(
        `fresh-process campaign restore failed\n${restore.stderr.slice(-4_000)}\n${restore.stdout.slice(-8_000)}`,
      );
    }
  } finally {
    await stopHost(running.host);
  }
}

async function waitForHostState(address, predicate, label, timeout = 15_000) {
  const deadline = Date.now() + timeout;
  let lastState;
  while (Date.now() < deadline) {
    const response = await fetch(`http://${address}/api/state`, {
      cache: "no-store",
    });
    const state = await response.json();
    lastState = state;
    if (response.ok && predicate(state)) {
      return state;
    }
    await delay(50);
  }
  throw new Error(
    `timed out waiting for ${label}: ${JSON.stringify({
      player: lastState?.player,
      enemies: lastState?.enemies,
      doorAccess: lastState?.doorAccess,
      doorState: lastState?.doorState,
      levelComplete: lastState?.levelComplete,
      levelExits: lastState?.levelExits,
      secretRegions: lastState?.secretRegions,
      interaction: lastState?.interaction,
    })}`,
  );
}

async function defeatEnemyThroughBrowserInput(client, address, enemyId) {
  for (let shot = 0; shot < 4; shot += 1) {
    let state = await waitForHostState(
      address,
      (candidate) =>
        candidate.enemies?.some((enemy) => enemy.id === enemyId) === true,
      `enemy ${String(enemyId)} projection`,
    );
    const enemy = state.enemies.find((candidate) => candidate.id === enemyId);
    if (enemy.state === "defeated") {
      return;
    }
    state = await aimAtEnemyThroughBrowserInput(
      client,
      address,
      enemyId,
      state,
    );
    const healthBefore = state.enemies.find(
      (candidate) => candidate.id === enemyId,
    ).currentHealth;
    await client.send("Runtime.evaluate", {
      expression: `document.querySelector("#primary-fire")?.click()`,
    });
    await waitForHostState(
      address,
      (candidate) => {
        const updated = candidate.enemies?.find(
          (enemy) => enemy.id === enemyId,
        );
        return (
          updated?.state === "defeated" || updated?.currentHealth < healthBefore
        );
      },
      `enemy ${String(enemyId)} browser-input damage`,
    );
  }
  await waitForHostState(
    address,
    (state) =>
      state.enemies?.find((enemy) => enemy.id === enemyId)?.state ===
      "defeated",
    `enemy ${String(enemyId)} browser-input defeat`,
  );
}

async function aimAtEnemyThroughBrowserInput(
  client,
  address,
  enemyId,
  initialState,
) {
  let state = initialState;
  for (let step = 0; step < 40; step += 1) {
    const enemy = state.enemies.find((candidate) => candidate.id === enemyId);
    const offsetX = enemy.position[0] - state.player.position[0];
    const offsetY = enemy.position[1] - state.player.position[1];
    const offsetZ = enemy.position[2] - state.player.position[2];
    const desiredYaw = normalizeDegrees(
      (Math.atan2(-offsetX, -offsetZ) * 180) / Math.PI,
    );
    const desiredPitch =
      (Math.atan2(offsetY, Math.hypot(offsetX, offsetZ)) * 180) / Math.PI;
    const yawDifference = normalizeDegrees(
      desiredYaw - state.player.yawDegrees,
    );
    const pitchDifference = desiredPitch - state.player.pitchDegrees;
    if (Math.abs(yawDifference) < 1 && Math.abs(pitchDifference) < 1) {
      return state;
    }
    const beforeYaw = state.player.yawDegrees;
    const beforePitch = state.player.pitchDegrees;
    const yawUnits = Math.max(-1, Math.min(1, yawDifference / 12));
    const pitchUnits = Math.max(-1, Math.min(1, pitchDifference / 12));
    await client.send("Runtime.evaluate", {
      expression: `window.dispatchEvent(new MouseEvent("mousemove", {
        movementX: ${String(-yawUnits * 20)},
        movementY: ${String(-pitchUnits * 20)},
      }))`,
    });
    state = await waitForHostState(
      address,
      (candidate) =>
        candidate.player.yawDegrees !== beforeYaw ||
        candidate.player.pitchDegrees !== beforePitch,
      `enemy ${String(enemyId)} browser-input aim`,
    );
  }
  throw new Error(`could not aim at enemy ${String(enemyId)} in Chromium`);
}

async function orientPlayerThroughBrowserInput(
  client,
  address,
  desiredYaw,
  desiredPitch,
) {
  let state = await waitForHostState(
    address,
    (candidate) => candidate.player !== undefined,
    "player projection for browser-input orientation",
  );
  for (let step = 0; step < 40; step += 1) {
    const yawDifference = normalizeDegrees(
      desiredYaw - state.player.yawDegrees,
    );
    const pitchDifference = desiredPitch - state.player.pitchDegrees;
    if (Math.abs(yawDifference) < 1 && Math.abs(pitchDifference) < 1) {
      return;
    }
    const beforeYaw = state.player.yawDegrees;
    const beforePitch = state.player.pitchDegrees;
    const yawUnits = Math.max(-1, Math.min(1, yawDifference / 12));
    const pitchUnits = Math.max(-1, Math.min(1, pitchDifference / 12));
    await client.send("Runtime.evaluate", {
      expression: `window.dispatchEvent(new MouseEvent("mousemove", {
        movementX: ${String(-yawUnits * 20)},
        movementY: ${String(-pitchUnits * 20)},
      }))`,
    });
    state = await waitForHostState(
      address,
      (candidate) =>
        candidate.player.yawDegrees !== beforeYaw ||
        candidate.player.pitchDegrees !== beforePitch,
      "browser-input route orientation",
    );
  }
  throw new Error("could not restore progression route orientation");
}

function normalizeDegrees(degrees) {
  return ((((degrees + 180) % 360) + 360) % 360) - 180;
}

async function holdKeysUntil(
  client,
  address,
  codes,
  predicate,
  label,
  timeout = 15_000,
) {
  for (const code of codes) {
    await dispatchKey(client, "keyDown", code);
  }
  try {
    return await waitForHostState(address, predicate, label, timeout);
  } finally {
    for (const code of codes.toReversed()) {
      await dispatchKey(client, "keyUp", code);
    }
    await waitForHostState(
      address,
      (state) => state.playerMotionState === "idle",
      `${label} input release`,
      5_000,
    );
  }
}

async function pressKey(client, code) {
  await client.send("Runtime.evaluate", {
    expression: `window.dispatchEvent(
      new KeyboardEvent("keydown", {
        code: ${JSON.stringify(code)},
        key: ${JSON.stringify(code.startsWith("Key") ? code.slice(3).toLowerCase() : code)},
        bubbles: true,
      }),
    )`,
  });
  await client.send("Runtime.evaluate", {
    expression: `window.dispatchEvent(
      new KeyboardEvent("keyup", {
        code: ${JSON.stringify(code)},
        key: ${JSON.stringify(code.startsWith("Key") ? code.slice(3).toLowerCase() : code)},
        bubbles: true,
      }),
    )`,
  });
}

function dispatchKey(client, type, code) {
  const key = code.startsWith("Key") ? code.slice(3).toLowerCase() : code;
  const virtualKeyCode = code.startsWith("Key")
    ? code.charCodeAt(code.length - 1)
    : 0;
  return client.send("Input.dispatchKeyEvent", {
    type,
    key,
    code,
    windowsVirtualKeyCode: virtualKeyCode,
    nativeVirtualKeyCode: virtualKeyCode,
  });
}

async function runHostReplacementContinueProof(project) {
  const address = `127.0.0.1:${String(await reservePort())}`;
  const saveRoot = resolve(proofDirectory, "host-continuity-save-slots");
  const profileDirectory = mkdtempSync(
    join(tmpdir(), "rusty-engine-host-continuity-"),
  );
  let running = await launchHost(project, address, saveRoot);
  try {
    await waitForHealth(
      `http://${address}/health`,
      running.host,
      running.output,
    );
    const first = await runChromiumSmoke(
      `http://${address}/#/`,
      "document.body?.dataset.hostContinuitySeed === 'pass'",
      15_000,
      {
        profileDirectory,
        setupExpression: `(async () => {
          const state = await fetch("/api/state", { cache: "no-store" }).then(
            (response) => response.json(),
          );
          localStorage.setItem(
            "rusty-engine-demo.continue-session.v1",
            state.hostSessionId,
          );
          document.body.dataset.hostContinuitySeed =
            typeof state.hostSessionId === "string" &&
            state.hostSessionId.length > 0
              ? "pass"
              : "fail";
        })()`,
      },
    );
    if (
      first.code !== 0 ||
      !first.stdout.includes('data-host-continuity-seed="pass"')
    ) {
      throw new Error(
        `host continuity seed failed\n${first.stderr}\n${first.stdout.slice(-4_000)}`,
      );
    }
    await stopHost(running.host);
    running = await launchHost(project, address, saveRoot);
    await waitForHealth(
      `http://${address}/health`,
      running.host,
      running.output,
    );
    const replacement = await runChromiumSmoke(
      `http://${address}/#/`,
      "document.body?.dataset.hostReplacementContinue === 'pass' || document.body?.dataset.hostReplacementContinue === 'fail'",
      15_000,
      {
        profileDirectory,
        setupExpression: `(async () => {
          const delay = (milliseconds) =>
            new Promise((resolve) => setTimeout(resolve, milliseconds));
          const deadline = Date.now() + 10000;
          while (Date.now() < deadline) {
            const button = [...document.querySelectorAll("button")].find(
              (element) => element.textContent?.trim() === "Continue",
            );
            const message =
              document.querySelector(".availability")?.textContent ?? "";
            if (
              button instanceof HTMLButtonElement &&
              !message.includes("Checking")
            ) {
              const state = await fetch("/api/state", {
                cache: "no-store",
              }).then((response) => response.json());
              const stale =
                localStorage.getItem(
                  "rusty-engine-demo.continue-session.v1",
                ) !== state.hostSessionId;
              document.body.dataset.hostReplacementContinue =
                stale &&
                button.disabled &&
                message.includes(
                  "No verified live session or compatible save exists",
                )
                  ? "pass"
                  : "fail";
              return;
            }
            await delay(50);
          }
          document.body.dataset.hostReplacementContinue = "fail";
        })()`,
      },
    );
    if (
      replacement.code !== 0 ||
      !replacement.stdout.includes('data-host-replacement-continue="pass"')
    ) {
      throw new Error(
        `replacement host exposed stale Continue\n${replacement.stderr}\n${replacement.stdout.slice(-6_000)}`,
      );
    }
  } finally {
    await stopHost(running.host);
    removeChromiumProfile(profileDirectory);
  }
}

async function runIsolatedGameShellProof(project, viewport) {
  const running = await launchHost(
    project,
    undefined,
    resolve(proofDirectory, `${viewport.label}-game-shell-save-slots`),
  );
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    await runGameShellProof(running.address, viewport);
  } finally {
    await stopHost(running.host);
  }
}

async function runViewmodelResizeProof(project) {
  const running = await launchHost(project);
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const result = await runChromiumSmoke(
      `http://${running.address}/#/`,
      "document.body?.dataset.weaponViewmodelResize === 'pass' || document.body?.dataset.weaponViewmodelResize === 'fail'",
      30_000,
      {
        viewport: { width: 1280, height: 720 },
        interactiveSetup: async (client) => {
          await waitForCdp(
            client,
            "document.querySelector('red-main-menu') !== null",
            "viewmodel resize main menu",
          );
          await client.send("Runtime.evaluate", {
            expression: `[...document.querySelectorAll("button")].find(
              (button) => button.textContent?.trim() === "New game",
            )?.click()`,
          });
          await waitForCdp(
            client,
            `document.body.dataset.weaponViewmodel === "pass" &&
              document.body.dataset.weaponViewmodelLayer === "viewmodel" &&
              document.body.dataset.weaponViewmodelLifecycle === "mounted" &&
              document.body.dataset.rendererLifecycle === "mounted" &&
              document.querySelector(".game-state-overlay") === null &&
              document.querySelector("#feedback-layer")?.dataset.viewmodelNodes === "3"`,
            "initial retained viewmodel",
          );
          for (const viewport of [
            { width: 1024, height: 600 },
            { width: 390, height: 844 },
          ]) {
            await client.send("Emulation.setDeviceMetricsOverride", {
              width: viewport.width,
              height: viewport.height,
              deviceScaleFactor: 1,
              mobile: viewport.width < 600,
            });
            await client.send("Runtime.evaluate", {
              expression: `window.dispatchEvent(new Event("resize"))`,
            });
            await waitForCdp(
              client,
              `(() => {
                const rect = document.querySelector("#viewport")?.getBoundingClientRect();
                return rect !== undefined &&
                  innerWidth === ${String(viewport.width)} &&
                  rect.width >= document.documentElement.clientWidth - 2 &&
                  document.body.dataset.weaponViewmodel === "pass" &&
                  document.body.dataset.weaponViewmodelLifecycle === "mounted" &&
                  document.querySelector("#feedback-layer")?.dataset.viewmodelStatus === "active" &&
                  document.querySelector("#feedback-layer")?.dataset.viewmodelNodes === "3";
              })()`,
              `viewmodel at ${String(viewport.width)}x${String(viewport.height)}`,
            );
          }
          await pressKey(client, "Escape");
          await waitForCdp(
            client,
            `document.querySelector(".simulation-state")?.textContent?.includes("PAUSED") === true`,
            "viewmodel pause menu",
          );
          await client.send("Runtime.evaluate", {
            expression: `[...document.querySelectorAll("button")].find(
              (button) => button.textContent?.trim() === "Main menu",
            )?.click()`,
          });
          await waitForCdp(
            client,
            `document.querySelector("red-main-menu") !== null &&
              document.body.dataset.rendererLifecycle === "disposed" &&
              document.body.dataset.weaponViewmodelLifecycle === "disposed"`,
            "viewmodel disposal",
          );
          await waitForCdp(
            client,
            `[...document.querySelectorAll("button")].some(
              (button) =>
                button.textContent?.trim() === "Continue" &&
                button.disabled === false,
            )`,
            "viewmodel Continue availability",
          );
          await client.send("Runtime.evaluate", {
            expression: `[...document.querySelectorAll("button")].find(
              (button) => button.textContent?.trim() === "Continue",
            )?.click()`,
          });
          await waitForCdp(
            client,
            `document.body.dataset.rendererLifecycle === "mounted" &&
              document.body.dataset.weaponViewmodelLifecycle === "mounted" &&
              document.querySelector("#feedback-layer")?.dataset.viewmodelStatus === "active" &&
              document.querySelector("#feedback-layer")?.dataset.viewmodelNodes === "3"`,
            "viewmodel remount",
          );
          await client.send("Runtime.evaluate", {
            expression: `document.body.dataset.weaponViewmodelResize = "pass";
              document.body.dataset.weaponViewmodelRemount = "pass"`,
          });
        },
      },
    );
    if (
      result.code !== 0 ||
      !result.stdout.includes('data-weapon-viewmodel-resize="pass"') ||
      !result.stdout.includes('data-weapon-viewmodel-remount="pass"')
    ) {
      throw new Error(
        `viewmodel resize/lifecycle proof failed\n${result.stderr.slice(-4_000)}\n${result.stdout.slice(-8_000)}`,
      );
    }
  } finally {
    await stopHost(running.host);
  }
}

async function runGameShellProof(address, viewport) {
  const result = await runChromiumSmoke(
    `http://${address}/#/`,
    "document.body?.dataset.gameShellProof === 'pass' || document.body?.dataset.gameShellProof === 'fail'",
    45_000,
    {
      viewport,
      setupExpression: gameShellScenario(viewport.label),
    },
  );
  if (result.code !== 0) {
    throw new Error(
      `${viewport.label} game-shell Chromium exited ${String(result.code)}\n${result.stderr.slice(-4_000)}`,
    );
  }
  const required = [
    'data-game-shell-proof="pass"',
    `data-game-shell-viewport="${viewport.label}"`,
    'data-game-shell-menu="pass"',
    'data-game-shell-continue="pass"',
    'data-game-shell-pause="pass"',
    'data-game-shell-focus="pass"',
    'data-game-shell-inventory="pass"',
    'data-game-shell-settings="pass"',
    'data-game-shell-aiming="pass"',
    'data-game-shell-overflow="pass"',
    'data-weapon-viewmodel="pass"',
    'data-weapon-viewmodel-layer="viewmodel"',
    'data-weapon-viewmodel-lifecycle="mounted"',
  ];
  const missing = required.filter((marker) => !result.stdout.includes(marker));
  if (missing.length > 0) {
    const bodyDataset =
      result.stdout.match(/<body[^>]*>/)?.[0] ?? "<body missing>";
    throw new Error(
      `${viewport.label} game-shell proof missing ${missing.join(", ")}\n${bodyDataset}\n${result.stdout.slice(-8_000)}`,
    );
  }
}

async function runDeadDialogFocusProof(project) {
  const running = await launchHost(
    project,
    undefined,
    resolve(proofDirectory, "dead-dialog-save-slots"),
  );
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const result = await runChromiumSmoke(
      `http://${running.address}/#/`,
      "document.body?.dataset.deadDialogFocus === 'pass' || document.body?.dataset.deadDialogFocus === 'fail'",
      30_000,
      {
        viewport: { width: 1440, height: 900 },
        interactiveSetup: async (client) => {
          await waitForCdp(
            client,
            "document.querySelector('red-main-menu') !== null",
            "dead-dialog main menu",
          );
          await client.send("Runtime.evaluate", {
            expression: `[...document.querySelectorAll("button")].find(
              (button) => button.textContent?.trim() === "New game",
            )?.click()`,
          });
          await waitForCdp(
            client,
            `document.body.dataset.rendererLifecycle === "mounted" &&
              document.querySelector(".game-state-overlay") === null`,
            "dead-dialog connected game",
          );
          await holdKeysUntil(
            client,
            running.address,
            ["KeyW"],
            (state) => state.player.vitalityState === "dead",
            "cargo-floor enemy-caused death",
            60_000,
          );
          await waitForHostState(
            running.address,
            (state) => state.player.vitalityState === "dead",
            "enemy-caused dead player projection",
            // The player remains outside the coolant hazard here and the
            // cargo-floor melee enemy owns the full death transition.
            45_000,
          );
          try {
            await waitForCdp(
              client,
              `document.querySelector(".game-state-overlay")?.textContent?.includes("PLAYER DOWN") === true &&
                document.activeElement?.textContent?.trim() === "Restart loading bay" &&
                document.querySelector("#feedback-layer")?.dataset.viewmodelStatus === "hidden" &&
                document.querySelector("#feedback-layer")?.dataset.viewmodelNodes === "3" &&
                document.body.dataset.weaponViewmodelLifecycle === "mounted"`,
              "focused dead dialog",
            );
          } catch (error) {
            const diagnostic = await client.send("Runtime.evaluate", {
              expression: `({
                dialog: document.querySelector(".game-state-overlay")?.textContent?.trim() ?? "",
                active: document.activeElement?.textContent?.trim() ?? "",
                viewmodelStatus: document.querySelector("#feedback-layer")?.dataset.viewmodelStatus ?? "",
                viewmodelNodes: document.querySelector("#feedback-layer")?.dataset.viewmodelNodes ?? "",
                lifecycle: document.body.dataset.weaponViewmodelLifecycle ?? "",
              })`,
              returnByValue: true,
            });
            throw new Error(
              `${error instanceof Error ? error.message : String(error)}: ${JSON.stringify(diagnostic?.result?.value)}`,
            );
          }
          await client.send("Input.dispatchKeyEvent", {
            type: "keyDown",
            key: "Tab",
            code: "Tab",
            windowsVirtualKeyCode: 9,
            nativeVirtualKeyCode: 9,
          });
          await client.send("Input.dispatchKeyEvent", {
            type: "keyUp",
            key: "Tab",
            code: "Tab",
            windowsVirtualKeyCode: 9,
            nativeVirtualKeyCode: 9,
          });
          await waitForCdp(
            client,
            `document.activeElement?.textContent?.trim() === "Main menu"`,
            "dead-dialog second action focus",
          );
          await delay(500);
          const retained = await client.send("Runtime.evaluate", {
            expression: `document.activeElement?.textContent?.trim() === "Main menu"`,
            returnByValue: true,
          });
          if (retained?.result?.value !== true) {
            throw new Error(
              "dead-state projections stole focus from the Main menu action",
            );
          }
          await client.send("Runtime.evaluate", {
            expression: `[...document.querySelectorAll("button")].find(
              (button) => button.textContent?.trim() === "Restart loading bay",
            )?.click()`,
          });
          await waitForCdp(
            client,
            `document.querySelector(".game-state-overlay") === null &&
              document.querySelector("#feedback-layer")?.dataset.viewmodelStatus === "active" &&
              document.querySelector("#feedback-layer")?.dataset.viewmodelWeapon === "weapon/arc-pistol" &&
              document.querySelector("#feedback-layer")?.dataset.viewmodelImpulse === "idle"`,
            "dead-dialog restart",
          );
          await delay(100);
          const restartFocus = await client.send("Runtime.evaluate", {
            expression: `({
              id: document.activeElement?.id ?? "",
              tag: document.activeElement?.tagName ?? "",
              text: document.activeElement?.textContent?.trim() ?? "",
            })`,
            returnByValue: true,
          });
          if (restartFocus?.result?.value?.id !== "viewport") {
            throw new Error(
              `restart did not restore viewport focus: ${JSON.stringify(restartFocus?.result?.value)}`,
            );
          }
          await client.send("Runtime.evaluate", {
            expression: `document.body.dataset.enemyCombatDeath = "pass";
              document.body.dataset.weaponViewmodelDeathReset = "pass";
              document.body.dataset.deadDialogFocus = "pass"`,
          });
        },
      },
    );
    if (
      result.code !== 0 ||
      !result.stdout.includes('data-dead-dialog-focus="pass"') ||
      !result.stdout.includes('data-enemy-combat-death="pass"') ||
      !result.stdout.includes('data-weapon-viewmodel-death-reset="pass"')
    ) {
      throw new Error(
        `dead-dialog focus proof failed\n${result.stderr.slice(-4_000)}\n${result.stdout.slice(-6_000)}`,
      );
    }
  } finally {
    await stopHost(running.host);
  }
}

function gameShellScenario(viewportLabel) {
  return `(async () => {
    const delay = (milliseconds) =>
      new Promise((resolve) => setTimeout(resolve, milliseconds));
    const waitFor = async (predicate, label) => {
      const deadline = Date.now() + 15000;
      while (Date.now() < deadline) {
        if (await predicate()) return;
        await delay(50);
      }
      throw new Error("timed out waiting for " + label);
    };
    const byText = (selector, text) =>
      [...document.querySelectorAll(selector)].find(
        (element) => element.textContent?.trim() === text,
      );
    const control = (text) =>
      [...document.querySelectorAll("label")].find((label) =>
        label.textContent?.includes(text),
      )?.querySelector("input");
    try {
      await waitFor(() => {
        const menu = document.querySelector("red-main-menu");
        const newGame = byText("button", "New game");
        const continueButton = byText("button", "Continue");
        const availability = document.querySelector(".availability")?.textContent?.trim() ?? "";
        return menu !== null &&
          newGame instanceof HTMLButtonElement &&
          continueButton instanceof HTMLButtonElement &&
          availability.length > 0 &&
          !availability.startsWith("Checking");
      }, "authoritative main menu session readiness");
      const newGame = byText("button", "New game");
      const continueButton = byText("button", "Continue");
      if (!(newGame instanceof HTMLButtonElement) ||
          !(continueButton instanceof HTMLButtonElement) ||
          !continueButton.disabled) {
        throw new Error("main menu did not expose accurate new/continue state");
      }
      newGame.click();
      await waitFor(
        () =>
          document.body.dataset.rendererLifecycle === "mounted" &&
          document.querySelector(".game-state-overlay") === null,
        "connected game",
      );
      const canvas = document.querySelector("#viewport");
      const rect = canvas?.getBoundingClientRect();
      const aimingTarget =
        rect === undefined
          ? null
          : document.elementFromPoint(
              Math.floor(rect.left + rect.width / 2),
              Math.floor(rect.top + rect.height / 2),
            );
      const aimingPassed =
        canvas instanceof HTMLCanvasElement &&
        rect !== undefined &&
        rect.width >= document.documentElement.clientWidth - 2 &&
        rect.height >= Math.min(500, innerHeight - 30) &&
        aimingTarget === canvas;
      const hotbarPassed =
        document.querySelectorAll("red-game-hotbar button").length === 3 &&
        document.body.textContent?.includes("Arc Pistol") === true;

      window.dispatchEvent(
        new KeyboardEvent("keydown", { code: "KeyI", bubbles: true }),
      );
      await waitFor(
        () => document.querySelector(".game-panel")?.textContent?.includes("Inventory") === true,
        "inventory panel",
      );
      const inventoryOverlay = document.querySelector(".game-panel-overlay");
      const inventoryFocusEntered =
        inventoryOverlay?.contains(document.activeElement) === true;
      const inventoryFocusable = inventoryOverlay === null
        ? []
        : [...inventoryOverlay.querySelectorAll(
            'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
          )];
      inventoryFocusable.at(-1)?.focus();
      window.dispatchEvent(
        new KeyboardEvent("keydown", { code: "Tab", bubbles: true }),
      );
      const inventoryFocusContained =
        inventoryOverlay?.contains(document.activeElement) === true;
      const inventoryPanel = document.querySelector(".game-panel");
      const inventoryLive =
        inventoryPanel?.textContent?.includes("SIMULATION LIVE") === true &&
        inventoryPanel.textContent.includes("Med Patch") &&
        inventoryPanel.textContent.includes("Arc Pistol");
      const useButton = [...document.querySelectorAll(".game-panel button")].find(
        (button) => button.textContent?.trim().startsWith("USE"),
      );
      if (!(useButton instanceof HTMLButtonElement)) {
        throw new Error("inventory did not expose supported item use");
      }
      const itemStateBefore = await fetch("/api/state", {
        cache: "no-store",
      }).then((response) => response.json());
      const patchQuantityBefore =
        itemStateBefore.inventory?.stacks?.find(
          (stack) => stack.item === "supply/med-patch",
        )?.quantity ?? 0;
      useButton.click();
      let itemUseEvidence = "";
      let itemUsePassed = false;
      if (itemStateBefore.player.currentHealth === itemStateBefore.player.maxHealth) {
        await waitFor(
          () => document.querySelector(".action-rejection") !== null,
          "typed full-health rejection",
        );
        const rejection =
          document.querySelector(".action-rejection")?.textContent ?? "";
        itemUsePassed = rejection.includes("healthFull");
        itemUseEvidence = rejection.trim();
      } else {
        let itemStateAfter = itemStateBefore;
        await waitFor(async () => {
          itemStateAfter = await fetch("/api/state", {
            cache: "no-store",
          }).then((response) => response.json());
          const patchQuantityAfter =
            itemStateAfter.inventory?.stacks?.find(
              (stack) => stack.item === "supply/med-patch",
            )?.quantity ?? 0;
          return (
            itemStateAfter.player.currentHealth >
              itemStateBefore.player.currentHealth &&
            patchQuantityAfter === patchQuantityBefore - 1
          );
        }, "authoritative med-patch consumption");
        itemUsePassed = true;
        itemUseEvidence =
          "healed:" +
          itemStateBefore.player.currentHealth +
          "->" +
          itemStateAfter.player.currentHealth +
          ",patches:" +
          patchQuantityBefore +
          "->" +
          (itemStateAfter.inventory?.stacks?.find(
            (stack) => stack.item === "supply/med-patch",
          )?.quantity ?? 0);
      }
      byText(".panel-actions button", "Return to game")?.click();
      await waitFor(() => document.querySelector(".game-panel-overlay") === null, "game return");

      window.dispatchEvent(
        new KeyboardEvent("keydown", { code: "Escape", bubbles: true }),
      );
      await waitFor(
        () => document.querySelector(".simulation-state")?.textContent?.includes("PAUSED") === true,
        "Rust pause acknowledgement",
      );
      const pauseOverlay = document.querySelector(".game-panel-overlay");
      const pauseFocusEntered =
        pauseOverlay?.contains(document.activeElement) === true;
      const pauseFocusable = pauseOverlay === null
        ? []
        : [...pauseOverlay.querySelectorAll("button:not([disabled])")];
      pauseFocusable.at(-1)?.focus();
      window.dispatchEvent(
        new KeyboardEvent("keydown", { code: "Tab", bubbles: true }),
      );
      const pauseFocusContained =
        pauseOverlay?.contains(document.activeElement) === true;
      const backgroundInert =
        document.querySelector("#viewport")?.hasAttribute("inert") === true &&
        document.querySelector(".hud-top")?.hasAttribute("inert") === true;
      const focusPassed =
        inventoryFocusEntered &&
        inventoryFocusContained &&
        pauseFocusEntered &&
        pauseFocusContained &&
        backgroundInert;
      const pausePassed =
        document.querySelector(".game-panel")?.textContent?.includes("Restart loading bay") === true;
      byText(".pause-actions button", "Settings")?.click();
      await waitFor(
        () => document.querySelector(".game-panel")?.textContent?.includes("Mouse sensitivity") === true,
        "settings panel",
      );
      const sensitivity = control("Mouse sensitivity");
      const invert = control("Invert vertical look");
      const volume = control("Effects volume");
      const flash = control("Flash intensity");
      const hud = control("Show game HUD");
      const telemetry = control("Show renderer telemetry");
      if (!(sensitivity instanceof HTMLInputElement) ||
          !(invert instanceof HTMLInputElement) ||
          !(volume instanceof HTMLInputElement) ||
          !(flash instanceof HTMLInputElement) ||
          !(hud instanceof HTMLInputElement) ||
          !(telemetry instanceof HTMLInputElement)) {
        throw new Error("settings controls were incomplete");
      }
      sensitivity.value = "1.35";
      sensitivity.dispatchEvent(new Event("input", { bubbles: true }));
      invert.checked = true;
      invert.dispatchEvent(new Event("change", { bubbles: true }));
      volume.value = "0.35";
      volume.dispatchEvent(new Event("input", { bubbles: true }));
      flash.value = "0.25";
      flash.dispatchEvent(new Event("input", { bubbles: true }));
      telemetry.checked = true;
      telemetry.dispatchEvent(new Event("change", { bubbles: true }));
      hud.checked = false;
      hud.dispatchEvent(new Event("change", { bubbles: true }));
      await delay(50);
      const hudHidden = document.querySelector(".viewport-card")?.classList.contains("hud-hidden");
      hud.checked = true;
      hud.dispatchEvent(new Event("change", { bubbles: true }));
      await delay(50);
      const stored = JSON.parse(
        localStorage.getItem("rusty-engine-demo.host-user-settings.v1") ?? "{}",
      );
      const settingsPassed =
        stored.mouseSensitivity === 1.35 &&
        stored.invertY === true &&
        stored.sfxVolume === 0.35 &&
        stored.flashIntensity === 0.25 &&
        stored.telemetryVisible === true &&
        hudHidden === true &&
        document.querySelector("#feedback-audio-status")?.dataset.volume === "0.35" &&
        document.querySelector("#feedback-layer")?.dataset.flashIntensity ===
          "0.25" &&
        document.querySelector("#renderer-telemetry")?.hidden === false;
      byText(".panel-actions button", "Done")?.click();
      await waitFor(
        () => document.querySelector(".game-panel")?.textContent?.includes("Restart loading bay") === true,
        "pause return",
      );
      byText(".pause-actions button", "Main menu")?.click();
      await waitFor(() => document.querySelector("red-main-menu") !== null, "menu return");
      await waitFor(() => {
        const button = byText("button", "Continue");
        return button instanceof HTMLButtonElement && !button.disabled;
      }, "Continue availability after menu return");
      const resumedContinue = byText("button", "Continue");
      const menuPassed =
        resumedContinue instanceof HTMLButtonElement && !resumedContinue.disabled;
      resumedContinue?.click();
      await waitFor(
        () =>
          document.body.dataset.rendererLifecycle === "mounted" &&
          document.querySelector(".game-state-overlay") === null,
        "continued game",
      );
      await waitFor(
        () =>
          document.querySelector("red-game-hotbar button[aria-pressed='true']")?.hasAttribute("disabled") === false,
        "continued unpaused simulation",
      );
      const continuePassed =
        document.querySelector(".game-panel-overlay") === null &&
        document.querySelector(".game-state-overlay") === null;
      const overflowPassed = document.documentElement.scrollWidth <= innerWidth + 1;

      document.body.dataset.gameShellViewport = ${JSON.stringify(viewportLabel)};
      document.body.dataset.gameShellAimingTarget =
        aimingTarget instanceof Element
          ? aimingTarget.tagName.toLowerCase() + "." + aimingTarget.className
          : String(aimingTarget);
      document.body.dataset.gameShellCanvasRect =
        rect === undefined
          ? "missing"
          : rect.width + "x" + rect.height + "@" + innerWidth + "x" + innerHeight;
      document.body.dataset.gameShellMenu = menuPassed ? "pass" : "fail";
      document.body.dataset.gameShellContinue = continuePassed ? "pass" : "fail";
      document.body.dataset.gameShellPause = pausePassed ? "pass" : "fail";
      document.body.dataset.gameShellFocus = focusPassed ? "pass" : "fail";
      document.body.dataset.gameShellFocusEvidence = [
        inventoryFocusEntered,
        inventoryFocusContained,
        pauseFocusEntered,
        pauseFocusContained,
        backgroundInert,
      ].join(":");
      document.body.dataset.gameShellInventoryEvidence =
        String(inventoryLive) +
        ":" +
        String(itemUsePassed) +
        ":" +
        String(hotbarPassed) +
        ":" +
        itemUseEvidence;
      document.body.dataset.gameShellInventory =
        inventoryLive && itemUsePassed && hotbarPassed ? "pass" : "fail";
      document.body.dataset.gameShellSettings = settingsPassed ? "pass" : "fail";
      document.body.dataset.gameShellAiming = aimingPassed ? "pass" : "fail";
      document.body.dataset.gameShellOverflow = overflowPassed ? "pass" : "fail";
      document.body.dataset.gameShellProof =
        menuPassed &&
        continuePassed &&
        pausePassed &&
        focusPassed &&
        inventoryLive &&
        itemUsePassed &&
        hotbarPassed &&
        settingsPassed &&
        aimingPassed &&
        overflowPassed
          ? "pass"
          : "fail";
    } catch (error) {
      document.body.dataset.gameShellError =
        error instanceof Error ? error.message : String(error);
      document.body.dataset.gameShellProof = "fail";
    }
  })()`;
}

function storedProjectEntityCount(project) {
  const value = JSON.parse(readFileSync(project, "utf8"));
  if (!Array.isArray(value.scenes)) {
    throw new Error(`stored project ${project} has no scenes`);
  }
  return value.scenes.reduce((total, scene) => {
    if (!Array.isArray(scene.entities)) {
      throw new Error(`stored project ${project} has a scene without entities`);
    }
    return total + scene.entities.length;
  }, 0);
}

function storedProjectAssetCount(project) {
  const value = JSON.parse(readFileSync(project, "utf8"));
  if (!Array.isArray(value.assets)) {
    throw new Error(`stored project ${project} has no asset catalog`);
  }
  return value.assets.length;
}

async function runMigratedBrowserProduct(project) {
  const running = await launchHost(
    project,
    undefined,
    resolve(proofDirectory, "migrated-save-slots"),
  );
  let session;
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    session = await openGameSession(running.address);
    const state = session.state();
    if (
      state.generatedEnvironment?.seed !== 4 ||
      state.enemies?.length !== 0 ||
      state.weapon?.ammoRemaining !== 8 ||
      ![3, 4, 5].every((entity) =>
        state.projection?.some((node) => node.id === entity),
      )
    ) {
      throw new Error(
        `migrated browser state was incomplete\n${JSON.stringify(state)}`,
      );
    }
    const attacked = await session.command({
      kind: "setInputIntent",
      movement: [0, 0],
      lookDelta: [0, 0],
      primaryFireHeld: true,
    });
    if (
      attacked.tick <= state.tick ||
      attacked.weapon?.ammoRemaining >= state.weapon.ammoRemaining
    ) {
      throw new Error(
        `migrated browser action failed\n${JSON.stringify(attacked)}`,
      );
    }
    await session.command({
      kind: "setInputIntent",
      movement: [0, 0],
      lookDelta: [0, 0],
      primaryFireHeld: false,
    });
    const startup = running.output();
    for (const marker of [
      "project id=migrated-v6-project",
      "currentSchema=24",
      "assets=4",
      "scenes=1",
      "entities=6",
    ]) {
      if (!startup.includes(marker)) {
        throw new Error(
          `migrated browser startup missing ${marker}\n${startup}`,
        );
      }
    }
  } finally {
    session?.close();
    await stopHost(running.host);
  }
}

async function runConvertedBrowserProduct(project) {
  const expectedAssetCount = storedProjectAssetCount(project);
  const running = await launchHost(project);
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const result = await runChromiumSmoke(
      `http://${running.address}/?converted-smoke=1#/game`,
      "document.body?.dataset.smokeStatus === 'pass' || document.body?.dataset.smokeStatus === 'fail'",
      CONVERTED_CAMPAIGN_TIMEOUT_MILLISECONDS,
    );
    if (result.code !== 0) {
      const convertedPhase =
        result.stdout.match(/data-converted-phase="([^"]+)"/u)?.[1] ??
        "unpublished";
      throw new Error(
        `converted Chromium exited ${String(result.code)} during ${convertedPhase}\n${result.stderr.slice(-4_000)}\n${result.stdout.slice(-8_000)}`,
      );
    }
    const required = [
      'data-smoke-status="pass"',
      'data-status="pass"',
      'data-converted-asset="pass"',
      'data-converted-visible="pass"',
      'data-converted-collision="pass"',
      'data-converted-navigation="pass"',
      'data-converted-edit="pass"',
      "PASS · Converted voxel asset reached retained WebGL, collision, navigation, and live edits",
      "MATERIALIZED · 90 VOXELS",
      'data-engine="three.js',
    ];
    const missing = required.filter(
      (marker) => !result.stdout.includes(marker),
    );
    if (missing.length > 0) {
      throw new Error(
        `converted browser smoke missing ${missing.join(", ")}\n${result.stdout.slice(-8_000)}`,
      );
    }
    const startup = running.output();
    for (const marker of [
      "project id=converted-wall",
      "sourceSchema=24",
      "currentSchema=24",
      "entryScene=scene/converted-wall",
      `assets=${String(expectedAssetCount)}`,
      "scenes=1",
      "entities=7",
    ]) {
      if (!startup.includes(marker)) {
        throw new Error(
          `converted browser startup missing ${marker}\n${startup}`,
        );
      }
    }
  } finally {
    await stopHost(running.host);
  }
}

async function runPersistedConvertedVoxelEditProduct(project) {
  const edits = [
    { kind: "clear", address: [4, 1, 6] },
    { kind: "clear", address: [5, 1, 6] },
    { kind: "clear", address: [4, 1, 7] },
    { kind: "clear", address: [5, 1, 7] },
  ];
  const running = await launchHost(project);
  let persisted;
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const beforeResponse = await fetch(`http://${running.address}/api/state`);
    const before = await beforeResponse.json();
    if (
      !beforeResponse.ok ||
      before.voxelRevision !== 0 ||
      before.voxelSolidCount !== 94 ||
      before.voxelProbePathLength !== 9 ||
      before.generatedEnvironment !== null
    ) {
      throw new Error(
        `converted persisted-edit baseline was incomplete\n${JSON.stringify(before)}`,
      );
    }
    const editResponse = await fetch(
      `http://${running.address}/api/voxel-edit`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          expectedRevision: before.voxelRevision,
          persistToProject: true,
          edits,
        }),
      },
    );
    const edited = await editResponse.json();
    if (
      !editResponse.ok ||
      edited.voxelEditReceipt?.persistedToProject !== true ||
      edited.voxelEditReceipt?.changedVoxels !== 4 ||
      edited.voxelRevision !== 1 ||
      edited.voxelSolidCount !== 90 ||
      edited.voxelAuthorityHash === before.voxelAuthorityHash ||
      edited.voxelNavigationHash === before.voxelNavigationHash ||
      edited.voxelProbePathLength >= before.voxelProbePathLength ||
      JSON.stringify(edited.voxelMeshes) ===
        JSON.stringify(before.voxelMeshes) ||
      edited.generatedEnvironment !== null
    ) {
      throw new Error(
        `converted persisted voxel edit was incomplete\n${JSON.stringify(edited)}`,
      );
    }
    persisted = voxelStateFingerprint(edited);

    const reset = await restartGameSession(running.address);
    if (
      reset.voxelRevision !== 0 ||
      reset.voxelEditReceipt !== undefined ||
      !reset.lastEvents?.every((event) => event === "NavigationAdvanced") ||
      JSON.stringify(voxelStateFingerprint(reset)) !== JSON.stringify(persisted)
    ) {
      const actual = voxelStateFingerprint(reset);
      throw new Error(
        `converted reset did not reopen static edited authority\nexpected=${JSON.stringify(persisted)}\nactual=${JSON.stringify(actual)}\nlastEvents=${JSON.stringify(reset.lastEvents)}`,
      );
    }
  } finally {
    await stopHost(running.host);
  }

  const bytes = readFileSync(project, "utf8");
  const document = JSON.parse(bytes);
  const environment = document.scenes?.[0]?.voxelEnvironment;
  const removed = new Set(edits.map((edit) => JSON.stringify(edit.address)));
  if (
    environment?.kind !== "material" ||
    !Array.isArray(environment.materialVoxels) ||
    environment.materialVoxels.length !== 90 ||
    environment.materialVoxels.some((voxel) =>
      removed.has(JSON.stringify(voxel.address)),
    ) ||
    (Array.isArray(environment.voxelAssets) &&
      environment.voxelAssets.length !== 0)
  ) {
    throw new Error(
      `converted saved project did not materialize edited authority\n${bytes}`,
    );
  }
  for (const forbidden of [
    "sourceRevision",
    "authorityHash",
    "voxelEdit",
    "changedVoxels",
    "editHistory",
    "events",
    "replay",
  ]) {
    if (bytes.includes(forbidden)) {
      throw new Error(
        `converted saved project leaked transient field ${forbidden}`,
      );
    }
  }

  const reopened = await launchHost(project);
  try {
    await waitForHealth(
      `http://${reopened.address}/health`,
      reopened.host,
      reopened.output,
    );
    const response = await fetch(`http://${reopened.address}/api/state`);
    const state = await response.json();
    if (
      !response.ok ||
      state.voxelRevision !== 0 ||
      JSON.stringify(voxelStateFingerprint(state)) !== JSON.stringify(persisted)
    ) {
      throw new Error(
        `fresh host did not reopen converted edited authority\n${JSON.stringify(state)}`,
      );
    }
  } finally {
    await stopHost(reopened.host);
  }
}

function voxelStateFingerprint(state) {
  return {
    solidCount: state.voxelSolidCount,
    authorityHash: state.voxelAuthorityHash,
    navigationHash: state.voxelNavigationHash,
    probePathLength: state.voxelProbePathLength,
    meshes: state.voxelMeshes.map((mesh) => ({
      chunk: mesh.chunk,
      contentHash: mesh.contentHash,
      boundsMin: mesh.boundsMin,
      boundsMax: mesh.boundsMax,
      groups: mesh.groups.map((group) => ({
        materialSlot: group.materialSlot,
        start: group.start,
        count: group.count,
      })),
      positionCount: mesh.positions.length,
      normalCount: mesh.normals.length,
      indexCount: mesh.indices.length,
    })),
    generatedEnvironment: state.generatedEnvironment,
  };
}

async function restartGameSession(address) {
  const session = await openGameSession(address);
  try {
    return await session.command({
      kind: "restart",
      mode: "authoredBaseline",
    });
  } finally {
    session.close();
  }
}

async function openGameSession(address) {
  const socket = new WebSocket(`ws://${address}/api/session`, "loading-bay.v1");
  const inbox = socketInbox(socket);
  await new Promise((resolveOpen, rejectOpen) => {
    socket.addEventListener("open", resolveOpen, { once: true });
    socket.addEventListener(
      "error",
      () => rejectOpen(new Error("game-session WebSocket failed to open")),
      { once: true },
    );
  });

  let sessionId = "";
  let snapshotSequence = 0;
  let dynamic;
  let resources;
  let state;
  let sequence = 0;

  const apply = (envelope) => {
    if (envelope.protocolVersion !== 1 || envelope.update === undefined) {
      throw new Error(
        `unexpected game-session update ${JSON.stringify(envelope)}`,
      );
    }
    if (envelope.update.kind === "full") {
      dynamic = envelope.update.state;
    } else {
      if (
        dynamic === undefined ||
        envelope.sessionId !== sessionId ||
        envelope.update.baseSnapshotSequence !== snapshotSequence
      ) {
        throw new Error(
          `non-contiguous game-session delta ${JSON.stringify(envelope)}`,
        );
      }
      dynamic = { ...dynamic, ...envelope.update.changes };
    }
    resources = envelope.resources ?? resources;
    if (resources === undefined) {
      throw new Error("game-session update did not establish static resources");
    }
    const { staticRevision: _staticRevision, ...runtimeResources } = resources;
    state = { ...dynamic, ...runtimeResources };
    sessionId = envelope.sessionId;
    snapshotSequence = envelope.snapshotSequence;
    return state;
  };

  apply(await inbox.next());
  return {
    state: () => state,
    async command(command) {
      sequence += 1;
      const commandSequence = sequence;
      const commandSession = sessionId;
      socket.send(
        JSON.stringify({
          protocolVersion: 1,
          sessionId,
          sequence: commandSequence,
          observedSnapshotSequence: snapshotSequence,
          observedStaticRevision: resources?.staticRevision,
          command,
        }),
      );
      for (;;) {
        const envelope = await inbox.next();
        if (envelope.update === undefined) {
          if (envelope.commandSequence === commandSequence) {
            throw new Error(
              `game-session command rejected: ${String(envelope.code)}: ${String(envelope.message)}`,
            );
          }
          continue;
        }
        const accepted = apply(envelope);
        if (command.kind === "restart") {
          if (sessionId !== commandSession) {
            sequence = 0;
            return accepted;
          }
        } else if (
          envelope.acknowledgedCommandSequence >= commandSequence &&
          accepted.input.consumedSequence >= commandSequence
        ) {
          return accepted;
        }
      }
    },
    close() {
      socket.close(1000, "proof complete");
    },
  };
}

function socketInbox(socket) {
  const queued = [];
  const waiting = [];
  let failure;
  socket.addEventListener("message", (event) => {
    const value = JSON.parse(String(event.data));
    const waiter = waiting.shift();
    if (waiter === undefined) {
      queued.push(value);
    } else {
      waiter.resolve(value);
    }
  });
  socket.addEventListener("close", () => {
    failure ??= new Error("game-session WebSocket closed");
    for (const waiter of waiting.splice(0)) {
      waiter.reject(failure);
    }
  });
  socket.addEventListener("error", () => {
    failure ??= new Error("game-session WebSocket failed");
    for (const waiter of waiting.splice(0)) {
      waiter.reject(failure);
    }
  });
  return {
    next() {
      if (queued.length > 0) {
        return Promise.resolve(queued.shift());
      }
      if (failure !== undefined) {
        return Promise.reject(failure);
      }
      return new Promise((resolve, reject) => {
        waiting.push({ resolve, reject });
      });
    },
  };
}

async function runPersistedVoxelEditProduct(project) {
  const edits = [
    { kind: "clear", address: [2, 1, 6] },
    { kind: "clear", address: [3, 1, 6] },
  ];
  const running = await launchHost(project);
  let persistedHash;
  let persistedNavigationHash;
  let persistedPathLength;
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const beforeResponse = await fetch(`http://${running.address}/api/state`);
    const before = await beforeResponse.json();
    const editResponse = await fetch(
      `http://${running.address}/api/voxel-edit`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          expectedRevision: before.voxelRevision,
          persistToProject: true,
          edits,
        }),
      },
    );
    const edited = await editResponse.json();
    if (
      !editResponse.ok ||
      edited.voxelEditReceipt?.persistedToProject !== true ||
      edited.voxelEditReceipt?.changedVoxels !== edits.length ||
      edited.voxelRevision !== 1 ||
      edited.voxelSolidCount !== before.voxelSolidCount - edits.length ||
      edited.voxelAuthorityHash === before.voxelAuthorityHash ||
      edited.voxelNavigationHash === before.voxelNavigationHash ||
      edited.voxelProbePathLength >= before.voxelProbePathLength ||
      edited.generatedEnvironment !== null
    ) {
      throw new Error(
        `persisted voxel edit response was incomplete\n${JSON.stringify(edited)}`,
      );
    }
    persistedHash = edited.voxelAuthorityHash;
    persistedNavigationHash = edited.voxelNavigationHash;
    persistedPathLength = edited.voxelProbePathLength;

    const reset = await restartGameSession(running.address);
    if (
      reset.voxelRevision !== 0 ||
      reset.voxelAuthorityHash !== persistedHash ||
      reset.voxelNavigationHash !== persistedNavigationHash ||
      reset.voxelProbePathLength !== persistedPathLength ||
      reset.generatedEnvironment !== null ||
      reset.voxelEditReceipt !== undefined ||
      !reset.lastEvents?.every((event) => event === "NavigationAdvanced")
    ) {
      throw new Error(
        `persisted voxel reset did not reopen static authority\n${JSON.stringify(reset)}`,
      );
    }
  } finally {
    await stopHost(running.host);
  }

  const bytes = readFileSync(project, "utf8");
  const document = JSON.parse(bytes);
  const environment = document.scenes?.[0]?.voxelEnvironment;
  if (
    environment?.kind !== "material" ||
    !Array.isArray(environment.materialVoxels) ||
    edits.some((edit) =>
      environment.materialVoxels.some(
        (voxel) =>
          JSON.stringify(voxel.address) === JSON.stringify(edit.address),
      ),
    )
  ) {
    throw new Error(
      `saved project did not materialize the accepted edit\n${bytes}`,
    );
  }
  for (const forbidden of [
    "sourceRevision",
    "authorityHash",
    "voxelEdit",
    "changedVoxels",
    "editHistory",
    "events",
  ]) {
    if (bytes.includes(forbidden)) {
      throw new Error(`saved project leaked transient field ${forbidden}`);
    }
  }

  const reopened = await launchHost(project);
  try {
    await waitForHealth(
      `http://${reopened.address}/health`,
      reopened.host,
      reopened.output,
    );
    const response = await fetch(`http://${reopened.address}/api/state`);
    const state = await response.json();
    if (
      !response.ok ||
      state.voxelRevision !== 0 ||
      state.voxelAuthorityHash !== persistedHash ||
      state.voxelNavigationHash !== persistedNavigationHash ||
      state.voxelProbePathLength !== persistedPathLength ||
      state.generatedEnvironment !== null
    ) {
      throw new Error(
        `fresh host did not reopen persisted voxel authority\n${JSON.stringify(state)}`,
      );
    }
  } finally {
    await stopHost(reopened.host);
  }
}

async function launchHost(
  project,
  requestedAddress,
  saveRoot = resolve(proofDirectory, "save-slots"),
) {
  const address =
    requestedAddress ?? `127.0.0.1:${String(await reservePort())}`;
  const installedHost = process.env.RUSTY_ENGINE_DEMO_HOST_BINARY;
  const command =
    installedHost === undefined ? "cargo" : resolve(installedHost);
  const hostArguments =
    installedHost === undefined
      ? ["run", "-q", "-p", "loading-bay-game", "--bin", "browser-host", "--"]
      : ["--dist", resolve(dirname(command), "../lib/Loading Bay/web")];
  const host = spawn(
    command,
    [
      ...hostArguments,
      "--addr",
      address,
      "--project",
      project,
      "--save-root",
      saveRoot,
    ],
    {
      cwd: installedHost === undefined ? repoRoot : proofDirectory,
      env:
        installedHost === undefined
          ? process.env
          : {
              HOME: process.env.HOME,
              LANG: process.env.LANG ?? "C.UTF-8",
              PATH: "/usr/bin:/bin",
            },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  let output = "";
  host.stdout.on("data", (chunk) => {
    output += String(chunk);
  });
  host.stderr.on("data", (chunk) => {
    output += String(chunk);
  });
  return { host, address, output: () => output };
}

async function stopHost(host) {
  await terminateProcess(host);
}

async function reservePort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (address === null || typeof address === "string") {
    server.close();
    throw new Error("could not reserve a browser-smoke port");
  }
  const { port } = address;
  await new Promise((resolveClose, reject) =>
    server.close((error) =>
      error === undefined ? resolveClose() : reject(error),
    ),
  );
  return port;
}

async function waitForHealth(url, process, output) {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    if (process.exitCode !== null) {
      throw new Error(
        `browser host exited early (${String(process.exitCode)})\n${output()}`,
      );
    }
    try {
      const response = await fetch(url);
      const health = await response.json();
      if (
        response.ok &&
        response.headers.get("x-den-project") === "rusty-engine-demo" &&
        health?.project === "rusty-engine-demo" &&
        health?.status === "ok"
      ) {
        return;
      }
    } catch {
      // Compilation and listener startup can take a moment on a clean checkout.
    }
    await delay(100);
  }
  throw new Error(`browser host did not become healthy\n${output()}`);
}

function run(command, args) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, {
      cwd: repoRoot,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr += String(chunk);
    });
    child.once("error", reject);
    child.once("exit", (code) => resolveRun({ code, stdout, stderr }));
  });
}

async function runChromiumSmoke(
  url,
  completionExpression,
  timeout,
  options = {},
) {
  const debuggingPort = await reservePort();
  const ownsProfile = options.profileDirectory === undefined;
  const profileDirectory =
    options.profileDirectory ??
    mkdtempSync(join(tmpdir(), "rusty-engine-chromium-"));
  const browser = spawn(
    chromium,
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      // The product is a foreground desktop surface. Headless Chromium may
      // otherwise classify its only target as background/occluded and throttle
      // both requestAnimationFrame and the normal held-input scheduler.
      "--disable-background-timer-throttling",
      "--disable-backgrounding-occluded-windows",
      "--disable-renderer-backgrounding",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--enable-unsafe-swiftshader",
      "--autoplay-policy=no-user-gesture-required",
      `--remote-debugging-port=${String(debuggingPort)}`,
      "--remote-debugging-address=127.0.0.1",
      "--remote-allow-origins=*",
      `--user-data-dir=${profileDirectory}`,
      "about:blank",
    ],
    {
      cwd: repoRoot,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  let stderr = "";
  browser.stdout.on("data", (chunk) => {
    stderr += String(chunk);
  });
  browser.stderr.on("data", (chunk) => {
    stderr += String(chunk);
  });

  let client;
  try {
    const target = await waitForChromiumTarget(
      debuggingPort,
      browser,
      () => stderr,
    );
    client = await connectDevTools(target.webSocketDebuggerUrl);
    await client.send("Page.enable");
    await client.send("Runtime.enable");
    if (options.viewport !== undefined) {
      await client.send("Emulation.setDeviceMetricsOverride", {
        width: options.viewport.width,
        height: options.viewport.height,
        deviceScaleFactor: 1,
        mobile: options.viewport.width < 600,
      });
    }
    await client.send("Page.navigate", { url });
    if (options.interactiveSetup !== undefined) {
      await options.interactiveSetup(client);
    }
    if (options.setupExpression !== undefined) {
      const navigationDeadline = Date.now() + 10_000;
      let navigationReady = false;
      while (Date.now() < navigationDeadline) {
        try {
          const ready = await client.send("Runtime.evaluate", {
            expression: `location.href === ${JSON.stringify(url)} && document.readyState !== "loading"`,
            returnByValue: true,
          });
          if (ready?.result?.value === true) {
            navigationReady = true;
            break;
          }
        } catch {
          // Navigation replaces the JavaScript execution context.
        }
        await delay(50);
      }
      if (!navigationReady) {
        throw new Error(`Chromium did not finish navigation to ${url}`);
      }
      await client.send("Runtime.evaluate", {
        expression: options.setupExpression,
        awaitPromise: true,
        returnByValue: true,
      });
    }

    const deadline = Date.now() + timeout;
    let completed = false;
    while (Date.now() < deadline) {
      if (browser.exitCode !== null) {
        throw new Error(
          `Chromium exited early (${String(browser.exitCode)})\n${stderr.slice(-4_000)}`,
        );
      }
      const result = await client.send("Runtime.evaluate", {
        expression: completionExpression,
        returnByValue: true,
      });
      if (result?.result?.value === true) {
        completed = true;
        break;
      }
      await delay(50);
    }
    const htmlResult = await client.send("Runtime.evaluate", {
      expression: "document.documentElement.outerHTML",
      returnByValue: true,
    });
    const stdout =
      typeof htmlResult?.result?.value === "string"
        ? htmlResult.result.value
        : "";
    return {
      code: completed ? 0 : 1,
      stdout,
      stderr: completed
        ? stderr
        : `${stderr}\nTimed out waiting for ${completionExpression}`,
    };
  } finally {
    client?.close();
    await terminateProcess(browser);
    if (ownsProfile) {
      removeChromiumProfile(profileDirectory);
    }
  }
}

async function waitForCdp(client, expression, label, timeout = 15_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      const result = await client.send("Runtime.evaluate", {
        expression,
        returnByValue: true,
      });
      if (result?.result?.value === true) {
        return;
      }
    } catch {
      // Navigation can replace the execution context while the route mounts.
    }
    await delay(50);
  }
  throw new Error(`timed out waiting for ${label}`);
}

async function waitForChromiumTarget(port, process, output) {
  const deadline = Date.now() + CHROMIUM_STARTUP_TIMEOUT_MILLISECONDS;
  while (Date.now() < deadline) {
    if (process.exitCode !== null) {
      throw new Error(
        `Chromium exited before debugging was ready (${String(process.exitCode)})\n${output()}`,
      );
    }
    try {
      const response = await fetch(
        `http://127.0.0.1:${String(port)}/json/list`,
      );
      const targets = await response.json();
      const target = Array.isArray(targets)
        ? targets.find(
            (candidate) =>
              candidate?.type === "page" &&
              typeof candidate.webSocketDebuggerUrl === "string",
          )
        : undefined;
      if (target !== undefined) {
        return target;
      }
    } catch {
      // Chromium takes a moment to publish its debugging target.
    }
    await delay(50);
  }
  throw new Error(
    `Chromium debugging target did not become ready\n${output()}`,
  );
}

function connectDevTools(url) {
  return new Promise((resolveConnect, rejectConnect) => {
    const socket = new WebSocket(url);
    const pending = new Map();
    let nextId = 0;
    socket.addEventListener("open", () => {
      resolveConnect({
        send(method, params = {}) {
          nextId += 1;
          const id = nextId;
          return new Promise((resolveCommand, rejectCommand) => {
            pending.set(id, { resolveCommand, rejectCommand });
            socket.send(JSON.stringify({ id, method, params }));
          });
        },
        close() {
          socket.close();
        },
      });
    });
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data));
      if (typeof message.id !== "number") {
        return;
      }
      const command = pending.get(message.id);
      if (command === undefined) {
        return;
      }
      pending.delete(message.id);
      if (message.error === undefined) {
        command.resolveCommand(message.result);
      } else {
        command.rejectCommand(
          new Error(
            `Chromium debugging command failed: ${JSON.stringify(message.error)}`,
          ),
        );
      }
    });
    socket.addEventListener("error", rejectConnect);
    socket.addEventListener("close", () => {
      for (const command of pending.values()) {
        command.rejectCommand(
          new Error("Chromium debugging connection closed"),
        );
      }
      pending.clear();
    });
  });
}

function onceExit(process) {
  if (process.exitCode !== null || process.signalCode !== null) {
    return Promise.resolve();
  }
  return new Promise((resolveExit) => process.once("exit", resolveExit));
}

async function terminateProcess(process) {
  if (process.exitCode !== null || process.signalCode !== null) {
    return;
  }
  process.kill("SIGTERM");
  await Promise.race([onceExit(process), delay(1_000)]);
  if (process.exitCode === null && process.signalCode === null) {
    process.kill("SIGKILL");
    await onceExit(process);
  }
}

function removeChromiumProfile(profileDirectory) {
  // Chromium descendants can briefly finish writing profile state after the
  // root browser process exits. Let recursive removal retry that bounded race.
  rmSync(profileDirectory, {
    recursive: true,
    force: true,
    maxRetries: 12,
    retryDelay: 50,
  });
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}
