import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const chromium = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
const targetUrl =
  process.env.RUSTY_ENGINE_DEMO_URL ??
  "http://127.0.0.1:8787/?smoke=1&camera-boundary-evidence=1#/game";
const milestones = ["wall", "corner", "doorway"];
const viewport = { width: 1600, height: 900 };

if (!existsSync(chromium)) {
  throw new Error(`Chromium is required (${chromium} not found)`);
}
const health = new URL("/health", targetUrl);
const healthResponse = await fetch(health);
const healthBody = await healthResponse.json();
if (
  !healthResponse.ok ||
  healthBody?.project !== "rusty-engine-demo" ||
  healthBody?.status !== "ok"
) {
  throw new Error(`${health.href} is not a healthy Rusty Engine Demo host`);
}

const debuggingPort = await reservePort();
const profileDirectory = mkdtempSync(
  join(tmpdir(), "rusty-engine-camera-boundary-capture-"),
);
const browser = spawn(
  chromium,
  [
    "--headless=new",
    "--no-sandbox",
    "--disable-dev-shm-usage",
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
  { cwd: root, stdio: ["ignore", "pipe", "pipe"] },
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
  await Promise.all([
    client.send("Page.enable"),
    client.send("Runtime.enable"),
  ]);
  await client.send("Emulation.setDeviceMetricsOverride", {
    ...viewport,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await client.send("Page.navigate", { url: targetUrl });

  const captures = [];
  for (const milestone of milestones) {
    await waitFor(
      client,
      `document.body?.dataset.cameraBoundaryMilestone === ${JSON.stringify(milestone)}`,
      `${milestone} boundary milestone`,
      180_000,
    );
    const evidence = await evaluateValue(
      client,
      `(() => {
        const camera = (document.body.dataset.gameplayCameraPosition ?? "")
          .split(",").map(Number);
        const player = (document.body.dataset.gameplayCameraPlayerPosition ?? "")
          .split(",").map(Number);
        const horizontalOffset = Number(
          document.body.dataset.gameplayCameraHorizontalOffset,
        );
        const eyeHeight = Number(document.body.dataset.gameplayCameraEyeHeight);
        if (
          camera.length !== 3 || player.length !== 3 ||
          camera.some((value) => !Number.isFinite(value)) ||
          player.some((value) => !Number.isFinite(value)) ||
          !Number.isFinite(horizontalOffset) || !Number.isFinite(eyeHeight)
        ) {
          throw new Error("camera boundary evidence is incomplete");
        }
        let overlay = document.querySelector("#camera-boundary-evidence");
        if (!(overlay instanceof HTMLElement)) {
          overlay = document.createElement("aside");
          overlay.id = "camera-boundary-evidence";
          overlay.style.cssText = [
            "position:fixed", "left:24px", "bottom:24px", "z-index:2147483647",
            "padding:14px 18px", "background:rgba(3,8,14,.92)",
            "border:1px solid #76d6ff", "border-radius:6px", "color:#e9f8ff",
            "font:600 16px/1.45 ui-monospace,monospace", "white-space:pre-wrap",
            "box-shadow:0 8px 30px rgba(0,0,0,.55)",
          ].join(";");
          document.body.append(overlay);
        }
        overlay.textContent = [
          ${JSON.stringify(milestone.toUpperCase())} + " BOUNDARY APPROACH",
          "Rust player: " + player.join(", "),
          "Gameplay camera: " + camera.join(", "),
          "Horizontal offset: " + horizontalOffset.toFixed(6),
          "Eye height: " + eyeHeight.toFixed(6),
          "X/Z camera pivot equals collision-authoritative player pivot",
        ].join("\\n");
        return {
          milestone: ${JSON.stringify(milestone)},
          player,
          camera,
          horizontalOffset,
          eyeHeight,
          campaignEvidence:
            document.body.dataset[
              ${JSON.stringify(
                milestone === "wall"
                  ? "campaignArrivalEvidence"
                  : milestone === "corner"
                    ? "campaignStorageEvidence"
                    : "campaignLockedDoorEvidence",
              )}
            ] ?? null,
        };
      })()`,
    );
    if (evidence.horizontalOffset !== 0 || evidence.eyeHeight !== 1.2) {
      throw new Error(
        `${milestone} camera pivot diverged from player: ${JSON.stringify(evidence)}`,
      );
    }
    const relativePath = `docs/evidence/gameplay-camera-${milestone}.png`;
    const screenshot = await client.send("Page.captureScreenshot", {
      captureBeyondViewport: false,
      format: "png",
      fromSurface: true,
    });
    if (typeof screenshot?.data !== "string") {
      throw new Error("Chromium did not return PNG screenshot data");
    }
    writeFileSync(
      resolve(root, relativePath),
      Buffer.from(screenshot.data, "base64"),
    );
    captures.push({ ...evidence, screenshot: relativePath });
    await client.send("Runtime.evaluate", {
      expression: `document.body.dataset.cameraBoundaryCaptured = ${JSON.stringify(milestone)}`,
    });
  }

  await waitFor(
    client,
    `document.body?.dataset.smokeStatus === "pass"`,
    "complete normal-control campaign",
    300_000,
  );
  const product = await evaluateValue(
    client,
    `({
      smokeStatus: document.body.dataset.smokeStatus,
      sessionTransport: document.body.dataset.sessionTransport,
      roundTripMaxMilliseconds:
        Number(document.body.dataset.sessionRoundTripMaxMilliseconds),
      pendingOutboundMax: Number(document.body.dataset.sessionPendingOutboundMax),
      droppedFacts: Number(document.body.dataset.sessionDroppedFacts),
      pendingInput: Number(document.body.dataset.sessionPendingInput),
      rendererLifecycle: document.body.dataset.rendererLifecycle,
      revision: document.querySelector("#revision")?.textContent?.trim() ?? null,
    })`,
  );
  const report = {
    schemaVersion: 1,
    targetUrl,
    viewport: [viewport.width, viewport.height],
    project: healthBody.project,
    captures,
    product,
  };
  writeFileSync(
    resolve(root, "docs/evidence/gameplay-camera-boundaries.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  console.log(JSON.stringify(report));
} finally {
  client?.close();
  browser.kill("SIGTERM");
  await onceExit(browser);
  rmSync(profileDirectory, { force: true, recursive: true });
}

async function evaluateValue(client, expression) {
  const result = await client.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result?.exceptionDetails !== undefined) {
    throw new Error(
      `Chromium evaluation failed: ${JSON.stringify(result.exceptionDetails)}`,
    );
  }
  return result?.result?.value;
}

async function waitFor(client, expression, label, timeout) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if ((await evaluateValue(client, expression)) === true) {
      return;
    }
    if (browser.exitCode !== null) {
      throw new Error(
        `Chromium exited before ${label}\n${browserOutput.slice(-4_000)}`,
      );
    }
    await delay(50);
  }
  throw new Error(`timed out waiting for ${label}`);
}

async function reservePort() {
  const server = createServer();
  await new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (address === null || typeof address === "string") {
    server.close();
    throw new Error("could not reserve a Chromium debugging port");
  }
  await new Promise((resolveClose, rejectClose) =>
    server.close((error) =>
      error === undefined ? resolveClose() : rejectClose(error),
    ),
  );
  return address.port;
}

async function waitForChromiumTarget(port) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    if (browser.exitCode !== null) {
      throw new Error(
        `Chromium exited before debugging was ready\n${browserOutput.slice(-4_000)}`,
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
    `Chromium debugging target did not become ready\n${browserOutput.slice(-4_000)}`,
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
  });
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

function onceExit(process) {
  if (process.exitCode !== null || process.signalCode !== null) {
    return Promise.resolve();
  }
  return new Promise((resolveExit) => process.once("exit", resolveExit));
}
