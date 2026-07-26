import { spawn } from "node:child_process";
import {
  existsSync,
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
  const currentReceipt = await persistProject(
    resolve(repoRoot, "content/projects/loading-bay.project.json"),
    persistedProject,
  );
  if (
    !currentReceipt.includes("sourceSchema=15") ||
    !currentReceipt.includes("currentSchema=15")
  ) {
    throw new Error(
      `current project persistence receipt was incomplete\n${currentReceipt}`,
    );
  }
  await runFullBrowserProduct(persistedProject);
  await runPersistedVoxelEditProduct(persistedProject);

  const convertedReceipt = await persistProject(
    resolve(repoRoot, "content/projects/converted-wall.project.json"),
    convertedProject,
  );
  if (
    !convertedReceipt.includes("sourceSchema=11") ||
    !convertedReceipt.includes("currentSchema=15")
  ) {
    throw new Error(
      `converted project persistence receipt was incomplete\n${convertedReceipt}`,
    );
  }
  await runConvertedBrowserProduct(convertedProject);
  await runPersistedConvertedVoxelEditProduct(convertedProject);

  const migrationReceipt = await persistProject(
    resolve(repoRoot, "content/generated/encounter-gate.project.json"),
    migratedProject,
  );
  if (
    !migrationReceipt.includes("sourceSchema=6") ||
    !migrationReceipt.includes("currentSchema=15")
  ) {
    throw new Error(`migration receipt was incomplete\n${migrationReceipt}`);
  }
  await runMigratedBrowserProduct(migratedProject);

  console.log(
    "browser smoke passed: persisted projects + converted asset + v6 migration -> accepted gameplay -> shared Rusty Engine retained renderer + shared disposable hosts -> fresh-page posture rebuild",
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
  const expectedEntityCount = storedProjectEntityCount(project);
  const running = await launchHost(project);
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const result = await runChromiumSmoke(
      `http://${running.address}/?smoke=1`,
      "document.body?.dataset.smokeStatus === 'pass' || document.body?.dataset.smokeStatus === 'fail'",
      60_000,
    );
    if (result.code !== 0) {
      throw new Error(
        `Chromium exited ${String(result.code)}\n${result.stderr.slice(-4_000)}`,
      );
    }
    const required = [
      'data-smoke-status="pass"',
      'data-status="pass"',
      'data-held-input="pass"',
      'data-local-look-offset="pass"',
      'data-local-look-presentation="bounded-disposable"',
      'data-pickups="pass"',
      'data-gate-passage="pass"',
      'data-queue-recovery="pass"',
      'data-cooldown="pass"',
      'data-beacon-activation="pass"',
      'data-feedback-reset="pass"',
      'data-feedback-concrete-reset="pass"',
      'data-feedback-families="pass"',
      'data-audio-feedback="pass"',
      'data-feedback-drop="pass"',
      'data-feedback-concrete-restart="pass"',
      'data-voxel-edit="pass"',
      'data-voxel-rejection="pass"',
      'data-voxel-collision="pass"',
      'data-session-transport="pass"',
      'data-session-protocol="1"',
      'data-session-pending-outbound-max="1"',
      'data-session-dropped-facts="0"',
      'data-session-pending-input="0"',
      'data-session-pending-edges="0"',
      'data-renderer-telemetry="pass"',
      'data-renderer-single-loop="pass"',
      'data-renderer-telemetry-refresh="pass"',
      'data-renderer-telemetry-reset="pass"',
      "PASS · Rust facts reached retained WebGL and disposable feedback",
      "EnemyDefeated",
      "EncounterCleared",
      "DoorOpened",
      "NavigationAdvanced",
      "PlayerMoved",
      "PlayerBlocked",
      "PlayerLookChanged",
      "CombatHit",
      "DamageApplied",
      "CombatEnemyDefeated",
      "CombatRejected",
      "ExtractionBeaconActivated",
      "SEED 4",
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
    };
    if (
      sessionEvidence.legacyBytes <= 0 ||
      sessionEvidence.bootstrapBytes <= 0 ||
      sessionEvidence.staticUpdates <= 0 ||
      sessionEvidence.staticMaxBytes <= sessionEvidence.steadyMaxBytes ||
      sessionEvidence.steadyBytes >= sessionEvidence.legacyBytes / 2 ||
      sessionEvidence.steadyMaxBytes >= sessionEvidence.legacyBytes / 2 ||
      sessionEvidence.pendingOutboundMax !== 1 ||
      sessionEvidence.pendingInputMax > 2 ||
      sessionEvidence.pendingEdgesMax > 32 ||
      sessionEvidence.droppedFacts !== 0 ||
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
      rendererEvidence.residentChunkCount <= 0 ||
      rendererEvidence.renderDiffCount < 0
    ) {
      throw new Error(
        `shared-renderer telemetry was not live\n${JSON.stringify(rendererEvidence)}`,
      );
    }
    console.log(
      `shared-renderer correctness proof ${JSON.stringify(rendererEvidence)} (headless SwiftShader; not GPU performance evidence)`,
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
      beforeReload.presentation?.cues?.length !== 0
    ) {
      throw new Error(
        `browser reload baseline was not retained defeated/open authority\n${JSON.stringify(beforeReload)}`,
      );
    }
    const reloadResult = await runChromiumSmoke(
      `http://${running.address}/?reload-smoke=1`,
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
      `http://${running.address}/?lifecycle-smoke=1`,
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
    const startup = running.output();
    for (const marker of [
      "project id=loading-bay",
      "sourceSchema=15",
      "currentSchema=15",
      "entryScene=scene/loading-bay",
      "assets=13",
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

async function runMigratedBrowserProduct(project) {
  const running = await launchHost(project);
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
      state.enemies?.length !== 2 ||
      state.weapon?.ammoRemaining !== 8 ||
      !state.projection?.some((node) => node.id === 3)
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
      "currentSchema=15",
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
  const running = await launchHost(project);
  try {
    await waitForHealth(
      `http://${running.address}/health`,
      running.host,
      running.output,
    );
    const result = await runChromiumSmoke(
      `http://${running.address}/?converted-smoke=1`,
      "document.body?.dataset.smokeStatus === 'pass' || document.body?.dataset.smokeStatus === 'fail'",
      30_000,
    );
    if (result.code !== 0) {
      throw new Error(
        `converted Chromium exited ${String(result.code)}\n${result.stderr.slice(-4_000)}\n${result.stdout.slice(-8_000)}`,
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
      "sourceSchema=15",
      "currentSchema=15",
      "entryScene=scene/converted-wall",
      "assets=10",
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
          edits: [{ kind: "clear", address: [4, 1, 6] }],
        }),
      },
    );
    const edited = await editResponse.json();
    if (
      !editResponse.ok ||
      edited.voxelEditReceipt?.persistedToProject !== true ||
      edited.voxelEditReceipt?.changedVoxels !== 1 ||
      edited.voxelRevision !== 1 ||
      edited.voxelSolidCount !== before.voxelSolidCount - 1 ||
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
    environment.materialVoxels.some(
      (voxel) => JSON.stringify(voxel.address) === JSON.stringify([4, 1, 6]),
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

async function launchHost(project) {
  const port = await reservePort();
  const address = `127.0.0.1:${String(port)}`;
  const host = spawn(
    "cargo",
    [
      "run",
      "-q",
      "-p",
      "loading-bay-game",
      "--bin",
      "browser-host",
      "--",
      "--addr",
      address,
      "--project",
      project,
    ],
    { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
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
  host.kill("SIGTERM");
  await Promise.race([onceExit(host), delay(1_000)]);
  if (host.exitCode === null) {
    host.kill("SIGKILL");
  }
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

async function runChromiumSmoke(url, completionExpression, timeout) {
  const debuggingPort = await reservePort();
  const profileDirectory = mkdtempSync(
    join(tmpdir(), "rusty-engine-chromium-"),
  );
  const browser = spawn(
    chromium,
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
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
    await client.send("Page.navigate", { url });

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
    browser.kill("SIGTERM");
    await Promise.race([onceExit(browser), delay(1_000)]);
    if (browser.exitCode === null) {
      browser.kill("SIGKILL");
    }
    rmSync(profileDirectory, { recursive: true, force: true });
  }
}

async function waitForChromiumTarget(port, process, output) {
  const deadline = Date.now() + 15_000;
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
  if (process.exitCode !== null) {
    return Promise.resolve();
  }
  return new Promise((resolveExit) => process.once("exit", resolveExit));
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}
