import { spawn } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

const repoRoot = resolve(dirname(new URL(import.meta.url).pathname), "..");
const application = resolve(
  repoRoot,
  process.env.TAURI_APPLICATION ?? "target/release/loading-bay-desktop",
);
const driverBinary = process.env.TAURI_DRIVER_BIN ?? "/home/agent/.cargo/bin/tauri-driver";
const nativeDriver = process.env.TAURI_NATIVE_DRIVER ?? "/usr/bin/WebKitWebDriver";
const driverPort = Number(process.env.TAURI_DRIVER_PORT ?? 4454);
const nativePort = Number(process.env.TAURI_NATIVE_DRIVER_PORT ?? 4455);
const evidencePath = resolve(
  repoRoot,
  process.env.TAURI_HEADED_EVIDENCE ?? "target/tauri-headed-smoke-evidence.json",
);
const screenshotPath = resolve(
  repoRoot,
  process.env.TAURI_HEADED_SCREENSHOT ?? "target/tauri-headed-smoke.png",
);

if (!existsSync(application)) {
  throw new Error(
    `headed Tauri proof requires ${application}; run pnpm run build:tauri:binary first`,
  );
}
if (!existsSync(driverBinary) || !existsSync(nativeDriver)) {
  throw new Error("headed Tauri proof requires tauri-driver and WebKitWebDriver");
}

const temporaryRoot = mkdtempSync(join(tmpdir(), "loading-bay-tauri-headed-"));
const xdg = {
  XDG_DATA_HOME: join(temporaryRoot, "data"),
  XDG_CACHE_HOME: join(temporaryRoot, "cache"),
  XDG_CONFIG_HOME: join(temporaryRoot, "config"),
  XDG_STATE_HOME: join(temporaryRoot, "state"),
};
for (const path of Object.values(xdg)) mkdirSync(path, { recursive: true });

const driverOutput = [];
let runEvidence = null;
const driver = spawn(
  process.env.XVFB_RUN ?? "xvfb-run",
  [
    "-a",
    driverBinary,
    "--port",
    String(driverPort),
    "--native-port",
    String(nativePort),
    "--native-driver",
    nativeDriver,
  ],
  {
    cwd: temporaryRoot,
    detached: true,
    env: { ...process.env, ...xdg },
    stdio: ["ignore", "pipe", "pipe"],
  },
);
driver.stdout.on("data", (chunk) => driverOutput.push(`stdout ${String(chunk)}`));
driver.stderr.on("data", (chunk) => driverOutput.push(`stderr ${String(chunk)}`));

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

async function waitFor(predicate, label, timeoutMilliseconds = 45_000) {
  const deadline = Date.now() + timeoutMilliseconds;
  let last = null;
  while (Date.now() < deadline) {
    try {
      const value = await predicate();
      if (value) return value;
      last = value;
    } catch (error) {
      last = error instanceof Error ? error.message : String(error);
    }
    await delay(200);
  }
  throw new Error(`timed out waiting for ${label}: ${JSON.stringify(last)}`);
}

async function request(path, options = {}) {
  const response = await fetch(`http://127.0.0.1:${driverPort}${path}`, {
    ...options,
    headers: { "content-type": "application/json", ...(options.headers ?? {}) },
  });
  const text = await response.text();
  const body = text === "" ? null : JSON.parse(text);
  if (!response.ok || body?.value?.error) {
    throw new Error(`WebDriver ${options.method ?? "GET"} ${path}: ${text}`);
  }
  return body?.value;
}

async function createSession() {
  const value = await request("/session", {
    method: "POST",
    body: JSON.stringify({
      capabilities: { alwaysMatch: { "tauri:options": { application } } },
    }),
  });
  const sessionId = value?.sessionId;
  if (typeof sessionId !== "string" || sessionId === "") {
    throw new Error(`WebDriver returned no session id: ${JSON.stringify(value)}`);
  }
  await request(`/session/${sessionId}/timeouts`, {
    method: "POST",
    body: JSON.stringify({ script: 60_000, pageLoad: 60_000 }),
  });
  return sessionId;
}

async function execute(sessionId, script) {
  const serialized = await request(`/session/${sessionId}/execute/sync`, {
    method: "POST",
    body: JSON.stringify({
      script: `return JSON.stringify((() => { ${script} })());`,
      args: [],
    }),
  });
  return JSON.parse(serialized);
}

async function deleteSession(sessionId) {
  await request(`/session/${sessionId}`, { method: "DELETE" });
}

function productPids() {
  const expected = realpathSync(application);
  return readdirSync("/proc")
    .filter((entry) => /^\d+$/u.test(entry))
    .flatMap((entry) => {
      try {
        return realpathSync(`/proc/${entry}/exe`) === expected ? [Number(entry)] : [];
      } catch {
        return [];
      }
    });
}

async function main() {
  await waitFor(() => request("/status"), "tauri-driver readiness");
  const sessionId = await createSession();
  const evidence = { schemaVersion: 1, application, sessionId, startedAt: new Date().toISOString() };
  runEvidence = evidence;
  try {
    try {
      evidence.menu = await waitFor(async () => {
        const menu = await execute(
          sessionId,
          `return {
            title: document.title,
            menu: document.querySelector("red-main-menu") !== null,
            newGame: [...document.querySelectorAll("button")].some((button) => button.textContent?.trim() === "New game"),
            lifecycle: document.body.dataset.rendererLifecycle ?? null,
          };`,
        );
        return menu.menu && menu.newGame ? menu : null;
      }, "visible Loading Bay main menu", 15_000);
    } catch (error) {
      evidence.startupDiagnostic = await execute(
        sessionId,
        `return {
          title: document.title,
          lifecycle: document.body.dataset.rendererLifecycle ?? null,
          runtimeError: document.body.dataset.runtimeError ?? null,
          startupError: document.body.dataset.desktopStartupError ?? null,
          bodyText: document.body.innerText.slice(0, 4000),
        };`,
      );
      throw error;
    }
    evidence.productPidsBeforeClose = productPids();

    const newGame = await request(`/session/${sessionId}/element`, {
      method: "POST",
      body: JSON.stringify({ using: "css selector", value: "button.primary" }),
    });
    const elementId = newGame?.["element-6066-11e4-a52e-4f735466cecf"];
    if (typeof elementId !== "string") throw new Error("WebDriver did not locate New game");
    await request(`/session/${sessionId}/element/${elementId}/click`, { method: "POST", body: "{}" });

    try {
      evidence.frame = await waitFor(async () => {
        const frame = await execute(
          sessionId,
          `return {
            lifecycle: document.body.dataset.rendererLifecycle ?? null,
            routeFrame: document.body.dataset.rendererRouteFrame ?? null,
            content: document.body.dataset.rendererContent ?? null,
            frameOps: Number(document.body.dataset.rendererFrameOps ?? "0"),
            resourceCount: Number(document.body.dataset.rendererResourceCount ?? "0"),
            textureCount: Number(document.body.dataset.rendererTextureCount ?? "0"),
            connected: document.querySelector("#feedback-session-status")?.dataset.state ?? null,
            overlay: document.querySelector(".game-state-overlay")?.textContent?.trim() ?? null,
            canvasCount: document.querySelectorAll("canvas[data-rusty-application-renderer='engine-owned']").length,
            runtimeError: document.body.dataset.runtimeError ?? "",
            camera: document.body.dataset.rendererRouteCamera ?? null,
          };`,
        );
        return frame.lifecycle === "mounted" && frame.routeFrame === "rust-authoritative" &&
          frame.content === "complete" && frame.frameOps > 0 && frame.resourceCount > 0 &&
          frame.textureCount > 0 && frame.connected === "connected" && frame.overlay === null && frame.canvasCount === 1 &&
          frame.runtimeError === "" ? frame : null;
      }, "mounted Engine-owned E1M1 frame and typed desktop session", 120_000);
    } catch (error) {
      evidence.frameDiagnostic = await execute(
        sessionId,
        `return {
          lifecycle: document.body.dataset.rendererLifecycle ?? null,
          routeFrame: document.body.dataset.rendererRouteFrame ?? null,
          content: document.body.dataset.rendererContent ?? null,
          frameOps: Number(document.body.dataset.rendererFrameOps ?? "0"),
          resourceCount: Number(document.body.dataset.rendererResourceCount ?? "0"),
          textureCount: Number(document.body.dataset.rendererTextureCount ?? "0"),
          connected: document.querySelector("#feedback-session-status")?.dataset.state ?? null,
          canvasCount: document.querySelectorAll("canvas[data-rusty-application-renderer='engine-owned']").length,
          runtimeError: document.body.dataset.runtimeError ?? "",
          startupError: document.body.dataset.desktopStartupError ?? "",
          connectionText: document.querySelector("#feedback-session-status")?.textContent ?? null,
          bodyText: document.body.innerText.slice(0, 4000),
        };`,
      );
      throw error;
    }

    // Capture only after the player-visible session overlay is gone, not only
    // after retained renderer diagnostics have been written.
    await waitFor(async () => {
      const overlay = await execute(sessionId, "return document.querySelector('.game-state-overlay') === null;");
      return overlay ? true : null;
    }, "connected overlay retirement", 15_000);
    const screenshot = await request(`/session/${sessionId}/screenshot`);
    mkdirSync(dirname(screenshotPath), { recursive: true });
    writeFileSync(screenshotPath, Buffer.from(screenshot, "base64"));
    evidence.screenshot = screenshotPath;
    evidence.typedSessionReadout = await waitFor(async () => {
      const readout = await execute(
        sessionId,
        `return {
          connected: document.querySelector("#feedback-session-status")?.dataset.state ?? null,
          lifecycle: document.body.dataset.rendererLifecycle ?? null,
          camera: document.body.dataset.rendererRouteCamera ?? null,
          connectionGeneration: document.querySelector("#feedback-session-status")?.getAttribute("data-connection-generation") ?? null,
        };`,
      );
      if (readout.connected !== "connected" || readout.lifecycle !== "mounted" ||
        readout.camera === null || readout.connectionGeneration === null) {
        return null;
      }
      return {
        ...readout,
        cameraChangedSinceFrame: readout.camera !== evidence.frame.camera,
      };
    }, "active typed desktop-session DOM readout");
  } finally {
    await deleteSession(sessionId);
  }
  evidence.shutdownClean = await waitFor(() => productPids().length === 0, "clean desktop shutdown", 15_000);
  evidence.completedAt = new Date().toISOString();
  mkdirSync(dirname(evidencePath), { recursive: true });
  writeFileSync(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
  process.stdout.write(`headed Tauri proof passed: ${evidencePath}\n`);
}

try {
  await main();
} catch (error) {
  const message = error instanceof Error ? error.stack ?? error.message : String(error);
  mkdirSync(dirname(evidencePath), { recursive: true });
  writeFileSync(
    evidencePath,
    `${JSON.stringify({
      schemaVersion: 1,
      status: "failed",
      application,
      ...runEvidence,
      error: message,
      driverOutput,
    }, null, 2)}\n`,
  );
  throw new Error(`${message}\ntauri-driver output:\n${driverOutput.join("")}`);
} finally {
  try {
    process.kill(-driver.pid, "SIGTERM");
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
  await delay(500);
  try {
    process.kill(-driver.pid, "SIGKILL");
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
  rmSync(temporaryRoot, { recursive: true, force: true });
}
