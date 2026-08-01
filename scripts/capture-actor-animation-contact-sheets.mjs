import { spawn, spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUTPUT = resolve(ROOT, "docs/evidence/animated-mesh-contact-sheets");
const CHROMIUM = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
const ENGINE_REVISION = JSON.parse(
  readFileSync(resolve(ROOT, "engine-source.json"), "utf8"),
).commit;
const TIMES = [0, 0.25, 0.5, 0.75, 1];
const CLIPS = ["idle", "run", "jump", "attack", "hit", "death"];
const ACTORS = [
  {
    asset: "mesh-animation/bay-rusher",
    camera: { position: [7.5, 2.2, 15], pitchDegrees: 0, yawDegrees: 0 },
  },
  {
    asset: "mesh-animation/arc-warden",
    camera: { position: [20.5, 2.8, 24.5], pitchDegrees: 0, yawDegrees: 0 },
  },
];

const proofRoot = mkdtempSync(join(tmpdir(), "loading-bay-animation-capture-"));
mkdirSync(dirname(OUTPUT), { recursive: true });
const stagedOutput = mkdtempSync(
  resolve(dirname(OUTPUT), ".animated-mesh-contact-sheets-stage-"),
);
let host;
let browser;
try {
  const project = resolve(proofRoot, "loading-bay.project.json");
  const committedProject = spawnSync(
    "git",
    ["show", "HEAD:content/projects/loading-bay.project.json"],
    { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
  if (committedProject.status !== 0) {
    throw new Error(`could not read committed project: ${committedProject.stderr}`);
  }
  writeFileSync(project, committedProject.stdout);
  cpSync(
    resolve(ROOT, "content/assets/actor-kit"),
    resolve(proofRoot, "content/assets/actor-kit"),
    { recursive: true },
  );

  const address = `127.0.0.1:${String(await reservePort())}`;
  host = spawn(
    "cargo",
    [
      "run", "-q", "-p", "loading-bay-game", "--bin", "browser-host", "--",
      "--addr", address,
      "--project", project,
      "--save-root", resolve(proofRoot, "save-slots"),
    ],
    { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] },
  );
  const hostOutput = captureOutput(host);
  await waitForHealth(`http://${address}/health`, host, hostOutput);

  const debuggingPort = await reservePort();
  const browserProfile = resolve(proofRoot, "chromium");
  browser = spawn(
    CHROMIUM,
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
      `--remote-debugging-port=${String(debuggingPort)}`,
      "--remote-debugging-address=127.0.0.1",
      "--remote-allow-origins=*",
      `--user-data-dir=${browserProfile}`,
      "about:blank",
    ],
    { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] },
  );
  const browserOutput = captureOutput(browser);
  const target = await waitForChromiumTarget(debuggingPort, browser, browserOutput);
  const client = await connectDevTools(target.webSocketDebuggerUrl);
  try {
    await client.send("Emulation.setDeviceMetricsOverride", {
      width: 640,
      height: 640,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await client.send("Page.enable");
    await client.send("Runtime.enable");
    await client.send("Page.navigate", {
      url: `http://${address}/#/game?mode=new&visualQa=animation`,
    });
    await waitForExpression(
      client,
      "typeof window.__loadingBayAnimationCapture === 'function'",
      "Loading Bay animation capture hook",
      45_000,
    );

    const summary = [];
    for (const actor of ACTORS) {
      for (const clip of CLIPS) {
        const result = await evaluateValue(
          client,
          `window.__loadingBayAnimationCapture(${JSON.stringify({
            asset: actor.asset,
            camera: actor.camera,
            clip,
            normalizedTimes: TIMES,
            overlaysIncluded: true,
            providerRevision: ENGINE_REVISION,
          })})`,
        );
        const prefix = `${actor.asset.split("/").at(-1)}-${clip}`;
        for (const image of result.images) {
          writeDataUrl(resolve(stagedOutput, image.fileName), image.pngDataUrl);
        }
        writeDataUrl(
          resolve(stagedOutput, `${prefix}-contact-sheet.png`),
          result.contactSheetPngDataUrl,
        );
        writeFileSync(
          resolve(stagedOutput, `${prefix}.json`),
          result.manifestJson,
        );
        summary.push({
          asset: actor.asset,
          clip,
          contactSheet: `${prefix}-contact-sheet.png`,
          manifest: `${prefix}.json`,
          diagnostics: result.manifest.samples.flatMap((sample) => sample.diagnostics),
          sampledWorldBounds: result.manifest.samples.map((sample) => sample.sampledWorldBounds),
        });
      }
    }

    await client.send("Emulation.setDeviceMetricsOverride", {
      width: 900,
      height: 600,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await client.send("Page.navigate", { url: `http://${address}/#/` });
    await waitForExpression(client, "document.querySelector('red-main-menu') !== null", "disposed game route");
    await client.send("Page.navigate", {
      url: `http://${address}/#/game?mode=new&visualQa=animation`,
    });
    await waitForExpression(
      client,
      "typeof window.__loadingBayAnimationCapture === 'function'",
      "remounted animation capture hook",
      45_000,
    );
    const remounted = await evaluateValue(
      client,
      `window.__loadingBayAnimationCapture(${JSON.stringify({
        asset: ACTORS[0].asset,
        camera: ACTORS[0].camera,
        clip: "idle",
        normalizedTimes: [0.5],
        overlaysIncluded: true,
        providerRevision: ENGINE_REVISION,
      })})`,
    );
    writeFileSync(
      resolve(stagedOutput, "certification.json"),
      `${JSON.stringify({
        schemaVersion: 1,
        engineRevision: ENGINE_REVISION,
        viewport: [640, 640],
        resizedViewport: [900, 600],
        normalizedTimes: TIMES,
        actors: ACTORS.map((actor) => actor.asset),
        clips: CLIPS,
        captures: summary,
        remount: {
          asset: remounted.manifest.asset,
          clip: remounted.manifest.clip,
          normalizedTime: remounted.manifest.samples[0].normalizedTime,
          sampledWorldBounds: remounted.manifest.samples[0].sampledWorldBounds,
        },
      }, null, 2)}\n`,
    );
  } finally {
    client.close();
  }

  const backup = `${OUTPUT}.backup`;
  rmSync(backup, { recursive: true, force: true });
  if (existsSync(OUTPUT)) renameSync(OUTPUT, backup);
  try {
    renameSync(stagedOutput, OUTPUT);
    rmSync(backup, { recursive: true, force: true });
  } catch (error) {
    if (existsSync(backup)) renameSync(backup, OUTPUT);
    throw error;
  }
  console.log(`captured ${String(ACTORS.length * CLIPS.length)} deterministic animated-mesh contact sheets`);
} finally {
  if (browser !== undefined) await terminate(browser);
  if (host !== undefined) await terminate(host);
  rmSync(proofRoot, { recursive: true, force: true });
  rmSync(stagedOutput, { recursive: true, force: true });
}

function writeDataUrl(path, dataUrl) {
  const match = /^data:image\/png;base64,(.+)$/u.exec(dataUrl);
  if (match?.[1] === undefined) throw new Error(`capture did not return a PNG data URL for ${path}`);
  writeFileSync(path, Buffer.from(match[1], "base64"));
}

function captureOutput(child) {
  let output = "";
  child.stdout.on("data", (chunk) => { output += String(chunk); });
  child.stderr.on("data", (chunk) => { output += String(chunk); });
  return () => output;
}

async function reservePort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("could not reserve a port");
  await new Promise((resolveClose) => server.close(resolveClose));
  return address.port;
}

async function waitForHealth(url, child, output) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`browser host exited early\n${output()}`);
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {}
    await delay(100);
  }
  throw new Error(`browser host did not become healthy\n${output()}`);
}

async function waitForChromiumTarget(port, child, output) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Chromium exited early\n${output()}`);
    try {
      const targets = await (await fetch(`http://127.0.0.1:${String(port)}/json/list`)).json();
      const target = targets.find((candidate) => candidate.type === "page");
      if (target !== undefined) return target;
    } catch {}
    await delay(100);
  }
  throw new Error(`Chromium did not expose a page target\n${output()}`);
}

async function connectDevTools(url) {
  const socket = new WebSocket(url);
  await new Promise((resolveOpen, reject) => {
    socket.addEventListener("open", resolveOpen, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let sequence = 0;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(String(event.data));
    if (message.id === undefined) return;
    const request = pending.get(message.id);
    if (request === undefined) return;
    pending.delete(message.id);
    if (message.error !== undefined) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  });
  return {
    send(method, params = {}) {
      sequence += 1;
      const id = sequence;
      return new Promise((resolveResult, reject) => {
        pending.set(id, { resolve: resolveResult, reject });
        socket.send(JSON.stringify({ id, method, params }));
      });
    },
    close() { socket.close(); },
  };
}

async function evaluateValue(client, expression) {
  const result = await client.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails !== undefined) {
    throw new Error(result.exceptionDetails.exception?.description ?? "browser evaluation failed");
  }
  return result.result.value;
}

async function waitForExpression(client, expression, label, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await evaluateValue(client, `Boolean(${expression})`)) return;
    await delay(100);
  }
  throw new Error(`timed out waiting for ${label}`);
}

async function terminate(child) {
  if (child.exitCode !== null) return;
  child.kill("SIGTERM");
  await Promise.race([
    new Promise((resolveExit) => child.once("exit", resolveExit)),
    delay(2_000),
  ]);
  if (child.exitCode === null) child.kill("SIGKILL");
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}
