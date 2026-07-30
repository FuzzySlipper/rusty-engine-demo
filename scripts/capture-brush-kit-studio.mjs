import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const OUTPUT = resolve(ROOT, "docs/evidence");
const HOST = process.env.RUSTY_STUDIO_BRUSH_HOST ?? "http://127.0.0.1:4396";
const PROJECT_HASH = JSON.parse(
  await readFile(
    resolve(ROOT, "docs/evidence/voxel-brush-kit-authoring.json"),
    "utf8",
  ),
).finalHash;
const PORT = 9436;
const profile = await mkdtemp(resolve(tmpdir(), "rusty-brush-studio-"));
const browser = spawn(
  process.env.CHROMIUM_BIN ?? "chromium",
  [
    "--headless=new",
    "--no-sandbox",
    "--disable-dev-shm-usage",
    "--enable-unsafe-swiftshader",
    "--use-gl=angle",
    "--use-angle=swiftshader",
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${profile}`,
    "--window-size=1600,900",
    "about:blank",
  ],
  { stdio: ["ignore", "ignore", "pipe"] },
);

let socket;
let nextId = 1;
const pending = new Map();

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

async function target() {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const targets = await (
        await fetch(`http://127.0.0.1:${PORT}/json/list`)
      ).json();
      const page = targets.find((candidate) => candidate.type === "page");
      if (page !== undefined) return page;
    } catch {
      // Chromium has not exposed CDP yet.
    }
    await delay(100);
  }
  throw new Error("Chromium did not expose a page target");
}

function command(method, params = {}) {
  const id = nextId;
  nextId += 1;
  return new Promise((resolveCommand, rejectCommand) => {
    pending.set(id, { resolve: resolveCommand, reject: rejectCommand });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await command("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails !== undefined) {
    throw new Error(JSON.stringify(result.exceptionDetails));
  }
  return result.result.value;
}

async function waitFor(expression, description) {
  for (let attempt = 0; attempt < 240; attempt += 1) {
    if (await evaluate(expression)) return;
    await delay(125);
  }
  throw new Error(`timed out waiting for ${description}`);
}

async function focus(label) {
  await evaluate(`(() => {
    const input = document.querySelector('input[aria-label="Filter hierarchy"]');
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value'
    ).set;
    setter.call(input, ${JSON.stringify(label)});
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  })()`);
  await waitFor(
    `Array.from(document.querySelectorAll('[role="treeitem"]')).some(
      (node) => node.textContent.includes(${JSON.stringify(label)})
    )`,
    `${label} hierarchy row`,
  );
  await evaluate(`(() => {
    const row = Array.from(document.querySelectorAll('[role="treeitem"]')).find(
      (node) => node.textContent.includes(${JSON.stringify(label)})
    );
    row.click();
    row.click();
    row.dispatchEvent(new MouseEvent('dblclick', {
      bubbles: true,
      cancelable: true,
      view: window
    }));
    return row.getAttribute('data-entity-id');
  })()`);
  await waitFor(
    `document.querySelector('[data-selected-entity]')?.getAttribute('data-selected-entity') !== null`,
    `${label} selection`,
  );
  await delay(1500);
}

async function screenshot(name) {
  const captured = await command("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(resolve(OUTPUT, name), Buffer.from(captured.data, "base64"));
}

try {
  const page = await target();
  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolveOpen, rejectOpen) => {
    socket.addEventListener("open", resolveOpen, { once: true });
    socket.addEventListener("error", rejectOpen, { once: true });
  });
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id === undefined) return;
    const awaiting = pending.get(message.id);
    if (awaiting === undefined) return;
    pending.delete(message.id);
    if (message.error === undefined) awaiting.resolve(message.result);
    else awaiting.reject(new Error(JSON.stringify(message.error)));
  });
  await command("Page.enable");
  await command("Runtime.enable");
  await command("Performance.enable");
  await command("Emulation.setDeviceMetricsOverride", {
    width: 1600,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await command("Page.navigate", {
    url:
      `${HOST}/?root=%2Fhome%2Fdev%2Frusty-engine-demo` +
      "&project=content%2Fprojects%2Floading-bay.project.json",
  });
  await waitFor(
    `document.querySelector('[data-project-hash="${PROJECT_HASH}"]') !== null`,
    "canonical brush project",
  );
  await mkdir(OUTPUT, { recursive: true });
  await screenshot("voxel-brush-kit-studio-overview.png");
  await focus("brush-proof-wall-north-east");
  await screenshot("voxel-brush-kit-dense-wall.png");
  await focus("brush-proof-wall-north-west");
  await screenshot("voxel-brush-kit-conservative-wall.png");
  const proof = await evaluate(`({
    projectHash: document.querySelector('[data-project-hash]')?.getAttribute('data-project-hash'),
    selectedEntity: document.querySelector('rusty-studio-viewport')?.getAttribute('data-selected-entity'),
    canvasCount: document.querySelectorAll('canvas').length,
    viewport: (() => {
      const viewport = document.querySelector('rusty-studio-viewport');
      return {
        status: viewport?.getAttribute('data-renderer-status'),
        retainedOps: Number(viewport?.getAttribute('data-retained-ops')),
        definitions: Number(viewport?.getAttribute('data-voxel-object-definitions')),
        instances: Number(viewport?.getAttribute('data-voxel-object-instances')),
        placementGhosts: Number(viewport?.getAttribute('data-voxel-object-placement-ghosts')),
        selectedRenderHandle: viewport?.getAttribute('data-selected-render-handle'),
        authoredFrameHash: viewport?.getAttribute('data-authored-frame-hash'),
        rendererError: viewport?.getAttribute('data-renderer-error')
      };
    })(),
    viewportText: document.querySelector('[data-visual-id="studio-viewport-column"]')?.innerText
  })`);
  const performanceMetrics = await command("Performance.getMetrics");
  proof.performance = Object.fromEntries(
    performanceMetrics.metrics
      .filter(({ name }) =>
        [
          "Documents",
          "Frames",
          "JSEventListeners",
          "Nodes",
          "LayoutCount",
          "RecalcStyleCount",
          "ScriptDuration",
          "TaskDuration",
          "JSHeapUsedSize",
          "JSHeapTotalSize",
        ].includes(name),
      )
      .map(({ name, value }) => [name, value]),
  );
  proof.capture = {
    viewport: [1600, 900],
    host: HOST,
    screenshots: [
      "docs/evidence/voxel-brush-kit-studio-overview.png",
      "docs/evidence/voxel-brush-kit-dense-wall.png",
      "docs/evidence/voxel-brush-kit-conservative-wall.png",
    ],
    rendererSubmission:
      "pending Rusty Engine #6406; no private WebGL or Three instrumentation used",
  };
  await writeFile(
    resolve(OUTPUT, "voxel-brush-kit-studio-browser.json"),
    `${JSON.stringify(proof, null, 2)}\n`,
  );
  process.stdout.write(`${JSON.stringify(proof, null, 2)}\n`);
} finally {
  socket?.close();
  browser.kill("SIGTERM");
  await new Promise((resolveExit) => browser.once("exit", resolveExit));
  await rm(profile, { recursive: true });
}
