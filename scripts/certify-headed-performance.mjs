import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

const chromium = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
const targetUrl = process.env.RUSTY_ENGINE_DEMO_URL ?? "http://127.0.0.1:8787/";
const durationMilliseconds = Number(
  process.env.RUSTY_ENGINE_DEMO_SAMPLE_MILLISECONDS ?? "20000",
);
const viewport = { height: 900, width: 1600 };

if (!existsSync(chromium)) {
  throw new Error(`Chromium is required (${chromium} not found)`);
}
if (
  !Number.isFinite(durationMilliseconds) ||
  durationMilliseconds < 5_000 ||
  durationMilliseconds > 60_000
) {
  throw new Error(
    "RUSTY_ENGINE_DEMO_SAMPLE_MILLISECONDS must be from 5000 through 60000",
  );
}
if (
  process.env.DISPLAY === undefined &&
  process.env.WAYLAND_DISPLAY === undefined
) {
  throw new Error(
    "headed performance certification requires DISPLAY or WAYLAND_DISPLAY",
  );
}

const debuggingPort = await reservePort();
const profileDirectory = mkdtempSync(
  join(tmpdir(), "rusty-engine-headed-certification-"),
);
const browser = spawn(
  chromium,
  [
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--enable-gpu",
    "--ignore-gpu-blocklist",
    "--disable-background-timer-throttling",
    "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding",
    "--autoplay-policy=no-user-gesture-required",
    "--ozone-platform=wayland",
    `--remote-debugging-port=${String(debuggingPort)}`,
    "--remote-debugging-address=127.0.0.1",
    "--remote-allow-origins=*",
    `--user-data-dir=${profileDirectory}`,
    `--window-size=${String(viewport.width)},${String(viewport.height)}`,
    "about:blank",
  ],
  { stdio: ["ignore", "pipe", "pipe"] },
);
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
  await client.send("Page.enable");
  await client.send("Runtime.enable");
  await client.send("Network.enable");
  const networkUpdates = [];
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
      // Non-session WebSocket traffic is irrelevant to this proof.
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
      if (Number.isSafeInteger(acknowledged)) {
        const settledThrough = Math.min(consumedInputSequence, acknowledged);
        for (const [sequence, sentAt] of inputCommandSentAt) {
          if (sequence <= settledThrough) {
            inputCommandRoundTrips.push((event.timestamp - sentAt) * 1_000);
            inputCommandSentAt.delete(sequence);
          }
        }
      }
      if (envelope?.update?.kind === "delta") {
        networkUpdates.push({
          bytes: Buffer.byteLength(payload),
          owners: Object.fromEntries(
            Object.entries(envelope.update.changes ?? {}).map(
              ([owner, value]) => [
                owner,
                Buffer.byteLength(JSON.stringify(value)),
              ],
            ),
          ),
        });
      }
    } catch {
      // Non-session WebSocket traffic is irrelevant to this proof.
    }
  });
  await client.send("Emulation.setDeviceMetricsOverride", {
    width: viewport.width,
    height: viewport.height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await client.send("Page.navigate", { url: targetUrl });
  await client.send("Page.bringToFront");
  await waitFor(
    "document.querySelector('red-main-menu') !== null",
    "main menu",
  );
  await evaluate(`
    [...document.querySelectorAll("button")]
      .find((button) => button.textContent?.trim() === "New game")
      ?.click()
  `);
  await waitFor(
    "document.body.dataset.rendererLifecycle === 'mounted' && document.querySelector('#viewport') !== null",
    "mounted game renderer",
    30_000,
  );

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

  const samples = [];
  let lastRendererSampleSequence = -1;
  const startedAt = Date.now();
  let mouseStep = 0;
  let nextSampleAt = startedAt;
  while (Date.now() - startedAt < durationMilliseconds) {
    mouseStep += 1;
    const x = canvas.x + ((mouseStep % 7) - 3);
    const y = canvas.y + ((mouseStep % 5) - 2);
    await client.send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x,
      y,
      button: "none",
      buttons: 0,
    });
    if (Date.now() < nextSampleAt) {
      await delay(16);
      continue;
    }
    nextSampleAt += 100;
    const sample = await evaluate(`
      (() => {
        const telemetry = document.querySelector("#renderer-telemetry");
        const body = document.body.dataset;
        const data = telemetry?.dataset ?? {};
        return {
          backendSubmissionDurationMs: Number(data.rendererBackendSubmissionMilliseconds),
          frameIntervalMs: Number(data.rendererFrameIntervalMilliseconds),
          rendererSampleSequence: Number(data.rendererSampleSequence),
          renderSequence: Number(data.rendererRenderSequence),
          timingSource: data.rendererTimingSource ?? "",
          entityCount: Number(data.rendererEntityCount),
          residentChunkCount: Number(data.rendererResidentChunkCount),
          renderDiffCount: Number(data.rendererRenderDiffCount),
          snapshotCadenceMs: Number(body.sessionSnapshotCadenceMilliseconds),
          commandRttMs: Number(body.sessionRttMilliseconds),
          steadyPayloadBytes: Number(body.sessionSteadyBytes),
          pendingInput: Number(body.sessionPendingInput),
          pendingInputMax: Number(body.sessionPendingInputMax),
          pendingEdges: Number(body.sessionPendingEdges),
          pendingEdgesMax: Number(body.sessionPendingEdgesMax),
          pendingOutboundMax: Number(body.sessionPendingOutboundMax),
          droppedFacts: Number(body.sessionDroppedFacts),
          serverTick: Number(body.sessionServerTick),
        };
      })()
    `);
    if (
      sample.rendererSampleSequence > lastRendererSampleSequence &&
      finiteSample(sample)
    ) {
      samples.push(sample);
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

  const minimumSampleCount = Math.max(
    20,
    Math.floor(durationMilliseconds / 200),
  );
  if (samples.length < minimumSampleCount) {
    throw new Error(
      `headed certification collected only ${String(samples.length)} unique renderer samples (minimum ${String(minimumSampleCount)})`,
    );
  }
  const minimumCommandSampleCount = Math.max(
    50,
    Math.floor(durationMilliseconds / 100),
  );
  if (inputCommandRoundTrips.length < minimumCommandSampleCount) {
    throw new Error(
      `headed certification collected only ${String(inputCommandRoundTrips.length)} acknowledged input commands (minimum ${String(minimumCommandSampleCount)})`,
    );
  }
  const report = buildReport(
    samples,
    environment,
    networkUpdates,
    inputCommandRoundTrips,
  );
  console.log(JSON.stringify(report, null, 2));
  const failures = [
    report.rendererCadenceMs.p95 > 20
      ? `renderer p95 ${String(report.rendererCadenceMs.p95)} ms > 20 ms`
      : null,
    report.rendererCadenceMs.p99 > 33.5
      ? `renderer p99 ${String(report.rendererCadenceMs.p99)} ms > 33.5 ms`
      : null,
    report.commandRttMs.p95 > 50
      ? `command RTT p95 ${String(report.commandRttMs.p95)} ms > 50 ms`
      : null,
    report.commandRttMs.max > 100
      ? `command RTT max ${String(report.commandRttMs.max)} ms > 100 ms`
      : null,
    report.steadyPayloadBytes.p95 > 4_096
      ? `payload p95 ${String(report.steadyPayloadBytes.p95)} bytes > 4096 bytes`
      : null,
    report.bounds.pendingInputMax > 2
      ? `pending input ${String(report.bounds.pendingInputMax)} > 2`
      : null,
    report.bounds.pendingEdgesMax > 32
      ? `pending edges ${String(report.bounds.pendingEdgesMax)} > 32`
      : null,
    report.bounds.pendingOutboundMax > 1
      ? `pending outbound ${String(report.bounds.pendingOutboundMax)} > 1`
      : null,
    report.bounds.droppedFacts !== 0
      ? `dropped facts ${String(report.bounds.droppedFacts)} != 0`
      : null,
    report.timingSources.some((source) => source !== "animationFrame")
      ? `unexpected renderer timing source ${report.timingSources.join(", ")}`
      : null,
    environment.runtimeError.length > 0
      ? `runtime error: ${environment.runtimeError}`
      : null,
  ].filter((failure) => failure !== null);
  if (failures.length > 0) {
    throw new Error(`headed certification failed: ${failures.join("; ")}`);
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

function finiteSample(sample) {
  return [
    sample.backendSubmissionDurationMs,
    sample.commandRttMs,
    sample.frameIntervalMs,
    sample.rendererSampleSequence,
    sample.snapshotCadenceMs,
    sample.steadyPayloadBytes,
  ].every(Number.isFinite);
}

function buildReport(
  samples,
  environment,
  networkUpdates,
  inputCommandRoundTrips,
) {
  const metric = (name) => distribution(samples.map((sample) => sample[name]));
  const largestNetworkUpdates = networkUpdates
    .toSorted((left, right) => right.bytes - left.bytes)
    .slice(0, 5);
  return {
    schemaVersion: 1,
    targetUrl,
    durationMilliseconds,
    sampleCount: samples.length,
    firstServerTick: samples[0].serverTick,
    lastServerTick: samples.at(-1).serverTick,
    environment,
    timingSources: [...new Set(samples.map((sample) => sample.timingSource))],
    rendererCadenceMs: metric("frameIntervalMs"),
    backendSubmissionDurationMs: metric("backendSubmissionDurationMs"),
    snapshotCadenceMs: metric("snapshotCadenceMs"),
    commandRttMs: distribution(inputCommandRoundTrips),
    commandRttSampleCount: inputCommandRoundTrips.length,
    sampledLastCommandRttMs: metric("commandRttMs"),
    steadyPayloadBytes: metric("steadyPayloadBytes"),
    counters: {
      entityCount: range(samples.map((sample) => sample.entityCount)),
      residentChunkCount: range(
        samples.map((sample) => sample.residentChunkCount),
      ),
      renderDiffCount: range(samples.map((sample) => sample.renderDiffCount)),
      renderSequence: range(samples.map((sample) => sample.renderSequence)),
    },
    bounds: {
      pendingInputMax: Math.max(
        ...samples.map((sample) => sample.pendingInputMax),
      ),
      pendingEdgesMax: Math.max(
        ...samples.map((sample) => sample.pendingEdgesMax),
      ),
      pendingOutboundMax: Math.max(
        ...samples.map((sample) => sample.pendingOutboundMax),
      ),
      droppedFacts: Math.max(...samples.map((sample) => sample.droppedFacts)),
    },
    largestNetworkUpdates,
  };
}

function distribution(values) {
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

function range(values) {
  return { min: Math.min(...values), max: Math.max(...values) };
}

function round(value) {
  return Number(value.toFixed(3));
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
    } catch {
      // Route navigation replaces the execution context.
    }
    await delay(50);
  }
  throw new Error(`timed out waiting for ${label}\n${browserOutput}`);
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
