import { spawn } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readlinkSync,
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

const output = [];
const driverCommand = process.env.DISPLAY
  ? driverBinary
  : (process.env.XVFB_RUN ?? "xvfb-run");
const driverArguments = [
  ...(process.env.DISPLAY ? [] : ["-a", driverBinary]),
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

async function createSession() {
  const value = await request("/session", {
    method: "POST",
    body: JSON.stringify({
      capabilities: {
        alwaysMatch: {
          "tauri:options": {
            application,
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

async function runProductSession({ startGame }) {
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
       };
     })().then(done, (error) => done({ error: String(error?.stack ?? error) }));`,
  );
  if (menu?.error) {
    await deleteSession(sessionId);
    throw new Error(menu.error);
  }
  let frame = {
    renderSequence: null,
    rendererStatus: null,
    lifecycle: null,
    runtimeError: "",
  };
  if (startGame) {
    const clicked = await execute(
      sessionId,
      `const button = [...document.querySelectorAll("button")].find(
         (element) => element.textContent?.trim() === "New game",
       );
       if (!(button instanceof HTMLButtonElement) || button.disabled) return false;
       button.click();
       return true;`,
    );
    if (!clicked) {
      throw new Error("New game action is unavailable");
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
          !state.overlay
          ? state
          : null;
      },
      "authoritative rendered game frame",
      240_000,
    );
  }
  const ready = await waitFor(() => {
    try {
      return latestReady();
    } catch {
      return null;
    }
  }, "host readiness");
  return {
    sessionId,
    result: {
      ...menu,
      ...frame,
    },
    ready,
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

  const first = await runProductSession({ startGame: true });
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
  await closeProductSession(first.sessionId);
  await waitFor(
    () =>
      !processExists(first.ready.pid, "loading-bay-browser-host") &&
      !processExists(first.ready.shellPid, "loading-bay-desktop"),
    "first shell and host cleanup",
  );

  const restarted = await runProductSession({ startGame: false });
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
  await waitFor(
    () => !processExists(crash.ready.shellPid, "loading-bay-desktop"),
    "shell exit after host crash",
  );
  await deleteSession(crash.sessionId);

  const shellCrash = await runProductSession({ startGame: false });
  sessions.push(shellCrash);
  process.kill(shellCrash.ready.shellPid, "SIGKILL");
  await waitFor(
    () => !processExists(shellCrash.ready.pid, "loading-bay-browser-host"),
    "host cleanup after shell crash",
  );
  await deleteSession(shellCrash.sessionId);

  const evidence = {
    schemaVersion: 1,
    application,
    first: first.result,
    restart: restarted.result,
    processLifecycle: {
      normalExitRemovedHost: true,
      restartRemovedHost: true,
      hostCrashExitedShell: true,
      shellCrashRemovedHost: true,
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
