import { spawn } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { relative, resolve } from "node:path";
import { createInterface } from "node:readline";

const ROOT = resolve(import.meta.dirname, "..");
const OUTPUT = resolve(ROOT, "docs/evidence");
const HOST = process.env.RUSTY_STUDIO_ACTOR_HOST ?? "http://127.0.0.1:4396";
const PROTOCOL_VERSION = 14;
const MANIFEST = JSON.parse(
  await readFile(
    resolve(ROOT, "content/assets/actor-kit/source-manifest.json"),
    "utf8",
  ),
);
const ENGINE_REVISION = (JSON.parse(
  await readFile(resolve(ROOT, "package.json"), "utf8"),
).dependencies["@rusty-engine/studio-viewport"].match(/#([0-9a-f]{40})&/u) ??
  [])[1];
if (ENGINE_REVISION === undefined) {
  throw new Error(
    "studio-viewport dependency is not pinned to an exact revision",
  );
}

class Adapter {
  #child;
  #lines;
  #pending = [];
  #stderr = "";

  constructor() {
    this.#child = spawn(
      "cargo",
      [
        "run",
        "--locked",
        "--quiet",
        "-p",
        "loading-bay-game",
        "--bin",
        "studio-adapter",
      ],
      { cwd: ROOT, stdio: ["pipe", "pipe", "pipe"] },
    );
    this.#child.stderr.setEncoding("utf8");
    this.#child.stderr.on("data", (chunk) => {
      this.#stderr += chunk;
    });
    this.#lines = createInterface({ input: this.#child.stdout });
    this.#lines.on("line", (line) => {
      const pending = this.#pending.shift();
      if (pending === undefined) {
        throw new Error(`unexpected Studio adapter response: ${line}`);
      }
      pending.resolve(JSON.parse(line));
    });
    this.#child.on("exit", (code) => {
      if (code !== 0 && this.#pending.length > 0) {
        const error = new Error(
          `Studio adapter exited ${String(code)}\n${this.#stderr}`,
        );
        for (const pending of this.#pending.splice(0)) {
          pending.reject(error);
        }
      }
    });
  }

  async send(request) {
    const response = await new Promise((resolveResponse, reject) => {
      this.#pending.push({ resolve: resolveResponse, reject });
      this.#child.stdin.write(`${JSON.stringify(request)}\n`);
    });
    if (response.type === "rejected") {
      throw new Error(
        `${request.type} rejected: ${JSON.stringify(response.error)}`,
      );
    }
    return response;
  }

  async close() {
    await new Promise((resolveExit, reject) => {
      this.#child.once("exit", (code) => {
        if (code === 0) resolveExit();
        else
          reject(
            new Error(`Studio adapter exited ${String(code)}\n${this.#stderr}`),
          );
      });
      this.#child.stdin.end();
    });
  }
}

function projectHash(response) {
  return response.project.identity.projectHash;
}

function sceneRevision(response) {
  return response.project.identity.sceneRevision;
}

function projectedActorClips(response, entityIds) {
  const expectedEntities = new Set(entityIds);
  return response.project.projection.ops
    .filter(
      (operation) =>
        operation.op === "createAnimatedMeshInstance" &&
        expectedEntities.has(operation.instance.metadata.sourceEntity),
    )
    .map((operation) => ({
      entityId: operation.instance.metadata.sourceEntity,
      asset: operation.instance.asset,
      clip: operation.instance.playback?.clip ?? null,
    }))
    .sort((left, right) => left.entityId - right.entityId);
}

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

async function run(command, args) {
  const child = spawn(command, args, {
    cwd: ROOT,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "";
  let stderr = "";
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const code = await new Promise((resolveExit) =>
    child.once("exit", resolveExit),
  );
  if (code !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} exited ${String(code)}\n${stdout}\n${stderr}`,
    );
  }
  return stdout;
}

const sandbox = await mkdtemp(resolve(tmpdir(), "rusty-actor-studio-"));
const projectFile = resolve(
  sandbox,
  "content/projects/loading-bay.project.json",
);
const authoringEvidenceFile = resolve(sandbox, "actor-authoring.json");
await cp(resolve(ROOT, "content"), resolve(sandbox, "content"), {
  recursive: true,
});

let browser;
let socket;
const profile = await mkdtemp(resolve(tmpdir(), "rusty-actor-chromium-"));
const port = 9437;
let nextId = 1;
const pending = new Map();

async function target() {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    try {
      const targets = await (
        await fetch(`http://127.0.0.1:${String(port)}/json/list`)
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
  for (let attempt = 0; attempt < 480; attempt += 1) {
    if (await evaluate(expression)) return;
    await delay(125);
  }
  const diagnostic = await evaluate(`({
    title: document.title,
    projectHash: document.querySelector('[data-project-hash]')
      ?.getAttribute('data-project-hash'),
    text: document.body?.innerText?.slice(0, 2000)
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
      return evidence.count >= ${String(minimumCount)} &&
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
  if (!clicked) {
    throw new Error(`could not find ${label} button in ${selector}`);
  }
}

async function clickFileCommand(label) {
  const commandVisible = await evaluate(
    `Array.from(document.querySelectorAll('section.file-menu button')).some(
      (candidate) => candidate.textContent.trim() === ${JSON.stringify(label)}
    )`,
  );
  if (!commandVisible) {
    await clickButton("header.titlebar button", "File");
  }
  await clickButton("section.file-menu button", label);
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
    row.dispatchEvent(new MouseEvent('dblclick', {
      bubbles: true,
      cancelable: true,
      view: window
    }));
    return true;
  })()`);
}

async function zoomTowardSelection() {
  for (let step = 0; step < 1; step += 1) {
    await command("Input.dispatchMouseEvent", {
      type: "mouseWheel",
      x: 800,
      y: 450,
      deltaX: 0,
      deltaY: -600,
    });
    await delay(75);
  }
}

async function screenshot(name) {
  const fileMenuVisible = await evaluate(
    `Array.from(document.querySelectorAll('section.file-menu button')).some(
      (candidate) => candidate.getClientRects().length > 0
    )`,
  );
  if (fileMenuVisible) {
    await clickButton("header.titlebar button", "File");
  }
  await delay(100);
  const captured = await command("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  await writeFile(resolve(OUTPUT, name), Buffer.from(captured.data, "base64"));
}

try {
  await run("node", [
    "scripts/author-actor-kit.mjs",
    projectFile,
    authoringEvidenceFile,
    sandbox,
  ]);
  const authoringEvidence = JSON.parse(
    await readFile(authoringEvidenceFile, "utf8"),
  );

  browser = spawn(
    process.env.CHROMIUM_BIN ?? "chromium",
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--enable-unsafe-swiftshader",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      `--remote-debugging-port=${String(port)}`,
      `--user-data-dir=${profile}`,
      "--window-size=1600,900",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
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
      `${HOST}/?root=${encodeURIComponent(sandbox)}` +
      "&project=content%2Fprojects%2Floading-bay.project.json",
  });
  await waitFor(
    `document.querySelector('[data-project-hash="${authoringEvidence.project.finalHash}"]') !== null`,
    "canonical imported actor library",
  );
  const baseline = await waitForSubmission(
    1,
    "complete",
    "actor-library baseline submission",
  );

  await mkdir(OUTPUT, { recursive: true });
  await focus("cargo-loader-arrival");
  await zoomTowardSelection();
  const groundingDesktop = await waitForSubmission(
    baseline.count + 1,
    "presentation",
    "grounded actor selection",
  );
  await screenshot("renderable-grounding-desktop.png");
  await command("Emulation.setDeviceMetricsOverride", {
    width: 1000,
    height: 844,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await delay(350);
  await focus("generator-coolant-leak");
  await zoomTowardSelection();
  const groundingNarrow = await waitForSubmission(
    groundingDesktop.count + 1,
    "presentation",
    "grounded hazard selection",
  );
  await screenshot("renderable-grounding-narrow.png");
  await command("Emulation.setDeviceMetricsOverride", {
    width: 1600,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await delay(350);

  const adapter = new Adapter();
  let previewProject = await adapter.send({
    type: "openProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "actor-preview-open",
    root: sandbox,
    projectFile: relative(sandbox, projectFile),
  });
  const previewInstances = [];
  let entityId = 9000;
  for (const [variantIndex, variant] of MANIFEST.variants.entries()) {
    for (const [clipIndex, clip] of variant.clips.entries()) {
      entityId += 1;
      const name = `actor-proof-${variant.file.replace(/\.glb$/u, "")}-${clip.id}`;
      previewProject = await adapter.send({
        type: "createSceneObject",
        protocolVersion: PROTOCOL_VERSION,
        requestId: `actor-preview-${String(entityId)}`,
        expectedProjectHash: projectHash(previewProject),
        expectedSceneRevision: sceneRevision(previewProject),
        object: {
          entityId,
          name,
          parentEntityId: null,
          childOrder: entityId,
          transform: {
            translation: [40 + clipIndex * 3, 4, 10 + variantIndex * 5],
            rotation: [0, 0, 0, 1],
            scale: [1, 1, 1],
          },
          appearance: {
            kind: "animatedMesh",
            asset: variant.assetId,
            visible: true,
            clip: clip.id,
          },
          collision: null,
          kinematic: null,
        },
      });
      previewInstances.push({
        entityId,
        name,
        asset: variant.assetId,
        clip: clip.id,
      });
    }
  }
  const previewHash = projectHash(previewProject);
  const projectedClips = projectedActorClips(
    previewProject,
    previewInstances.map(({ entityId }) => entityId),
  );
  await adapter.close();

  const reopenedAdapter = new Adapter();
  const reopenedProject = await reopenedAdapter.send({
    type: "openProject",
    protocolVersion: PROTOCOL_VERSION,
    requestId: "actor-preview-reopen",
    root: sandbox,
    projectFile: relative(sandbox, projectFile),
  });
  const reopenedClips = projectedActorClips(
    reopenedProject,
    previewInstances.map(({ entityId }) => entityId),
  );
  await reopenedAdapter.close();
  if (JSON.stringify(reopenedClips) !== JSON.stringify(projectedClips)) {
    throw new Error("fresh-adapter reconstruction changed actor playback");
  }

  await clickFileCommand("Refresh from Owner");
  await waitFor(
    `document.querySelector('[data-project-hash="${previewHash}"]') !== null`,
    "Studio actor preview refresh",
  );
  const preview = await waitForSubmission(
    baseline.count + 1,
    "complete",
    "animated actor preview submission",
  );

  const expectedClips = previewInstances.map(({ clip }) => clip).sort();
  const actualClipList = projectedClips.map(({ clip }) => clip).sort();
  if (JSON.stringify(actualClipList) !== JSON.stringify(expectedClips)) {
    throw new Error(
      `Studio projection clips ${JSON.stringify(actualClipList)} did not match ${JSON.stringify(expectedClips)}`,
    );
  }

  // The canonical project already owns both animated asset identities. Use
  // the post-selection baseline after the asynchronous project resources have
  // settled; the first complete frame can legitimately precede mesh/prop
  // resolution and would misattribute those existing resources to the later
  // temporary actor preview.
  const baselineStats = groundingNarrow.latest.submission.statistics;
  const previewStats = preview.latest.submission.statistics;
  const animatedDelta =
    previewStats.animatedInstanceCount.value -
    baselineStats.animatedInstanceCount.value;
  if (animatedDelta !== previewInstances.length) {
    throw new Error(
      `expected ${String(previewInstances.length)} new animated instances, observed ${String(animatedDelta)}`,
    );
  }
  const geometryDelta =
    previewStats.geometryResourceCount.value -
    baselineStats.geometryResourceCount.value;
  const materialDelta =
    previewStats.materialResourceCount.value -
    baselineStats.materialResourceCount.value;
  const textureDelta =
    previewStats.textureResourceCount.value -
    baselineStats.textureResourceCount.value;
  if (geometryDelta !== 0 || materialDelta !== 0 || textureDelta !== 0) {
    throw new Error(
      `resident actor identities were duplicated: geometry +${String(geometryDelta)}, ` +
        `materials +${String(materialDelta)}, textures +${String(textureDelta)}, ` +
        `identities ${String(MANIFEST.variants.length)}`,
    );
  }

  await focus("actor-proof-arc-warden-attack");
  await zoomTowardSelection();
  const attack = await waitForSubmission(
    preview.count + 1,
    "presentation",
    "Arc Warden attack selection",
  );
  await screenshot("actor-kit-studio-arc-warden-attack.png");
  await focus("actor-proof-bay-rusher-death");
  await zoomTowardSelection();
  const death = await waitForSubmission(
    attack.count + 1,
    "presentation",
    "Bay Rusher death selection",
  );
  await screenshot("actor-kit-studio-bay-rusher-death.png");

  const resize = [];
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
    resize.push(
      await evaluate(`({
        viewport: [${String(width)}, ${String(height)}],
        canvasCount: document.querySelectorAll('canvas').length,
        rendererStatus: document.querySelector('rusty-studio-viewport')
          ?.getAttribute('data-renderer-status'),
        rendererError: document.querySelector('rusty-studio-viewport')
          ?.getAttribute('data-renderer-error')
      })`),
    );
  }

  await clickFileCommand("Close Project");
  await waitFor(
    `document.querySelector('[data-project-hash]') === null`,
    "Studio actor project disposal",
  );
  const disposed = await evaluate(`({
    projectHash: document.querySelector('[data-project-hash]')
      ?.getAttribute('data-project-hash'),
    canvasCount: document.querySelectorAll('canvas').length
  })`);

  await clickButton(
    '[data-visual-id="studio-project-open-controls"] button',
    "Open",
  );
  await waitFor(
    `document.querySelector('[data-project-hash="${previewHash}"]') !== null &&
      document.querySelectorAll('canvas').length === 1`,
    "Studio actor project remount",
  );
  const remounted = await waitForSubmission(
    death.count + 1,
    "complete",
    "remounted actor submission",
  );

  await command("Page.reload", { ignoreCache: true });
  await waitFor(
    `document.querySelector('[data-project-hash="${previewHash}"]') !== null &&
      document.querySelectorAll('canvas').length === 1`,
    "fresh-page actor reconstruction",
  );
  const reloaded = await waitForSubmission(
    1,
    "complete",
    "reloaded actor submission",
  );

  const proof = {
    schemaVersion: 1,
    authority:
      "temporary Studio scene objects exercise imported actors; gameplay posture binding remains deferred to VC8",
    engineRevision: ENGINE_REVISION,
    sourceProjectHash: authoringEvidence.project.finalHash,
    previewProjectHash: previewHash,
    previewInstances,
    clips: actualClipList,
    projectedClips,
    reopenedClips,
    submissions: {
      baseline,
      groundingDesktop,
      groundingNarrow,
      preview,
      attackSelection: attack,
      deathSelection: death,
      remounted,
      reloaded,
    },
    resourceDeltas: {
      renderHandles:
        previewStats.renderHandleCount.value -
        baselineStats.renderHandleCount.value,
      geometry: geometryDelta,
      materials: materialDelta,
      textures: textureDelta,
      animatedInstances: animatedDelta,
    },
    lifecycle: {
      resize,
      disposed,
      remounted: {
        projectHash: previewHash,
        canvasCount: 1,
        statistics: remounted.latest.submission.statistics,
      },
      reloaded: {
        projectHash: previewHash,
        canvasCount: 1,
        statistics: reloaded.latest.submission.statistics,
      },
    },
    capture: {
      host: HOST,
      viewport: [1600, 900],
      screenshots: [
        "docs/evidence/renderable-grounding-desktop.png",
        "docs/evidence/renderable-grounding-narrow.png",
        "docs/evidence/actor-kit-studio-arc-warden-attack.png",
        "docs/evidence/actor-kit-studio-bay-rusher-death.png",
      ],
      rendererSubmission: {
        event: "rusty_studio_viewport_frame_submitted.v1",
        outlet: "StudioShellComponent.frameSubmitted",
        collection:
          "public shell output and DOM readouts only; no WebGL, Three, private component, or second-loop access",
      },
    },
  };
  await writeFile(
    resolve(OUTPUT, "actor-kit-studio-browser.json"),
    `${JSON.stringify(proof, null, 2)}\n`,
  );
  process.stdout.write(`${JSON.stringify(proof, null, 2)}\n`);
} finally {
  socket?.close();
  if (browser !== undefined) {
    browser.kill("SIGTERM");
    await new Promise((resolveExit) => browser.once("exit", resolveExit));
  }
  await rm(profile, {
    recursive: true,
    force: true,
    maxRetries: 8,
    retryDelay: 125,
  });
  await rm(sandbox, {
    recursive: true,
    force: true,
    maxRetries: 8,
    retryDelay: 125,
  });
}
