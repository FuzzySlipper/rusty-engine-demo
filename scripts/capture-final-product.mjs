import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const chromium = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
const targetUrl =
  process.env.RUSTY_ENGINE_DEMO_URL ?? "http://127.0.0.1:8787/#/game?mode=new";
const captures = [
  {
    path: resolve(root, "docs/evidence/final-game-desktop.png"),
    viewport: { width: 1600, height: 900 },
  },
  {
    path: resolve(root, "docs/evidence/final-game-narrow.png"),
    viewport: { width: 390, height: 844 },
  },
];

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
  join(tmpdir(), "rusty-engine-final-capture-"),
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
  await setViewport(client, captures[0].viewport);
  await client.send("Page.navigate", { url: targetUrl });
  await waitFor(
    client,
    `document.querySelector("#revision")?.textContent?.trim() !== "REV —" &&
      document.querySelector("#encounter-state")?.textContent?.trim() !== "LOADING" &&
      document.querySelector("#viewport") instanceof HTMLCanvasElement &&
      document.querySelector("[data-active-modal]") === null`,
    "accepted game projection",
    45_000,
  );
  await delay(1_000);

  for (const capture of captures) {
    await setViewport(client, capture.viewport);
    await delay(500);
    const result = await client.send("Page.captureScreenshot", {
      captureBeyondViewport: false,
      format: "png",
      fromSurface: true,
    });
    if (typeof result?.data !== "string") {
      throw new Error("Chromium did not return PNG screenshot data");
    }
    writeFileSync(capture.path, Buffer.from(result.data, "base64"));
  }

  const readout = await client.send("Runtime.evaluate", {
    expression: `({
      encounter: document.querySelector("#encounter-state")?.textContent?.trim(),
      revision: document.querySelector("#revision")?.textContent?.trim(),
      renderer: document.querySelector("#renderer-status")?.textContent?.trim(),
      runtimeError: document.body.dataset.runtimeError ?? ""
    })`,
    returnByValue: true,
  });
  console.log(
    JSON.stringify({
      captures: captures.map((capture) => ({
        path: capture.path.slice(root.length + 1),
        viewport: [capture.viewport.width, capture.viewport.height],
      })),
      page: readout?.result?.value,
      targetUrl,
    }),
  );
} finally {
  client?.close();
  browser.kill("SIGTERM");
  await onceExit(browser);
  rmSync(profileDirectory, { force: true, recursive: true });
}

async function setViewport(client, viewport) {
  await client.send("Emulation.setDeviceMetricsOverride", {
    width: viewport.width,
    height: viewport.height,
    deviceScaleFactor: 1,
    mobile: viewport.width < 600,
  });
}

async function waitFor(client, expression, label, timeout) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const result = await client.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
    });
    if (result?.result?.value === true) {
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

function onceExit(process) {
  if (process.exitCode !== null || process.signalCode !== null) {
    return Promise.resolve();
  }
  return new Promise((resolveExit) => process.once("exit", resolveExit));
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}
