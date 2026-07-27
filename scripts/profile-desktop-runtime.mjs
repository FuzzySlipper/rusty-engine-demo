import { spawn, execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { analyzeBuildStats } from "./profile-browser-build.mjs";

const chromium = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
const targetUrl = process.env.RUSTY_ENGINE_DEMO_URL ?? "http://127.0.0.1:8787/";
const idleMilliseconds = boundedDuration(
  "RUSTY_ENGINE_DEMO_IDLE_MILLISECONDS",
  2_000,
  1_000,
  10_000,
);
const inputMilliseconds = boundedDuration(
  "RUSTY_ENGINE_DEMO_INPUT_MILLISECONDS",
  5_000,
  2_000,
  30_000,
);
const viewport = { height: 900, width: 1600 };

if (!existsSync(chromium)) {
  throw new Error(`Chromium is required (${chromium} not found)`);
}
if (
  process.env.DISPLAY === undefined &&
  process.env.WAYLAND_DISPLAY === undefined
) {
  throw new Error(
    "desktop-representative profiling requires DISPLAY or WAYLAND_DISPLAY",
  );
}

const debuggingPort = await reservePort();
const profileDirectory = mkdtempSync(
  join(tmpdir(), "rusty-engine-desktop-profile-"),
);
const browserArguments = [
  "--no-sandbox",
  "--disable-dev-shm-usage",
  "--enable-gpu",
  "--ignore-gpu-blocklist",
  "--disable-background-timer-throttling",
  "--disable-backgrounding-occluded-windows",
  "--disable-renderer-backgrounding",
  "--autoplay-policy=no-user-gesture-required",
  ...(process.env.WAYLAND_DISPLAY === undefined
    ? []
    : ["--ozone-platform=wayland"]),
  `--remote-debugging-port=${String(debuggingPort)}`,
  "--remote-debugging-address=127.0.0.1",
  "--remote-allow-origins=*",
  `--user-data-dir=${profileDirectory}`,
  `--window-size=${String(viewport.width)},${String(viewport.height)}`,
  "about:blank",
];
const browser = spawn(chromium, browserArguments, {
  stdio: ["ignore", "pipe", "pipe"],
});
let browserOutput = "";
browser.stdout.on("data", (chunk) => {
  browserOutput += String(chunk);
});
browser.stderr.on("data", (chunk) => {
  browserOutput += String(chunk);
});

let client;
try {
  const target = await waitForChromiumTarget(debuggingPort);
  client = await connectDevTools(target.webSocketDebuggerUrl);
  await Promise.all([
    client.send("Page.enable"),
    client.send("Runtime.enable"),
    client.send("Network.enable"),
    client.send("Performance.enable"),
  ]);
  await client.send("Page.addScriptToEvaluateOnNewDocument", {
    source: pageInstrumentationSource(),
  });
  await client.send("Emulation.setDeviceMetricsOverride", {
    width: viewport.width,
    height: viewport.height,
    deviceScaleFactor: 1,
    mobile: false,
  });

  const inputCommandSentAt = new Map();
  const inputCommandRoundTrips = [];
  let consumedInputSequence = 0;
  client.on("Network.webSocketFrameSent", (event) => {
    const payload = event?.response?.payloadData;
    if (typeof payload !== "string") return;
    try {
      const envelope = JSON.parse(payload);
      if (
        envelope?.command?.kind === "setInputIntent" &&
        Number.isSafeInteger(envelope.sequence)
      ) {
        inputCommandSentAt.set(envelope.sequence, event.timestamp);
      }
    } catch {
      // Other WebSocket traffic is outside this profile.
    }
  });
  client.on("Network.webSocketFrameReceived", (event) => {
    const payload = event?.response?.payloadData;
    if (typeof payload !== "string") return;
    try {
      const envelope = JSON.parse(payload);
      const input =
        envelope?.update?.kind === "full"
          ? envelope.update.state?.input
          : envelope?.update?.kind === "delta"
            ? envelope.update.changes?.input
            : undefined;
      if (Number.isSafeInteger(input?.consumedSequence)) {
        consumedInputSequence = input.consumedSequence;
      }
      const acknowledged = Number(envelope?.acknowledgedCommandSequence);
      if (!Number.isSafeInteger(acknowledged)) return;
      const settledThrough = Math.min(consumedInputSequence, acknowledged);
      for (const [sequence, sentAt] of inputCommandSentAt) {
        if (sequence <= settledThrough) {
          inputCommandRoundTrips.push((event.timestamp - sentAt) * 1_000);
          inputCommandSentAt.delete(sequence);
        }
      }
    } catch {
      // Other WebSocket traffic is outside this profile.
    }
  });

  const coldStartedAt = performance.now();
  await client.send("Page.navigate", { url: targetUrl });
  await client.send("Page.bringToFront");
  await waitForUsableMenu();
  const coldMenuWallMs = performance.now() - coldStartedAt;
  const coldMenu = await captureState("cold-menu", coldMenuWallMs);
  const coldMenuIdle = await measureIdleWindow();

  const warmStartedAt = performance.now();
  await client.send("Page.reload", { ignoreCache: false });
  await waitForUsableMenu();
  const warmMenuWallMs = performance.now() - warmStartedAt;
  const warmMenu = await captureState("warm-menu", warmMenuWallMs);
  const warmMenuIdle = await measureIdleWindow();

  const gameStartedAt = await evaluate(`
    (() => {
      const startedAt = performance.now();
      [...document.querySelectorAll("button")]
        .find((button) => button.textContent?.trim() === "New game")
        ?.click();
      return startedAt;
    })()
  `);
  await waitFor(
    "document.body.dataset.rendererLifecycle === 'mounted' && Number(document.body.dataset.sessionSnapshotSequence ?? '0') > 0 && Number(document.querySelector('#renderer-telemetry')?.dataset.rendererSampleSequence ?? '0') > 0",
    "first authoritative projection and rendered frame",
    30_000,
  );
  const gameReadyMs = await evaluate(
    `performance.now() - ${JSON.stringify(gameStartedAt)}`,
  );
  const gameplay = await captureState("gameplay", gameReadyMs);
  const gameplayIdle = await measureIdleWindow();

  const canvas = await evaluate(`
    (() => {
      const rectangle = document.querySelector("#viewport").getBoundingClientRect();
      return {
        x: rectangle.left + rectangle.width / 2,
        y: rectangle.top + rectangle.height / 2,
      };
    })()
  `);
  await client.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: canvas.x,
    y: canvas.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await client.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: canvas.x,
    y: canvas.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  await waitFor(
    "document.pointerLockElement?.id === 'viewport'",
    "pointer lock",
  );
  await client.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    code: "KeyW",
    key: "w",
    windowsVirtualKeyCode: 87,
  });

  const rendererSamples = [];
  let lastRendererSampleSequence = -1;
  let mouseStep = 0;
  const inputStartedAt = Date.now();
  while (Date.now() - inputStartedAt < inputMilliseconds) {
    mouseStep += 1;
    await client.send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: canvas.x + ((mouseStep % 7) - 3),
      y: canvas.y + ((mouseStep % 5) - 2),
      button: "none",
      buttons: 0,
    });
    const sample = await evaluate(`
      (() => {
        const data = document.querySelector("#renderer-telemetry")?.dataset ?? {};
        return {
          backendSubmissionDurationMs: Number(data.rendererBackendSubmissionMilliseconds),
          frameIntervalMs: Number(data.rendererFrameIntervalMilliseconds),
          rendererSampleSequence: Number(data.rendererSampleSequence),
          timingSource: data.rendererTimingSource ?? "",
        };
      })()
    `);
    if (
      sample.rendererSampleSequence > lastRendererSampleSequence &&
      [
        sample.backendSubmissionDurationMs,
        sample.frameIntervalMs,
        sample.rendererSampleSequence,
      ].every(Number.isFinite)
    ) {
      rendererSamples.push(sample);
      lastRendererSampleSequence = sample.rendererSampleSequence;
    }
    await delay(16);
  }
  await client.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    code: "KeyW",
    key: "w",
    windowsVirtualKeyCode: 87,
  });
  await delay(250);

  const inputInstrumentation = await evaluate(
    "globalThis.__rustyDesktopProfile",
  );
  const environment = await evaluate(`
    (() => {
      const canvas = document.createElement("canvas");
      const gl = canvas.getContext("webgl2") ?? canvas.getContext("webgl");
      const extension = gl?.getExtension("WEBGL_debug_renderer_info");
      return {
        userAgent: navigator.userAgent,
        viewport: [window.innerWidth, window.innerHeight],
        webglVendor: extension
          ? gl.getParameter(extension.UNMASKED_VENDOR_WEBGL)
          : gl?.getParameter(gl.VENDOR) ?? "unavailable",
        webglRenderer: extension
          ? gl.getParameter(extension.UNMASKED_RENDERER_WEBGL)
          : gl?.getParameter(gl.RENDERER) ?? "unavailable",
        pointerLockActive: document.pointerLockElement?.id === "viewport",
        runtimeError:
          document.querySelector("#runtime-error")?.textContent?.trim() ?? "",
      };
    })()
  `);
  const buildStatsPath = "dist/apps/loading-bay/stats.json";
  const report = {
    schemaVersion: 1,
    revision: currentRevision(),
    targetUrl,
    desktopPackaging: {
      available: existsSync("src-tauri") || existsSync("apps/desktop"),
      measuredSurface:
        "visible Chromium/Wayland against the installed production host",
    },
    environment,
    durations: {
      idleMilliseconds,
      inputMilliseconds,
    },
    startup: {
      coldMenu,
      warmMenu,
      firstAuthoritativeProjectionAndFrameMs: round(gameReadyMs),
      gameplay,
    },
    idle: {
      coldMenu: coldMenuIdle,
      warmMenu: warmMenuIdle,
      gameplay: gameplayIdle,
    },
    input: {
      eventToNextFrameMs: distribution(inputInstrumentation.inputToNextFrameMs),
      eventToNextFrameSampleCount:
        inputInstrumentation.inputToNextFrameMs.length,
      authoritativeCommandRttMs: distribution(inputCommandRoundTrips),
      authoritativeCommandRttSampleCount: inputCommandRoundTrips.length,
      rendererCadenceMs: distribution(
        rendererSamples.map((sample) => sample.frameIntervalMs),
      ),
      backendSubmissionDurationMs: distribution(
        rendererSamples.map((sample) => sample.backendSubmissionDurationMs),
      ),
      rendererSampleCount: rendererSamples.length,
      timingSources: [
        ...new Set(rendererSamples.map((sample) => sample.timingSource)),
      ],
    },
    build: existsSync(buildStatsPath)
      ? analyzeBuildStats(JSON.parse(readFileSync(buildStatsPath, "utf8")))
      : null,
  };

  const failures = [
    report.desktopPackaging.available
      ? null
      : "native desktop packaging is not present (recorded dependency, not a profiling failure)",
    environment.runtimeError.length > 0
      ? `runtime error: ${environment.runtimeError}`
      : null,
    rendererSamples.length < Math.max(20, inputMilliseconds / 200)
      ? `only ${String(rendererSamples.length)} renderer samples`
      : null,
    inputCommandRoundTrips.length < Math.max(20, inputMilliseconds / 200)
      ? `only ${String(inputCommandRoundTrips.length)} authoritative input samples`
      : null,
    inputInstrumentation.inputToNextFrameMs.length <
    Math.max(20, inputMilliseconds / 200)
      ? `only ${String(inputInstrumentation.inputToNextFrameMs.length)} input-to-frame samples`
      : null,
  ].filter(
    (failure) =>
      failure !== null &&
      !failure.startsWith("native desktop packaging is not present"),
  );
  console.log(JSON.stringify(report, null, 2));
  if (failures.length > 0) {
    throw new Error(
      `desktop-representative profile failed: ${failures.join("; ")}`,
    );
  }
} finally {
  try {
    await client?.send("Input.dispatchKeyEvent", {
      type: "keyUp",
      code: "KeyW",
      key: "w",
      windowsVirtualKeyCode: 87,
    });
  } catch {
    // The page may already be gone.
  }
  client?.close();
  await terminateProcess(browser);
  rmSync(profileDirectory, { recursive: true, force: true });
}

async function waitForUsableMenu() {
  await waitFor(
    `(() => {
      const menu = document.querySelector("red-main-menu");
      const availability = document.querySelector(".availability")?.textContent?.trim() ?? "";
      return menu !== null && availability.length > 0 && !availability.startsWith("Checking");
    })()`,
    "usable main menu and Continue state",
  );
}

async function captureState(label, wallMilliseconds) {
  const page = await evaluate(`
    (() => {
      const navigation = performance.getEntriesByType("navigation")[0];
      const resources = performance.getEntriesByType("resource");
      const profile = globalThis.__rustyDesktopProfile;
      return {
        label: ${JSON.stringify(label)},
        wallMilliseconds: ${JSON.stringify(round(wallMilliseconds))},
        navigation: navigation
          ? {
              domContentLoadedMs: navigation.domContentLoadedEventEnd,
              loadMs: navigation.loadEventEnd,
              transferBytes: navigation.transferSize,
              encodedBodyBytes: navigation.encodedBodySize,
            }
          : null,
        firstPaintMs:
          performance.getEntriesByName("first-paint")[0]?.startTime ?? null,
        firstContentfulPaintMs:
          performance.getEntriesByName("first-contentful-paint")[0]?.startTime ?? null,
        resources: resources.map((resource) => ({
          name: new URL(resource.name).pathname,
          initiatorType: resource.initiatorType,
          transferBytes: resource.transferSize,
          encodedBodyBytes: resource.encodedBodySize,
          durationMs: resource.duration,
        })),
        longTasks: [...profile.longTasks],
      };
    })()
  `);
  const metrics = await performanceMetrics();
  return {
    ...page,
    resources: summarizeResources(page.resources),
    longTasks: summarizeLongTasks(page.longTasks),
    memory: {
      rendererJavaScriptHeapBytes: metrics.JSHeapUsedSize,
      rendererJavaScriptHeapTotalBytes: metrics.JSHeapTotalSize,
      chromiumProcessTreeRssBytes: processTreeResidentBytes(browser.pid),
      nodes: metrics.Nodes,
      eventListeners: metrics.JSEventListeners,
    },
  };
}

async function measureIdleWindow() {
  const before = await performanceMetrics();
  await delay(idleMilliseconds);
  const after = await performanceMetrics();
  return {
    taskDurationMs: round((after.TaskDuration - before.TaskDuration) * 1_000),
    scriptDurationMs: round(
      (after.ScriptDuration - before.ScriptDuration) * 1_000,
    ),
    layoutCount: after.LayoutCount - before.LayoutCount,
    styleRecalculationCount: after.RecalcStyleCount - before.RecalcStyleCount,
    layoutDurationMs: round(
      (after.LayoutDuration - before.LayoutDuration) * 1_000,
    ),
    styleRecalculationDurationMs: round(
      (after.RecalcStyleDuration - before.RecalcStyleDuration) * 1_000,
    ),
    heapGrowthBytes: after.JSHeapUsedSize - before.JSHeapUsedSize,
    nodeDelta: after.Nodes - before.Nodes,
    eventListenerDelta: after.JSEventListeners - before.JSEventListeners,
  };
}

async function performanceMetrics() {
  const result = await client.send("Performance.getMetrics");
  return Object.fromEntries(
    result.metrics.map(({ name, value }) => [name, value]),
  );
}

function summarizeResources(resources) {
  const scripts = resources.filter(
    ({ initiatorType, name }) =>
      initiatorType === "script" || name.endsWith(".js"),
  );
  return {
    count: resources.length,
    transferBytes: sum(resources.map(({ transferBytes }) => transferBytes)),
    encodedBodyBytes: sum(
      resources.map(({ encodedBodyBytes }) => encodedBodyBytes),
    ),
    scriptCount: scripts.length,
    scriptTransferBytes: sum(scripts.map(({ transferBytes }) => transferBytes)),
    scriptEncodedBodyBytes: sum(
      scripts.map(({ encodedBodyBytes }) => encodedBodyBytes),
    ),
    scripts: scripts
      .map(({ name, transferBytes, encodedBodyBytes, durationMs }) => ({
        name,
        transferBytes,
        encodedBodyBytes,
        durationMs: round(durationMs),
      }))
      .sort((left, right) => right.encodedBodyBytes - left.encodedBodyBytes),
    largestResources: resources
      .map(
        ({
          name,
          initiatorType,
          transferBytes,
          encodedBodyBytes,
          durationMs,
        }) => ({
          name,
          initiatorType,
          transferBytes,
          encodedBodyBytes,
          durationMs: round(durationMs),
        }),
      )
      .sort((left, right) => right.encodedBodyBytes - left.encodedBodyBytes)
      .slice(0, 12),
  };
}

function summarizeLongTasks(longTasks) {
  return {
    count: longTasks.length,
    totalDurationMs: round(sum(longTasks.map(({ duration }) => duration))),
    maxDurationMs: round(
      Math.max(0, ...longTasks.map(({ duration }) => duration)),
    ),
  };
}

function pageInstrumentationSource() {
  return `
    (() => {
      const profile = {
        inputToNextFrameMs: [],
        longTasks: [],
      };
      Object.defineProperty(globalThis, "__rustyDesktopProfile", {
        configurable: false,
        value: profile,
        writable: false,
      });
      try {
        new PerformanceObserver((list) => {
          for (const entry of list.getEntries()) {
            profile.longTasks.push({
              duration: entry.duration,
              startTime: entry.startTime,
            });
          }
        }).observe({ type: "longtask", buffered: true });
      } catch {
        // Long-task entries are optional browser instrumentation.
      }
      addEventListener(
        "mousemove",
        () => {
          const inputAt = performance.now();
          requestAnimationFrame(() => {
            profile.inputToNextFrameMs.push(performance.now() - inputAt);
          });
        },
        { capture: true, passive: true },
      );
    })();
  `;
}

function processTreeResidentBytes(rootPid) {
  if (process.platform !== "linux") {
    return null;
  }
  const visited = new Set();
  const pending = [rootPid];
  let residentKilobytes = 0;
  while (pending.length > 0) {
    const pid = pending.pop();
    if (!Number.isSafeInteger(pid) || visited.has(pid)) continue;
    visited.add(pid);
    try {
      const status = readFileSync(`/proc/${String(pid)}/status`, "utf8");
      const match = /^VmRSS:\s+(\d+)\s+kB$/m.exec(status);
      if (match !== null) {
        residentKilobytes += Number(match[1]);
      }
      const children = readFileSync(
        `/proc/${String(pid)}/task/${String(pid)}/children`,
        "utf8",
      )
        .trim()
        .split(/\s+/)
        .filter((value) => value.length > 0)
        .map(Number);
      pending.push(...children);
    } catch {
      // A short-lived process may exit between enumeration and reading.
    }
  }
  return residentKilobytes * 1_024;
}

function distribution(values) {
  if (values.length === 0) {
    return null;
  }
  const sorted = values.toSorted((left, right) => left - right);
  return {
    p50: percentile(sorted, 0.5),
    p95: percentile(sorted, 0.95),
    p99: percentile(sorted, 0.99),
    max: round(sorted.at(-1)),
  };
}

function percentile(sorted, quantile) {
  const index = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil(sorted.length * quantile) - 1),
  );
  return round(sorted[index]);
}

function currentRevision() {
  return execFileSync("git", ["rev-parse", "HEAD"], {
    encoding: "utf8",
  }).trim();
}

function boundedDuration(name, fallback, minimum, maximum) {
  const value = Number(process.env[name] ?? String(fallback));
  if (!Number.isFinite(value) || value < minimum || value > maximum) {
    throw new Error(
      `${name} must be from ${String(minimum)} through ${String(maximum)}`,
    );
  }
  return value;
}

function sum(values) {
  return values.reduce((total, value) => total + value, 0);
}

function round(value) {
  return value === null ? null : Number(value.toFixed(3));
}

async function evaluate(expression) {
  const result = await client.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails !== undefined) {
    throw new Error(
      `Chromium evaluation failed: ${result.exceptionDetails.text}`,
    );
  }
  return result.result.value;
}

async function waitFor(expression, label, timeout = 15_000) {
  const deadline = Date.now() + timeout;
  let lastError = "";
  while (Date.now() < deadline) {
    if (browser.exitCode !== null) {
      throw new Error(
        `Chromium exited while waiting for ${label}\n${browserOutput}`,
      );
    }
    try {
      if ((await evaluate(expression)) === true) {
        return;
      }
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await delay(50);
  }
  let pageState = "";
  try {
    pageState = JSON.stringify(
      await evaluate(`({
        href: location.href,
        rendererLifecycle: document.body.dataset.rendererLifecycle ?? "",
        runtimeError:
          document.querySelector("#runtime-error")?.textContent?.trim() ?? "",
        sessionSnapshotSequence:
          document.body.dataset.sessionSnapshotSequence ?? "",
        rendererSampleSequence:
          document.querySelector("#renderer-telemetry")?.dataset.rendererSampleSequence ?? "",
        text: document.body.textContent?.trim().slice(0, 500) ?? "",
      })`),
    );
  } catch {
    // The debugging connection may be gone.
  }
  throw new Error(
    `timed out waiting for ${label}${lastError.length > 0 ? ` (${lastError})` : ""}${pageState.length > 0 ? `\npage state: ${pageState}` : ""}\n${browserOutput}`,
  );
}

async function waitForChromiumTarget(port) {
  const deadline = Date.now() + 45_000;
  while (Date.now() < deadline) {
    if (browser.exitCode !== null) {
      throw new Error(
        `Chromium exited before debugging was ready\n${browserOutput}`,
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
      // Chromium takes a moment to publish the target.
    }
    await delay(50);
  }
  throw new Error(
    `Chromium debugging target did not become ready\n${browserOutput}`,
  );
}

function connectDevTools(url) {
  return new Promise((resolveConnect, rejectConnect) => {
    const socket = new WebSocket(url);
    const pending = new Map();
    const handlers = new Map();
    let nextId = 0;
    let connected = false;
    socket.addEventListener("open", () => {
      connected = true;
      resolveConnect({
        send(method, params = {}) {
          if (socket.readyState !== WebSocket.OPEN) {
            return Promise.reject(
              new Error(
                `Chromium debugging connection is not open (${method})`,
              ),
            );
          }
          nextId += 1;
          const id = nextId;
          return new Promise((resolveCommand, rejectCommand) => {
            pending.set(id, { resolveCommand, rejectCommand });
            try {
              socket.send(JSON.stringify({ id, method, params }));
            } catch (error) {
              pending.delete(id);
              rejectCommand(error);
            }
          });
        },
        close() {
          socket.close();
        },
        on(method, handler) {
          const listeners = handlers.get(method) ?? new Set();
          listeners.add(handler);
          handlers.set(method, listeners);
          return () => listeners.delete(handler);
        },
      });
    });
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data));
      if (typeof message.id !== "number") {
        if (typeof message.method === "string") {
          for (const handler of handlers.get(message.method) ?? []) {
            handler(message.params);
          }
        }
        return;
      }
      const command = pending.get(message.id);
      if (command === undefined) return;
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
    socket.addEventListener("error", (error) => {
      if (!connected) rejectConnect(error);
    });
    socket.addEventListener("close", () => {
      const error = new Error("Chromium debugging connection closed");
      if (!connected) rejectConnect(error);
      for (const command of pending.values()) {
        command.rejectCommand(error);
      }
      pending.clear();
    });
  });
}

function reservePort() {
  return new Promise((resolvePort, rejectPort) => {
    const server = createServer();
    server.once("error", rejectPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (typeof address !== "object" || address === null) {
        server.close();
        rejectPort(new Error("could not reserve Chromium debugging port"));
        return;
      }
      server.close(() => resolvePort(address.port));
    });
  });
}

function terminateProcess(process) {
  return new Promise((resolveTermination) => {
    if (process.exitCode !== null) {
      resolveTermination();
      return;
    }
    process.once("exit", () => resolveTermination());
    process.kill("SIGTERM");
    setTimeout(() => {
      if (process.exitCode === null) {
        process.kill("SIGKILL");
      }
    }, 2_000).unref();
  });
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => {
    setTimeout(resolveDelay, milliseconds);
  });
}
