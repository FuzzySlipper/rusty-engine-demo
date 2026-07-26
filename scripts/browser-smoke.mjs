import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const chromium = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
if (!existsSync(chromium)) {
  throw new Error(`Chromium is required for the product smoke (${chromium} not found)`);
}

const bundleDirectory = resolve(repoRoot, "ts/packages/browser-shell/dist/assets");
const browserBundle = readdirSync(bundleDirectory)
  .filter((name) => name.endsWith(".js"))
  .map((name) => readFileSync(resolve(bundleDirectory, name), "utf8"))
  .join("\n");
const forbiddenRuntimeSurface = [
  "GameplayRuntimeHost",
  "GameplayFabric",
  "NativeRuntimeBridge",
  "RuntimeSession",
  "VoxelConversionRequest",
  "rusty-engine.mesh-to-voxel",
  "voxel-convert",
  "planVoxelConversion",
  "previewVoxelConversion",
  "applyVoxelConversion",
  "VoxelReplayRecord",
  "GenericAssetProvider",
  "ProjectBundleFacade",
];
const bundledRuntimeSurface = forbiddenRuntimeSurface.filter((name) => browserBundle.includes(name));
if (bundledRuntimeSurface.length > 0) {
  throw new Error(`browser bundle imported old runtime surface: ${bundledRuntimeSurface.join(", ")}`);
}

const proofDirectory = mkdtempSync(join(tmpdir(), "rusty-engine-browser-smoke-"));
try {
  const persistedProject = resolve(proofDirectory, "loading-bay.project.json");
  const convertedProject = resolve(proofDirectory, "converted-wall.project.json");
  const migratedProject = resolve(proofDirectory, "migrated-v6.project.json");
  const currentReceipt = await persistProject(
    resolve(repoRoot, "content/projects/loading-bay.project.json"),
    persistedProject,
  );
  if (!currentReceipt.includes("sourceSchema=11") || !currentReceipt.includes("currentSchema=11")) {
    throw new Error(`current project persistence receipt was incomplete\n${currentReceipt}`);
  }
  await runFullBrowserProduct(persistedProject);
  await runPersistedVoxelEditProduct(persistedProject);

  const convertedReceipt = await persistProject(
    resolve(repoRoot, "content/projects/converted-wall.project.json"),
    convertedProject,
  );
  if (!convertedReceipt.includes("sourceSchema=11") || !convertedReceipt.includes("currentSchema=11")) {
    throw new Error(`converted project persistence receipt was incomplete\n${convertedReceipt}`);
  }
  await runConvertedBrowserProduct(convertedProject);
  await runPersistedConvertedVoxelEditProduct(convertedProject);

  const migrationReceipt = await persistProject(
    resolve(repoRoot, "content/generated/encounter-gate.project.json"),
    migratedProject,
  );
  if (!migrationReceipt.includes("sourceSchema=6") || !migrationReceipt.includes("currentSchema=11")) {
    throw new Error(`migration receipt was incomplete\n${migrationReceipt}`);
  }
  await runMigratedBrowserProduct(migratedProject);

  console.log(
    "browser smoke passed: persisted projects + converted asset + v6 migration -> accepted gameplay -> shared Rusty Engine retained renderer + shared disposable hosts -> fresh-page posture rebuild",
  );
} finally {
  rmSync(proofDirectory, { recursive: true, force: true });
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
    throw new Error(`project-store exited ${String(result.code)}\n${result.stderr}`);
  }
  return result.stdout;
}

async function runFullBrowserProduct(project) {
  const running = await launchHost(project);
  try {
    await waitForHealth(`http://${running.address}/health`, running.host, running.output);
    const result = await run(chromium, [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--enable-unsafe-swiftshader",
      "--autoplay-policy=no-user-gesture-required",
      "--virtual-time-budget=20000",
      "--dump-dom",
      `http://${running.address}/?smoke=1`,
    ]);
    if (result.code !== 0) {
      throw new Error(`Chromium exited ${String(result.code)}\n${result.stderr.slice(-4_000)}`);
    }
    const required = [
      'data-smoke-status="pass"',
      'data-status="pass"',
      'data-held-input="pass"',
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
      "PASS · Rust facts reached retained WebGL and disposable feedback",
      "EnemyDefeated",
      "EncounterCleared",
      "DoorOpened",
      "KinematicBlocked",
      "NavigationArrived",
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
    const missing = required.filter((marker) => !result.stdout.includes(marker));
    if (missing.length > 0) {
      throw new Error(
        `browser smoke missing ${missing.join(", ")}\n${result.stdout.slice(-6_000)}`,
      );
    }
    const beforeReloadResponse = await fetch(`http://${running.address}/api/state`);
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
    const reloadResult = await run(chromium, [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--enable-unsafe-swiftshader",
      "--virtual-time-budget=10000",
      "--dump-dom",
      `http://${running.address}/?reload-smoke=1`,
    ]);
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
    const afterReloadResponse = await fetch(`http://${running.address}/api/state`);
    const afterReload = await afterReloadResponse.json();
    if (
      !afterReloadResponse.ok ||
      JSON.stringify(afterReload) !== JSON.stringify(beforeReload)
    ) {
      throw new Error(
        `browser reload changed authoritative state\nbefore=${JSON.stringify(beforeReload)}\nafter=${JSON.stringify(afterReload)}`,
      );
    }
    const startup = running.output();
    for (const marker of [
      "project id=loading-bay",
      "sourceSchema=11",
      "currentSchema=11",
      "entryScene=scene/loading-bay",
      "assets=7",
      "scenes=1",
      "entities=8",
    ]) {
      if (!startup.includes(marker)) {
        throw new Error(`browser host startup missing ${marker}\n${startup}`);
      }
    }
  } finally {
    await stopHost(running.host);
  }
}

async function runMigratedBrowserProduct(project) {
  const running = await launchHost(project);
  try {
    await waitForHealth(`http://${running.address}/health`, running.host, running.output);
    const stateResponse = await fetch(`http://${running.address}/api/state`);
    const state = await stateResponse.json();
    if (
      !stateResponse.ok ||
      state.generatedEnvironment?.seed !== 4 ||
      state.enemies?.length !== 2 ||
      state.weapon?.ammoRemaining !== 8 ||
      !state.projection?.some((node) => node.id === 3)
    ) {
      throw new Error(`migrated browser state was incomplete\n${JSON.stringify(state)}`);
    }
    const attackResponse = await fetch(`http://${running.address}/api/attack`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ kind: "attack" }),
    });
    const attacked = await attackResponse.json();
    if (!attackResponse.ok || attacked.tick !== 1 || attacked.weapon?.ammoRemaining !== 7) {
      throw new Error(`migrated browser action failed\n${JSON.stringify(attacked)}`);
    }
    const startup = running.output();
    for (const marker of [
      "project id=migrated-v6-project",
      "currentSchema=11",
      "assets=4",
      "scenes=1",
      "entities=6",
    ]) {
      if (!startup.includes(marker)) {
        throw new Error(`migrated browser startup missing ${marker}\n${startup}`);
      }
    }
  } finally {
    await stopHost(running.host);
  }
}

async function runConvertedBrowserProduct(project) {
  const running = await launchHost(project);
  try {
    await waitForHealth(`http://${running.address}/health`, running.host, running.output);
    const result = await run(chromium, [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--enable-unsafe-swiftshader",
      "--virtual-time-budget=10000",
      "--dump-dom",
      `http://${running.address}/?converted-smoke=1`,
    ]);
    if (result.code !== 0) {
      throw new Error(`converted Chromium exited ${String(result.code)}\n${result.stderr.slice(-4_000)}`);
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
    const missing = required.filter((marker) => !result.stdout.includes(marker));
    if (missing.length > 0) {
      throw new Error(
        `converted browser smoke missing ${missing.join(", ")}\n${result.stdout.slice(-8_000)}`,
      );
    }
    const startup = running.output();
    for (const marker of [
      "project id=converted-wall",
      "sourceSchema=11",
      "currentSchema=11",
      "entryScene=scene/converted-wall",
      "assets=10",
      "scenes=1",
      "entities=7",
    ]) {
      if (!startup.includes(marker)) {
        throw new Error(`converted browser startup missing ${marker}\n${startup}`);
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
    await waitForHealth(`http://${running.address}/health`, running.host, running.output);
    const beforeResponse = await fetch(`http://${running.address}/api/state`);
    const before = await beforeResponse.json();
    if (
      !beforeResponse.ok ||
      before.voxelRevision !== 0 ||
      before.voxelSolidCount !== 94 ||
      before.voxelProbePathLength !== 9 ||
      before.generatedEnvironment !== null
    ) {
      throw new Error(`converted persisted-edit baseline was incomplete\n${JSON.stringify(before)}`);
    }
    const editResponse = await fetch(`http://${running.address}/api/voxel-edit`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        expectedRevision: before.voxelRevision,
        persistToProject: true,
        edits,
      }),
    });
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
      JSON.stringify(edited.voxelMeshes) === JSON.stringify(before.voxelMeshes) ||
      edited.generatedEnvironment !== null
    ) {
      throw new Error(`converted persisted voxel edit was incomplete\n${JSON.stringify(edited)}`);
    }
    persisted = voxelStateFingerprint(edited);

    const resetResponse = await fetch(`http://${running.address}/api/reset`, { method: "POST" });
    const reset = await resetResponse.json();
    if (
      !resetResponse.ok ||
      reset.voxelRevision !== 0 ||
      reset.voxelEditReceipt !== undefined ||
      reset.lastEvents?.length !== 0 ||
      JSON.stringify(voxelStateFingerprint(reset)) !== JSON.stringify(persisted)
    ) {
      throw new Error(`converted reset did not reopen static edited authority\n${JSON.stringify(reset)}`);
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
    environment.materialVoxels.some((voxel) => removed.has(JSON.stringify(voxel.address))) ||
    (Array.isArray(environment.voxelAssets) && environment.voxelAssets.length !== 0)
  ) {
    throw new Error(`converted saved project did not materialize edited authority\n${bytes}`);
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
      throw new Error(`converted saved project leaked transient field ${forbidden}`);
    }
  }

  const reopened = await launchHost(project);
  try {
    await waitForHealth(`http://${reopened.address}/health`, reopened.host, reopened.output);
    const response = await fetch(`http://${reopened.address}/api/state`);
    const state = await response.json();
    if (
      !response.ok ||
      state.voxelRevision !== 0 ||
      JSON.stringify(voxelStateFingerprint(state)) !== JSON.stringify(persisted)
    ) {
      throw new Error(`fresh host did not reopen converted edited authority\n${JSON.stringify(state)}`);
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
    meshes: state.voxelMeshes,
    generatedEnvironment: state.generatedEnvironment,
  };
}

async function runPersistedVoxelEditProduct(project) {
  const running = await launchHost(project);
  let persistedHash;
  let persistedNavigationHash;
  let persistedPathLength;
  try {
    await waitForHealth(`http://${running.address}/health`, running.host, running.output);
    const beforeResponse = await fetch(`http://${running.address}/api/state`);
    const before = await beforeResponse.json();
    const editResponse = await fetch(`http://${running.address}/api/voxel-edit`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        expectedRevision: before.voxelRevision,
        persistToProject: true,
        edits: [{ kind: "clear", address: [4, 1, 6] }],
      }),
    });
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
      throw new Error(`persisted voxel edit response was incomplete\n${JSON.stringify(edited)}`);
    }
    persistedHash = edited.voxelAuthorityHash;
    persistedNavigationHash = edited.voxelNavigationHash;
    persistedPathLength = edited.voxelProbePathLength;

    const resetResponse = await fetch(`http://${running.address}/api/reset`, { method: "POST" });
    const reset = await resetResponse.json();
    if (
      !resetResponse.ok ||
      reset.voxelRevision !== 0 ||
      reset.voxelAuthorityHash !== persistedHash ||
      reset.voxelNavigationHash !== persistedNavigationHash ||
      reset.voxelProbePathLength !== persistedPathLength ||
      reset.generatedEnvironment !== null ||
      reset.voxelEditReceipt !== undefined ||
      reset.lastEvents?.length !== 0
    ) {
      throw new Error(`persisted voxel reset did not reopen static authority\n${JSON.stringify(reset)}`);
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
    environment.materialVoxels.some((voxel) =>
      JSON.stringify(voxel.address) === JSON.stringify([4, 1, 6]))
  ) {
    throw new Error(`saved project did not materialize the accepted edit\n${bytes}`);
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
    await waitForHealth(`http://${reopened.address}/health`, reopened.host, reopened.output);
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
      throw new Error(`fresh host did not reopen persisted voxel authority\n${JSON.stringify(state)}`);
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
    server.close((error) => (error === undefined ? resolveClose() : reject(error))),
  );
  return port;
}

async function waitForHealth(url, process, output) {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    if (process.exitCode !== null) {
      throw new Error(`browser host exited early (${String(process.exitCode)})\n${output()}`);
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
    const child = spawn(command, args, { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] });
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

function onceExit(process) {
  if (process.exitCode !== null) {
    return Promise.resolve();
  }
  return new Promise((resolveExit) => process.once("exit", resolveExit));
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}
