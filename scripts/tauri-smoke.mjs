import { execFileSync, spawn } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import process from "node:process";

const repoRoot = resolve(dirname(new URL(import.meta.url).pathname), "..");
const application = resolve(
  repoRoot,
  process.env.TAURI_APPLICATION ?? "target/release/loading-bay-desktop",
);
const driverBinary =
  process.env.TAURI_DRIVER_BIN ?? "/home/agent/.cargo/bin/tauri-driver";
const nativeDriver =
  process.env.TAURI_NATIVE_DRIVER ?? "/usr/bin/WebKitWebDriver";
const driverPort = Number(process.env.TAURI_DRIVER_PORT ?? 4444);
const nativePort = Number(process.env.TAURI_NATIVE_DRIVER_PORT ?? 4445);
const evidencePath = resolve(
  repoRoot,
  process.env.TAURI_SMOKE_EVIDENCE ?? "target/tauri-smoke-evidence.json",
);
const screenshotPath = resolve(
  repoRoot,
  process.env.TAURI_SMOKE_SCREENSHOT ?? "target/tauri-smoke.png",
);
const narrowScreenshotPath = resolve(
  repoRoot,
  process.env.TAURI_SMOKE_NARROW_SCREENSHOT ?? "target/tauri-smoke-narrow.png",
);
const temporaryRoot = mkdtempSync(join(tmpdir(), "loading-bay-tauri-smoke-"));
const xdg = {
  XDG_DATA_HOME: join(temporaryRoot, "data"),
  XDG_CACHE_HOME: join(temporaryRoot, "cache"),
  XDG_CONFIG_HOME: join(temporaryRoot, "config"),
  XDG_STATE_HOME: join(temporaryRoot, "state"),
};
for (const directory of Object.values(xdg)) {
  mkdirSync(directory, { recursive: true });
}
const seedSaveRoot = process.env.TAURI_SMOKE_SEED_SAVE_ROOT;
if (seedSaveRoot !== undefined) {
  cpSync(
    resolve(seedSaveRoot),
    join(
      xdg.XDG_DATA_HOME,
      "dev.fuzzyslipper.rusty-engine-demo.loading-bay/saves",
    ),
    { recursive: true, force: true },
  );
}

const output = [];
const windowManager = process.env.TAURI_WINDOW_MANAGER;
if (windowManager && !existsSync(windowManager)) {
  throw new Error(`Tauri window manager does not exist: ${windowManager}`);
}
const driverCommand = process.env.DISPLAY
  ? driverBinary
  : (process.env.XVFB_RUN ?? "xvfb-run");
const driverArguments = [
  ...(process.env.DISPLAY
    ? []
    : windowManager
      ? [
          "-a",
          "sh",
          "-c",
          'window_manager="$1"; window_manager_log="$2"; shift 2; "$window_manager" >"$window_manager_log" 2>&1 & exec "$@"',
          "tauri-native-session",
          windowManager,
          join(temporaryRoot, "window-manager.log"),
          driverBinary,
        ]
      : ["-a", driverBinary]),
  "--port",
  String(driverPort),
  "--native-port",
  String(nativePort),
  "--native-driver",
  nativeDriver,
];
const driver = spawn(driverCommand, driverArguments, {
  cwd: temporaryRoot,
  env: { ...process.env, ...xdg },
  detached: true,
  stdio: ["ignore", "pipe", "pipe"],
});
driver.stdout.on("data", (chunk) => output.push(`stdout ${chunk}`));
driver.stderr.on("data", (chunk) => output.push(`stderr ${chunk}`));

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

async function waitFor(predicate, label, timeout = 30_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await predicate();
    if (value) return value;
    await delay(250);
  }
  throw new Error(`timed out waiting for ${label}`);
}

async function request(path, options = {}) {
  const response = await fetch(`http://127.0.0.1:${driverPort}${path}`, {
    ...options,
    headers: {
      "content-type": "application/json",
      ...(options.headers ?? {}),
    },
  });
  const text = await response.text();
  const body = text ? JSON.parse(text) : null;
  if (!response.ok || (body?.value?.error && body.value.message)) {
    throw new Error(
      `WebDriver ${options.method ?? "GET"} ${path} failed (${response.status}): ${text}`,
    );
  }
  return body?.value;
}

async function createSession(applicationPath = application) {
  const value = await request("/session", {
    method: "POST",
    body: JSON.stringify({
      capabilities: {
        alwaysMatch: {
          "tauri:options": {
            application: applicationPath,
          },
        },
      },
    }),
  });
  const sessionId = value?.sessionId;
  if (typeof sessionId !== "string" || sessionId.length === 0) {
    throw new Error(
      `WebDriver returned no session id: ${JSON.stringify(value)}`,
    );
  }
  await request(`/session/${sessionId}/timeouts`, {
    method: "POST",
    body: JSON.stringify({ script: 300_000, pageLoad: 120_000 }),
  });
  return sessionId;
}

async function executeAsync(sessionId, script) {
  return request(`/session/${sessionId}/execute/async`, {
    method: "POST",
    body: JSON.stringify({ script, args: [] }),
  });
}

async function execute(sessionId, script) {
  return request(`/session/${sessionId}/execute/sync`, {
    method: "POST",
    body: JSON.stringify({ script, args: [] }),
  });
}

async function deleteSession(sessionId) {
  try {
    await request(`/session/${sessionId}`, { method: "DELETE" });
  } catch (error) {
    output.push(`delete-session ${error}`);
  }
}

async function nativeInputProof(sessionId) {
  const before = await executeAsync(
    sessionId,
    `const done = arguments[arguments.length - 1];
     fetch("/api/state").then((response) => response.json()).then(
       (state) => done({ position: state.player.position, yaw: state.player.yaw }),
       (error) => done({ error: String(error) }),
     );`,
  );
  if (before?.error) throw new Error(before.error);
  const element = await request(`/session/${sessionId}/element`, {
    method: "POST",
    body: JSON.stringify({ using: "css selector", value: "#viewport" }),
  });
  const elementId = element?.["element-6066-11e4-a52e-4f735466cecf"];
  if (typeof elementId !== "string") {
    throw new Error(
      `WebDriver returned no viewport element: ${JSON.stringify(element)}`,
    );
  }
  await request(`/session/${sessionId}/element/${elementId}/click`, {
    method: "POST",
    body: "{}",
  });
  await waitFor(
    () =>
      execute(
        sessionId,
        `return document.pointerLockElement?.id === "viewport";`,
      ),
    "native WebKit pointer lock",
  );
  await request(`/session/${sessionId}/actions`, {
    method: "POST",
    body: JSON.stringify({
      actions: [
        {
          type: "key",
          id: "keyboard",
          actions: [
            { type: "keyDown", value: "w" },
            { type: "pause", duration: 400 },
            { type: "keyUp", value: "w" },
          ],
        },
      ],
    }),
  });
  const after = await waitFor(async () => {
    const value = await executeAsync(
      sessionId,
      `const done = arguments[arguments.length - 1];
       fetch("/api/state").then((response) => response.json()).then(
         (state) => done({ position: state.player.position, yaw: state.player.yaw }),
         (error) => done({ error: String(error) }),
       );`,
    );
    return value?.error === undefined &&
      JSON.stringify(value.position) !== JSON.stringify(before.position)
      ? value
      : null;
  }, "native keyboard movement");
  await request(`/session/${sessionId}/actions`, {
    method: "DELETE",
  });
  return {
    pointerLock: true,
    mouseActivatedPointerLock: true,
    keyboardMovedPlayer: true,
    before,
    after,
  };
}

async function closeProductSession(sessionId) {
  try {
    await Promise.race([
      executeAsync(
        sessionId,
        `const done = arguments[arguments.length - 1];
         window.__TAURI_INTERNALS__.invoke("plugin:window|close", { label: "main" }).then(
           () => done({ error: null }),
           (error) => done({ error: String(error) }),
         );`,
      ),
      delay(5_000),
    ]).catch(() => undefined);
  } finally {
    await deleteSession(sessionId);
  }
}

function processExists(pid, expectedExecutable) {
  try {
    if (basename(readlinkSync(`/proc/${pid}/exe`)) !== expectedExecutable) {
      return false;
    }
  } catch (error) {
    if (error?.code === "ENOENT") return false;
  }
  try {
    const state = readFileSync(`/proc/${pid}/stat`, "utf8")
      .split(" ")[2]
      ?.trim();
    if (state === "Z") return false;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
  }
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error?.code === "ESRCH") return false;
    throw error;
  }
}

function latestReady() {
  const directory = join(
    xdg.XDG_CACHE_HOME,
    "dev.fuzzyslipper.rusty-engine-demo.loading-bay",
  );
  const entries = readdirSync(directory)
    .filter((name) => /^host-ready-\d+\.json$/.test(name))
    .map((name) => {
      const path = join(directory, name);
      return { path, modified: statSync(path).mtimeMs };
    })
    .sort((left, right) => right.modified - left.modified);
  if (entries.length === 0) {
    throw new Error("Tauri application did not publish host readiness");
  }
  const ready = JSON.parse(readFileSync(entries[0].path, "utf8"));
  return {
    ...ready,
    shellPid: Number(basename(entries[0].path).match(/\d+/)?.[0]),
  };
}

async function runProductSession({
  startGame,
  continueGame = false,
  measureMenuIdle = false,
}) {
  const sessionStartedAt = performance.now();
  const sessionId = await createSession();
  const menu = await executeAsync(
    sessionId,
    `const done = arguments[arguments.length - 1];
     (async () => {
       const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
       const waitFor = async (predicate, label) => {
         const deadline = Date.now() + 240000;
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
       await waitFor(
         () => document.querySelector("red-main-menu") !== null,
         "Rust-owned main menu",
       );
       return {
         error: null,
         origin: location.origin,
         menu: await fetch("/api/menu-state").then((response) => response.json()),
         securityHeaders: await fetch("/").then((response) => ({
           contentSecurityPolicy: response.headers.get("content-security-policy"),
           referrerPolicy: response.headers.get("referrer-policy"),
           contentTypeOptions: response.headers.get("x-content-type-options"),
         })),
         stylesheetMedia:
           document.querySelector('link[rel="stylesheet"][href^="styles-"]')
             ?.media ?? null,
         userAgent: navigator.userAgent,
         viewport: { width: innerWidth, height: innerHeight, devicePixelRatio },
       };
     })().then(done, (error) => done({ error: String(error?.stack ?? error) }));`,
  );
  if (menu?.error) {
    await deleteSession(sessionId);
    throw new Error(menu.error);
  }
  const menuReadyMilliseconds = performance.now() - sessionStartedAt;
  const ready = await waitFor(() => {
    try {
      return latestReady();
    } catch {
      return null;
    }
  }, "host readiness");
  const menuIdleMeasurement = measureMenuIdle
    ? await measureIdle(ready.shellPid)
    : null;
  let frame = {
    renderSequence: null,
    rendererStatus: null,
    lifecycle: null,
    runtimeError: "",
  };
  if (startGame || continueGame) {
    const gameStartedAt = performance.now();
    const clicked = await execute(
      sessionId,
      `const button = [...document.querySelectorAll("button")].find(
         (element) => element.textContent?.trim() === ${JSON.stringify(continueGame ? "Continue" : "New game")},
       );
       if (!(button instanceof HTMLButtonElement) || button.disabled) return false;
       button.click();
       return true;`,
    );
    if (!clicked) {
      throw new Error(
        `${continueGame ? "Continue" : "New game"} action is unavailable`,
      );
    }
    let framePolls = 0;
    frame = await waitFor(
      async () => {
        const state = await execute(
          sessionId,
          `return {
           renderSequence:
             Number(document.querySelector("#renderer-telemetry")?.dataset.rendererRenderSequence) ||
             null,
           rendererStatus: document.querySelector("#renderer-status")?.textContent?.trim() ?? null,
           lifecycle: document.body.dataset.rendererLifecycle ?? null,
           runtimeError: document.body.dataset.runtimeError ?? "",
           overlay: document.querySelector(".game-state-overlay") !== null,
         };`,
        );
        framePolls += 1;
        if (framePolls % 20 === 0) {
          process.stderr.write(
            `Tauri frame wait lifecycle=${state.lifecycle} sequence=${state.renderSequence} ` +
              `status=${state.rendererStatus} overlay=${state.overlay} error=${state.runtimeError}\n`,
          );
        }
        return state.lifecycle === "mounted" &&
          state.renderSequence > 0 &&
          (continueGame ? state.overlay : !state.overlay)
          ? state
          : null;
      },
      "authoritative rendered game frame",
      240_000,
    );
    frame.firstFrameMilliseconds = performance.now() - gameStartedAt;
    frame.continuedCompletedCampaign = continueGame
      ? await execute(
          sessionId,
          `return document.querySelector(".game-state-overlay")?.textContent?.includes("LOADING BAY COMPLETE") === true;`,
        )
      : false;
    frame.webGl = await execute(
      sessionId,
      `const canvas = document.querySelector("canvas");
       const gl = canvas?.getContext("webgl2");
       const extension = gl?.getExtension("WEBGL_debug_renderer_info");
       return {
         renderer: extension ? gl.getParameter(extension.UNMASKED_RENDERER_WEBGL) : null,
         vendor: extension ? gl.getParameter(extension.UNMASKED_VENDOR_WEBGL) : null,
         version: gl?.getParameter(gl.VERSION) ?? null,
         shadingLanguageVersion: gl?.getParameter(gl.SHADING_LANGUAGE_VERSION) ?? null,
       };`,
    );
    if (!continueGame) {
      frame.nativeInput = await nativeInputProof(sessionId);
    }
  }
  return {
    sessionId,
    result: {
      ...menu,
      ...frame,
      menuReadyMilliseconds,
      menuIdleMeasurement,
    },
    ready,
  };
}

function processTree(rootPid) {
  const processes = new Map();
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      const status = readFileSync(`/proc/${entry}/status`, "utf8");
      const parentPid = Number(status.match(/^PPid:\s+(\d+)/m)?.[1]);
      const residentKiB = Number(
        status.match(/^VmRSS:\s+(\d+)\s+kB/m)?.[1] ?? 0,
      );
      const voluntaryContextSwitches = Number(
        status.match(/^voluntary_ctxt_switches:\s+(\d+)/m)?.[1] ?? 0,
      );
      const involuntaryContextSwitches = Number(
        status.match(/^nonvoluntary_ctxt_switches:\s+(\d+)/m)?.[1] ?? 0,
      );
      const stat = readFileSync(`/proc/${entry}/stat`, "utf8").split(" ");
      const cpuTicks = Number(stat[13] ?? 0) + Number(stat[14] ?? 0);
      const name = status.match(/^Name:\s+(.+)$/m)?.[1]?.trim() ?? "unknown";
      processes.set(Number(entry), {
        parentPid,
        residentKiB,
        voluntaryContextSwitches,
        involuntaryContextSwitches,
        cpuTicks,
        name,
      });
    } catch {
      // A short-lived process may disappear between /proc reads.
    }
  }
  const members = [];
  const pending = [rootPid];
  while (pending.length > 0) {
    const pid = pending.shift();
    const process = processes.get(pid);
    if (process !== undefined) members.push({ pid, ...process });
    for (const [candidatePid, candidate] of processes) {
      if (
        candidate.parentPid === pid &&
        !members.some((item) => item.pid === candidatePid)
      ) {
        pending.push(candidatePid);
      }
    }
  }
  return {
    residentBytes: members.reduce(
      (total, item) => total + item.residentKiB * 1024,
      0,
    ),
    processes: members,
    cpuTicks: members.reduce((total, item) => total + item.cpuTicks, 0),
    contextSwitches: members.reduce(
      (total, item) =>
        total + item.voluntaryContextSwitches + item.involuntaryContextSwitches,
      0,
    ),
  };
}

async function measureIdle(rootPid) {
  const clockTicksPerSecond = Number(
    execFileSync("getconf", ["CLK_TCK"], { encoding: "utf8" }).trim(),
  );
  const startedAt = performance.now();
  const before = processTree(rootPid);
  await delay(2_000);
  const after = processTree(rootPid);
  const elapsedMilliseconds = performance.now() - startedAt;
  const cpuTicks = after.cpuTicks - before.cpuTicks;
  return {
    elapsedMilliseconds,
    cpuMilliseconds: (cpuTicks / clockTicksPerSecond) * 1_000,
    cpuPercentOneCore:
      ((cpuTicks / clockTicksPerSecond) * 100_000) / elapsedMilliseconds,
    contextSwitches: after.contextSwitches - before.contextSwitches,
  };
}

async function exerciseInstalledWindow(first) {
  await request(`/session/${first.sessionId}/window/rect`, {
    method: "POST",
    body: JSON.stringify({ width: 960, height: 540, x: 20, y: 20 }),
  });
  const narrowViewport = await execute(
    first.sessionId,
    `return {
      width: innerWidth,
      height: innerHeight,
      overflowX: document.documentElement.scrollWidth > document.documentElement.clientWidth,
      runtimeError: document.body.dataset.runtimeError ?? "",
      lifecycle: document.body.dataset.rendererLifecycle ?? null,
    };`,
  );
  const narrowScreenshot = await request(
    `/session/${first.sessionId}/screenshot`,
  );
  mkdirSync(dirname(narrowScreenshotPath), { recursive: true });
  writeFileSync(narrowScreenshotPath, Buffer.from(narrowScreenshot, "base64"));

  const remount = await executeAsync(
    first.sessionId,
    `const done = arguments[arguments.length - 1];
     (async () => {
       const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
       const waitFor = async (predicate, label) => {
         const deadline = Date.now() + 30000;
         while (Date.now() < deadline) {
           if (predicate()) return;
           await delay(50);
         }
         throw new Error("timed out waiting for " + label);
       };
       location.hash = "#/diagnostics";
       await waitFor(() => document.body.dataset.rendererLifecycle === "disposed", "renderer disposal");
       history.back();
       await waitFor(() => document.body.dataset.rendererLifecycle === "mounted", "renderer remount");
       return {
         lifecycle: document.body.dataset.rendererLifecycle,
         routeDisposal: document.body.dataset.routeDisposal ?? null,
         renderSequence: Number(document.querySelector("#renderer-telemetry")?.dataset.rendererRenderSequence ?? "0"),
       };
     })().then(done, (error) => done({ error: String(error?.stack ?? error) }));`,
  );
  if (remount?.error) throw new Error(remount.error);
  return { narrowViewport, remount, narrowScreenshot: narrowScreenshotPath };
}

async function proveSingleInstance(first) {
  const readyDirectory = join(
    xdg.XDG_CACHE_HOME,
    "dev.fuzzyslipper.rusty-engine-demo.loading-bay",
  );
  const before = readdirSync(readyDirectory).filter((name) =>
    /^host-ready-\d+\.json$/.test(name),
  );
  const activationReceiptPath = join(
    readyDirectory,
    "desktop-activation.json",
  );
  const activationReceiptBefore = existsSync(activationReceiptPath)
    ? readFileSync(activationReceiptPath, "utf8")
    : null;
  const activationSequence = await execute(
    first.sessionId,
    `return Number(document.body.dataset.desktopActivationSequence ?? "0");`,
  );
  await request(`/session/${first.sessionId}/window/minimize`, {
    method: "POST",
    body: "{}",
  });
  await waitFor(
    () => execute(first.sessionId, "return document.hasFocus() === false;"),
    "native window focus loss",
  );
  const second = spawn(application, [], {
    cwd: temporaryRoot,
    env: { ...process.env, ...xdg },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const exitCode = await Promise.race([
    new Promise((resolveExit) =>
      second.once("exit", (code) => resolveExit(code ?? 1)),
    ),
    delay(10_000).then(() => null),
  ]);
  if (exitCode === null) {
    second.kill("SIGKILL");
    throw new Error("second desktop instance did not delegate and exit");
  }
  if (exitCode !== 0) {
    throw new Error(`second desktop instance exited with code ${exitCode}`);
  }
  const after = readdirSync(readyDirectory).filter((name) =>
    /^host-ready-\d+\.json$/.test(name),
  );
  if (
    after.length !== before.length ||
    !processExists(first.ready.pid, "loading-bay-browser-host")
  ) {
    throw new Error(
      "single-instance activation created another host or disrupted the first",
    );
  }
  const receipt = await waitFor(() => {
    if (!existsSync(activationReceiptPath)) return null;
    const source = readFileSync(activationReceiptPath, "utf8");
    if (source === activationReceiptBefore) return null;
    const value = JSON.parse(source);
    return value.schemaVersion === 1 &&
      value.shellPid === first.ready.shellPid &&
      value.sequence > 0 &&
      value.windowFound === true &&
      value.showRequested === true &&
      value.unminimizeRequested === true &&
      value.nativeFocusRequested === true &&
      value.webviewFocusRequested === true
      ? value
      : null;
  }, "native single-instance activation receipt");
  const activation = await executeAsync(
    first.sessionId,
    `const done = arguments[arguments.length - 1];
     Promise.all([
       window.__TAURI_INTERNALS__.invoke("plugin:window|is_minimized", { label: "main" }),
       window.__TAURI_INTERNALS__.invoke("plugin:window|is_visible", { label: "main" }),
       window.__TAURI_INTERNALS__.invoke("plugin:window|is_focused", { label: "main" }),
     ]).then(
       ([minimized, visible, focused]) => done({
         error: null,
         activationSequence: Number(document.body.dataset.desktopActivationSequence ?? "0"),
         minimized,
         visible,
         nativeFocused: focused,
         documentFocused: document.hasFocus(),
       }),
       (error) => done({ error: String(error) }),
     );`,
  );
  if (activation?.error) throw new Error(activation.error);
  return {
    delegated: true,
    focusLostBeforeActivation: true,
    nativeActivationReceipt: receipt,
    activatedExistingWebview:
      activation.activationSequence > activationSequence,
    nativeMinimized: activation.minimized,
    nativeVisible: activation.visible,
    focusRequested: true,
    nativeFocused: activation.nativeFocused,
    documentFocused: activation.documentFocused,
    exitCode,
    readyReceiptCount: after.length,
  };
}

function executablePids(executable) {
  const expected = realpathSync(executable);
  const pids = [];
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      if (realpathSync(`/proc/${entry}/exe`) === expected) {
        pids.push(Number(entry));
      }
    } catch {
      // Processes can exit while /proc is being inspected.
    }
  }
  return pids;
}

async function proveStartupFailure(sessions) {
  const brokenRoot = join(temporaryRoot, "startup-failure-release");
  const applicationDirectory = realpathSync(dirname(application));
  const installedResourceRoot = resolve(
    applicationDirectory,
    "../lib/Loading Bay",
  );
  let brokenApplication;
  if (
    existsSync(join(installedResourceRoot, "desktop-package-manifest.json"))
  ) {
    const releaseRoot = realpathSync(resolve(applicationDirectory, "../.."));
    cpSync(releaseRoot, brokenRoot, { recursive: true });
    brokenApplication = join(brokenRoot, "usr/bin/loading-bay-desktop");
    rmSync(join(brokenRoot, "usr/bin/loading-bay-browser-host"));
  } else {
    mkdirSync(brokenRoot, { recursive: true });
    brokenApplication = join(brokenRoot, "loading-bay-desktop");
    cpSync(application, brokenApplication);
    for (const entry of ["desktop-package-manifest.json", "content", "web"]) {
      cpSync(join(applicationDirectory, entry), join(brokenRoot, entry), {
        recursive: true,
      });
    }
  }
  const readyDirectory = join(
    xdg.XDG_CACHE_HOME,
    "dev.fuzzyslipper.rusty-engine-demo.loading-bay",
  );
  const readyBefore = readdirSync(readyDirectory).filter((name) =>
    /^host-ready-\d+\.json$/.test(name),
  ).length;
  const startedAt = performance.now();
  const sessionId = await createSession(brokenApplication);
  const session = { sessionId, ready: null };
  sessions.push(session);
  const visible = await waitFor(
    () =>
      execute(
        sessionId,
        `return document.body.dataset.desktopStartupError === "true"
          ? { title: document.title, message: document.body.textContent.trim() }
          : null;`,
      ),
    "visible packaged startup error",
  );
  const shellPids = executablePids(brokenApplication);
  if (shellPids.length !== 1) {
    throw new Error(
      `startup failure created ${shellPids.length} desktop processes; expected one`,
    );
  }
  const readyAfter = readdirSync(readyDirectory).filter((name) =>
    /^host-ready-\d+\.json$/.test(name),
  ).length;
  if (readyAfter !== readyBefore) {
    throw new Error("startup failure unexpectedly published host readiness");
  }
  await deleteSession(sessionId);
  await waitFor(
    () => executablePids(brokenApplication).length === 0,
    "startup-error shell cleanup",
  );
  return {
    visible: true,
    title: visible.title,
    message: visible.message,
    boundedMilliseconds: performance.now() - startedAt,
    sidecarStarted: false,
    orphanProcesses: false,
  };
}

const sessions = [];
try {
  await waitFor(async () => {
    try {
      const response = await fetch(`http://127.0.0.1:${driverPort}/status`);
      return response.ok;
    } catch {
      return false;
    }
  }, "tauri-driver");

  const first = await runProductSession({
    startGame: true,
    measureMenuIdle: true,
  });
  sessions.push(first);
  if (
    !/^http:\/\/127\.0\.0\.1:\d+$/.test(first.result.origin) ||
    typeof first.result.menu?.hostSessionId !== "string" ||
    first.result.renderSequence <= 0 ||
    first.result.lifecycle !== "mounted" ||
    first.result.runtimeError !== "" ||
    !first.result.securityHeaders?.contentSecurityPolicy?.includes(
      "ws://127.0.0.1:*",
    ) ||
    first.result.securityHeaders.referrerPolicy !== "no-referrer" ||
    first.result.securityHeaders.contentTypeOptions !== "nosniff" ||
    first.result.stylesheetMedia !== "all"
  ) {
    throw new Error(
      `native product evidence is incomplete: ${JSON.stringify(first.result)}`,
    );
  }
  const screenshot = await request(`/session/${first.sessionId}/screenshot`);
  mkdirSync(dirname(screenshotPath), { recursive: true });
  writeFileSync(screenshotPath, Buffer.from(screenshot, "base64"));
  const installedWindow = await exerciseInstalledWindow(first);
  const singleInstance = await proveSingleInstance(first);
  const gameplayIdleMeasurement = await measureIdle(first.ready.shellPid);
  const processMeasurement = processTree(first.ready.shellPid);
  await closeProductSession(first.sessionId);
  await waitFor(
    () =>
      !processExists(first.ready.pid, "loading-bay-browser-host") &&
      !processExists(first.ready.shellPid, "loading-bay-desktop"),
    "first shell and host cleanup",
  );

  const requireContinue = process.env.TAURI_SMOKE_REQUIRE_CONTINUE === "true";
  const restarted = await runProductSession({
    startGame: false,
    continueGame: requireContinue,
  });
  if (requireContinue && restarted.result.continuedCompletedCampaign !== true) {
    throw new Error(
      "installed Continue did not restore the completed campaign",
    );
  }
  sessions.push(restarted);
  await closeProductSession(restarted.sessionId);
  await waitFor(
    () =>
      !processExists(restarted.ready.pid, "loading-bay-browser-host") &&
      !processExists(restarted.ready.shellPid, "loading-bay-desktop"),
    "restarted shell and host cleanup",
  );

  const crash = await runProductSession({ startGame: false });
  sessions.push(crash);
  process.kill(crash.ready.pid, "SIGKILL");
  const hostCrashError = await waitFor(
    () =>
      execute(
        crash.sessionId,
        `return document.body.dataset.desktopFatalError === "true"
          ? {
              title: document.title,
              message: document.body.textContent.trim(),
              diagnostic: document.querySelector("#desktop-fatal-diagnostic")?.textContent ?? null,
            }
          : null;`,
      ),
    "visible host crash error",
  );
  if (!processExists(crash.ready.shellPid, "loading-bay-desktop")) {
    throw new Error("desktop shell exited before presenting the host crash");
  }
  const hostLogPath = join(
    xdg.XDG_DATA_HOME,
    "dev.fuzzyslipper.rusty-engine-demo.loading-bay",
    "logs/browser-host.log",
  );
  const hostLog = readFileSync(hostLogPath, "utf8");
  if (!hostLog.includes("terminated") || !hostLog.includes("signal")) {
    throw new Error(`host crash was not recorded actionably: ${hostLogPath}`);
  }
  const crashLogTail = hostLog.split("\n").slice(-12).join("\n");
  await closeProductSession(crash.sessionId);
  await waitFor(
    () => !processExists(crash.ready.shellPid, "loading-bay-desktop"),
    "host-error shell cleanup",
  );

  const shellCrash = await runProductSession({ startGame: false });
  sessions.push(shellCrash);
  process.kill(shellCrash.ready.shellPid, "SIGKILL");
  await waitFor(
    () => !processExists(shellCrash.ready.pid, "loading-bay-browser-host"),
    "host cleanup after shell crash",
  );
  await deleteSession(shellCrash.sessionId);
  const startupFailure = await proveStartupFailure(sessions);

  const evidence = {
    schemaVersion: 1,
    application,
    first: first.result,
    restart: restarted.result,
    processLifecycle: {
      normalExitRemovedHost: true,
      restartRemovedHost: true,
      hostCrashPresentedVisibleError: true,
      hostCrashError,
      hostErrorCloseExitedShell: true,
      shellCrashRemovedHost: true,
      startupFailure,
      crashLogPath: hostLogPath,
      crashLogTail,
    },
    installedWindow,
    singleInstance,
    processMeasurement,
    idleMeasurement: {
      gameplay: gameplayIdleMeasurement,
      menu: first.result.menuIdleMeasurement,
    },
    screenshot: screenshotPath,
  };
  mkdirSync(dirname(evidencePath), { recursive: true });
  writeFileSync(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(evidence, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`${output.join("")}\n`);
  throw error;
} finally {
  for (const session of sessions) {
    await deleteSession(session.sessionId);
    for (const pid of [session.ready?.pid, session.ready?.shellPid]) {
      const expected =
        pid === session.ready?.pid
          ? "loading-bay-browser-host"
          : "loading-bay-desktop";
      if (Number.isInteger(pid) && processExists(pid, expected)) {
        process.kill(pid, "SIGKILL");
      }
    }
  }
  try {
    process.kill(-driver.pid, "SIGTERM");
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
  await Promise.race([
    new Promise((resolveDriver) => driver.once("exit", resolveDriver)),
    delay(2000),
  ]);
  try {
    process.kill(-driver.pid, "SIGKILL");
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
  rmSync(temporaryRoot, { recursive: true, force: true });
}
