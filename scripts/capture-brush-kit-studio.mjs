import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const OUTPUT = resolve(ROOT, "docs/evidence");
const HOST = process.env.RUSTY_STUDIO_BRUSH_HOST ?? "http://127.0.0.1:4396";
const EVIDENCE_STEM =
  process.env.RUSTY_STUDIO_EVIDENCE_STEM ?? "voxel-brush-kit";
const AUTHORING_EVIDENCE =
  process.env.RUSTY_STUDIO_AUTHORING_EVIDENCE ??
  "docs/evidence/voxel-brush-kit-authoring.json";
const PRIMARY_FOCUS =
  process.env.RUSTY_STUDIO_PRIMARY_FOCUS ?? "brush-proof-wall-north-east";
const SECONDARY_FOCUS =
  process.env.RUSTY_STUDIO_SECONDARY_FOCUS ?? "brush-proof-wall-north-west";
const OVERVIEW_SCREENSHOT =
  process.env.RUSTY_STUDIO_OVERVIEW_SCREENSHOT ??
  "voxel-brush-kit-studio-overview.png";
const PRIMARY_SCREENSHOT =
  process.env.RUSTY_STUDIO_PRIMARY_SCREENSHOT ??
  "voxel-brush-kit-dense-wall.png";
const SECONDARY_SCREENSHOT =
  process.env.RUSTY_STUDIO_SECONDARY_SCREENSHOT ??
  "voxel-brush-kit-conservative-wall.png";
const PRIMARY_VIEWPORT = viewportFromEnvironment(
  "RUSTY_STUDIO_PRIMARY_VIEWPORT",
  [1600, 900],
);
const SECONDARY_VIEWPORT = viewportFromEnvironment(
  "RUSTY_STUDIO_SECONDARY_VIEWPORT",
  [1600, 900],
);
const ALIGNMENT_EVIDENCE = process.env.RUSTY_STUDIO_ALIGNMENT_EVIDENCE;
const WAIT_ATTEMPTS = Number(process.env.RUSTY_STUDIO_WAIT_ATTEMPTS ?? "480");
const authoringEvidence = JSON.parse(
  await readFile(resolve(ROOT, AUTHORING_EVIDENCE), "utf8"),
);
const PROJECT_HASH =
  authoringEvidence.finalHash ??
  authoringEvidence.projectHashAfter ??
  authoringEvidence.project?.finalHash ??
  authoringEvidence.project?.hash;
if (typeof PROJECT_HASH !== "string") {
  throw new Error("authoring evidence has no final project hash");
}
const ENGINE_REVISION = (JSON.parse(
  await readFile(resolve(ROOT, "package.json"), "utf8"),
).dependencies["@rusty-engine/renderer-host"].match(/#([0-9a-f]{40})&/u) ??
  [])[1];
if (ENGINE_REVISION === undefined) {
  throw new Error(
    "renderer-host dependency is not pinned to an exact revision",
  );
}
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

function viewportFromEnvironment(name, fallback) {
  const raw = process.env[name];
  if (raw === undefined) return fallback;
  const values = raw.split("x").map(Number);
  if (
    values.length !== 2 ||
    values.some((value) => !Number.isInteger(value) || value <= 0)
  ) {
    throw new Error(`${name} must use WIDTHxHEIGHT`);
  }
  return values;
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
  for (let attempt = 0; attempt < WAIT_ATTEMPTS; attempt += 1) {
    if (await evaluate(expression)) return;
    await delay(125);
  }
  const diagnostic = await evaluate(`({
    title: document.title,
    projectHash: document.querySelector('[data-project-hash]')
      ?.getAttribute('data-project-hash'),
    text: document.body?.innerText?.slice(0, 2000),
    rendererStatus: document.querySelector('rusty-studio-viewport')
      ?.getAttribute('data-renderer-status'),
    rendererError: document.querySelector('rusty-studio-viewport')
      ?.getAttribute('data-renderer-error'),
    frameSubmissionEvidence: document.querySelector('loading-bay-studio-root')
      ?.getAttribute('data-frame-submission-evidence')
  })`);
  throw new Error(
    `timed out waiting for ${description}: ${JSON.stringify(diagnostic)}`,
  );
}

async function frameEvidence() {
  return evaluate(`(() => {
    const value = document.querySelector('loading-bay-studio-root')
      ?.getAttribute('data-frame-submission-evidence');
    return value === null || value === undefined ? null : JSON.parse(value);
  })()`);
}

async function waitForSubmission(minimumCount, updateKind, description) {
  await waitFor(
    `(() => {
      const value = document.querySelector('loading-bay-studio-root')
        ?.getAttribute('data-frame-submission-evidence');
      if (value === null || value === undefined) return false;
      const evidence = JSON.parse(value);
      return evidence.count >= ${minimumCount} &&
        evidence.latest?.updateKind === ${JSON.stringify(updateKind)};
    })()`,
    description,
  );
  return frameEvidence();
}

async function clickButton(selector, label) {
  const clicked = await evaluate(`(() => {
    const button = Array.from(document.querySelectorAll(${JSON.stringify(selector)}))
      .find((candidate) => candidate.textContent.trim() === ${JSON.stringify(label)});
    if (button === undefined) return false;
    button.click();
    return true;
  })()`);
  if (!clicked)
    throw new Error(`could not find ${label} button in ${selector}`);
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
}

async function screenshot(name) {
  const captured = await command("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(resolve(OUTPUT, name), Buffer.from(captured.data, "base64"));
}

async function setViewport([width, height]) {
  await command("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await delay(350);
}

async function installAlignmentPanel(label, instanceId) {
  if (ALIGNMENT_EVIDENCE === undefined) return;
  const alignment = JSON.parse(
    await readFile(resolve(ROOT, ALIGNMENT_EVIDENCE), "utf8"),
  );
  const measurement = alignment.measurements.find(
    (candidate) => candidate.instanceId === instanceId,
  );
  if (measurement === undefined) {
    throw new Error(`missing wall proxy measurement for ${instanceId}`);
  }
  await evaluate(`(() => {
    let panel = document.querySelector('#wall-proxy-alignment-evidence');
    if (!(panel instanceof HTMLElement)) {
      panel = document.createElement('aside');
      panel.id = 'wall-proxy-alignment-evidence';
      panel.style.cssText = [
        'position:fixed', 'left:20px', 'bottom:20px', 'z-index:2147483647',
        'max-width:min(560px,calc(100vw - 40px))', 'padding:12px 16px',
        'background:rgba(3,8,14,.94)', 'border:1px solid #67d9d0',
        'border-radius:6px', 'color:#e9ffff',
        'font:600 14px/1.4 ui-monospace,monospace', 'white-space:pre-wrap',
        'box-shadow:0 8px 30px rgba(0,0,0,.55)'
      ].join(';');
      document.body.append(panel);
    }
    panel.textContent = [
      ${JSON.stringify(label)},
      'Visible authority: Studio repeated-brush selection / visual bounds',
      'Collision authority: Rust material-voxel gameplayProxy',
      'Measured surfaces: ${String(alignment.measurements.length)}',
      'Maximum gap: ${String(alignment.threshold.measuredMaximumGap)}',
      'Allowed: ${String(alignment.threshold.maximumAllowedGap)} (finest brush cell)',
      'Selected: ${measurement.instanceId} (${measurement.kind})',
      'Visual X/Z: ${measurement.visible.min[0]}..${measurement.visible.max[0]} / ${measurement.visible.min[1]}..${measurement.visible.max[1]}',
      'Proxy X/Z: ${measurement.gameplayProxy.min[0]}..${measurement.gameplayProxy.max[0]} / ${measurement.gameplayProxy.min[1]}..${measurement.gameplayProxy.max[1]}',
      'Selected gap: ${measurement.gap}',
      'Second collision source: no'
    ].join('\\n');
    return true;
  })()`);
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
  await setViewport(PRIMARY_VIEWPORT);
  await command("Page.navigate", {
    url:
      `${HOST}/?root=${encodeURIComponent(ROOT)}` +
      "&project=content%2Fprojects%2Floading-bay.project.json",
  });
  await waitFor(
    `document.querySelector('[data-project-hash="${PROJECT_HASH}"]') !== null`,
    "canonical brush project",
  );
  const initialSubmission = await waitForSubmission(
    1,
    "complete",
    "initial complete Studio submission",
  );
  await mkdir(OUTPUT, { recursive: true });
  await screenshot(OVERVIEW_SCREENSHOT);
  await focus(PRIMARY_FOCUS);
  await installAlignmentPanel(
    "WALL / COLUMN ALIGNMENT · DESKTOP",
    PRIMARY_FOCUS,
  );
  const denseSubmission = await waitForSubmission(
    initialSubmission.count + 1,
    "presentation",
    "dense-wall selection presentation",
  );
  await screenshot(PRIMARY_SCREENSHOT);
  await setViewport(SECONDARY_VIEWPORT);
  await focus(SECONDARY_FOCUS);
  await installAlignmentPanel(
    "DOOR SURROUND / NARROW PASSAGE · NARROW",
    SECONDARY_FOCUS,
  );
  const conservativeSubmission = await waitForSubmission(
    denseSubmission.count + 1,
    "presentation",
    "conservative-wall selection presentation",
  );
  await screenshot(SECONDARY_SCREENSHOT);

  const resizeProof = [];
  for (const [width, height] of [
    [1280, 720],
    [1600, 900],
  ]) {
    await command("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await delay(350);
    resizeProof.push(
      await evaluate(`({
        viewport: [${width}, ${height}],
        canvasCount: document.querySelectorAll('canvas').length,
        rendererStatus: document.querySelector('rusty-studio-viewport')
          ?.getAttribute('data-renderer-status'),
        rendererError: document.querySelector('rusty-studio-viewport')
          ?.getAttribute('data-renderer-error')
      })`),
    );
  }

  await clickButton("header.titlebar button", "File");
  await clickButton("section.file-menu button", "Close Project");
  await waitFor(
    `document.querySelector('[data-project-hash]') === null`,
    "Studio project close",
  );
  await delay(500);
  const disposed = await evaluate(`({
    projectHash: document.querySelector('[data-project-hash]')
      ?.getAttribute('data-project-hash'),
    canvasCount: document.querySelectorAll('canvas').length,
    submissionCount: JSON.parse(
      document.querySelector('loading-bay-studio-root')
        .getAttribute('data-frame-submission-evidence')
    ).count
  })`);

  await clickButton(
    '[data-visual-id="studio-project-open-controls"] button',
    "Open",
  );
  await waitFor(
    `document.querySelector('[data-project-hash="${PROJECT_HASH}"]') !== null &&
      document.querySelectorAll('canvas').length === 1`,
    "Studio project remount",
  );
  const remountedSubmission = await waitForSubmission(
    conservativeSubmission.count + 1,
    "complete",
    "remounted complete Studio submission",
  );
  const remounted = await evaluate(`({
    projectHash: document.querySelector('[data-project-hash]')
      ?.getAttribute('data-project-hash'),
    canvasCount: document.querySelectorAll('canvas').length,
    rendererStatus: document.querySelector('rusty-studio-viewport')
      ?.getAttribute('data-renderer-status'),
    rendererError: document.querySelector('rusty-studio-viewport')
      ?.getAttribute('data-renderer-error')
  })`);

  await command("Page.reload", { ignoreCache: true });
  await waitFor(
    `document.querySelector('[data-project-hash="${PROJECT_HASH}"]') !== null &&
      document.querySelectorAll('canvas').length === 1`,
    "fresh page reconstruction",
  );
  const reloadedSubmission = await waitForSubmission(
    1,
    "complete",
    "reloaded complete Studio submission",
  );
  await focus(PRIMARY_FOCUS);
  const reloadedPresentation = await waitForSubmission(
    reloadedSubmission.count + 1,
    "presentation",
    "reloaded selection presentation",
  );

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
  proof.submissions = {
    initial: initialSubmission,
    denseSelection: denseSubmission,
    conservativeSelection: conservativeSubmission,
    remounted: remountedSubmission,
    reloaded: reloadedSubmission,
    reloadedSelection: reloadedPresentation,
  };
  proof.lifecycle = {
    resize: resizeProof,
    disposed,
    remounted,
    reloaded: {
      projectHash: proof.projectHash,
      canvasCount: proof.canvasCount,
      rendererStatus: proof.viewport.status,
      rendererError: proof.viewport.rendererError,
    },
  };
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
    viewport: PRIMARY_VIEWPORT,
    secondaryViewport: SECONDARY_VIEWPORT,
    host: HOST,
    screenshots: [
      `docs/evidence/${OVERVIEW_SCREENSHOT}`,
      `docs/evidence/${PRIMARY_SCREENSHOT}`,
      `docs/evidence/${SECONDARY_SCREENSHOT}`,
    ],
    rendererSubmission: {
      engineRevision: ENGINE_REVISION,
      event: "rusty_studio_viewport_frame_submitted.v1",
      outlet: "StudioShellComponent.frameSubmitted",
      collection:
        "public shell output only; no WebGL, Three, private component, or second-loop access",
    },
  };
  await writeFile(
    resolve(OUTPUT, `${EVIDENCE_STEM}-studio-browser.json`),
    `${JSON.stringify(proof, null, 2)}\n`,
  );
  process.stdout.write(`${JSON.stringify(proof, null, 2)}\n`);
} finally {
  socket?.close();
  browser.kill("SIGTERM");
  await new Promise((resolveExit) => browser.once("exit", resolveExit));
  await rm(profile, {
    recursive: true,
    force: true,
    maxRetries: 8,
    retryDelay: 125,
  });
}
