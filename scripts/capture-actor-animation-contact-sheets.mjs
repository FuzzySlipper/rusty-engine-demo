import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import {
  cpSync,
  existsSync,
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
const HOST = process.env.RUSTY_STUDIO_ANIMATION_HOST ?? "http://127.0.0.1:4396";
const CHROMIUM = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
const CLIPS = ["idle", "run", "jump", "attack", "hit", "death"];
const ACTORS = [
  {
    asset: "mesh-animation/bay-rusher",
    entity: "cargo-loader-arrival",
    stem: "bay-rusher",
  },
  {
    asset: "mesh-animation/arc-warden",
    entity: "gantry-sentry-generator",
    stem: "arc-warden",
  },
];
const engineRevision = JSON.parse(
  readFileSync(resolve(ROOT, "engine-source.json"), "utf8"),
).commit;
const projectText = readFileSync(
  resolve(ROOT, "content/projects/loading-bay.project.json"),
  "utf8",
);
const expectedProjectHash = createHash("sha256").update(projectText).digest("hex");

const proofRoot = mkdtempSync(join(tmpdir(), "loading-bay-studio-animation-"));
const stagedOutput = mkdtempSync(
  resolve(dirname(OUTPUT), ".animated-mesh-contact-sheets-stage-"),
);
cpSync(resolve(ROOT, "content"), resolve(proofRoot, "content"), { recursive: true });

let browser;
try {
  const debuggingPort = await reservePort();
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
      `--user-data-dir=${resolve(proofRoot, "chromium")}`,
      "--window-size=1600,1000",
      "about:blank",
    ],
    { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] },
  );
  const browserOutput = captureOutput(browser);
  const target = await waitForChromiumTarget(debuggingPort, browser, browserOutput);
  const client = await connectDevTools(target.webSocketDebuggerUrl);
  try {
    await client.send("Emulation.setDeviceMetricsOverride", {
      width: 1600,
      height: 1000,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await client.send("Page.enable");
    await client.send("Runtime.enable");
    await client.send("Page.navigate", {
      url:
        `${HOST}/?root=${encodeURIComponent(proofRoot)}` +
        "&project=content%2Fprojects%2Floading-bay.project.json",
    });
    await waitForExpression(
      client,
      "document.querySelector('[data-project-hash]')?.getAttribute('data-project-hash')?.length > 0",
      "canonical Loading Bay project",
      60_000,
    );
    const openedHash = await attribute(client, "[data-project-hash]", "data-project-hash");
    const captures = [];
    for (const actor of ACTORS) {
      await selectHierarchyEntity(client, actor.entity);
      await openAnimationInspection(client);
      await waitForExpression(
        client,
        `document.querySelector('[data-visual-id="animation-inspection-workflow"]')?.innerText.includes(${JSON.stringify(actor.asset)}) === true`,
        `${actor.asset} Animation Inspection selection`,
      );
      for (const clip of CLIPS) {
        await setControlValue(
          client,
          'select[aria-label="Animation inspection clip"]',
          clip,
        );
        await waitForExpression(
          client,
          `document.querySelector('[data-visual-id="animation-inspection-readout"]')?.textContent.includes(${JSON.stringify(`${clip} 0%`)}) === true`,
          `${actor.asset}/${clip} initial public sample`,
        );
        await clickExactButton(client, "Capture 5-frame sheet");
        await waitForExpression(
          client,
          `document.querySelector('img[alt="${clip} animation contact sheet"]')?.getAttribute('src')?.startsWith('data:image/png;base64,') === true`,
          `${actor.asset}/${clip} public contact sheet`,
          30_000,
        );
        const sample = await evaluateValue(
          client,
          `(() => {
            const workflow = document.querySelector('[data-visual-id="animation-inspection-workflow"]');
            const image = workflow?.querySelector(${JSON.stringify(`img[alt="${clip} animation contact sheet"]`)});
            return {
              readout: workflow?.querySelector('[data-visual-id="animation-inspection-readout"]')?.textContent?.trim() ?? null,
              skinningFacts: workflow?.querySelector('[data-visual-id="animation-skinning-facts"]')?.textContent?.replace(/\\s+/gu, ' ').trim() ?? null,
              pngDataUrl: image?.getAttribute('src') ?? null,
            };
          })()`,
        );
        if (
          typeof sample.readout !== "string" ||
          typeof sample.skinningFacts !== "string" ||
          typeof sample.pngDataUrl !== "string"
        ) {
          throw new Error(`${actor.asset}/${clip} omitted public inspection facts`);
        }
        const visibleFacts = `${sample.readout} ${sample.skinningFacts}`.toLowerCase();
        for (const [required, pattern] of [
          ["finite inverse binds", /inverse binds\s*\d+\s*· finite/u],
          ["normalized weights", /weights\s*normalized/u],
          ["zero invalid weights", /0 invalid/u],
          ["linear interpolation", /interpolation[^|]*linear/u],
          ["independent root", /root independent/u],
          ["independent skeleton", /skeleton independent/u],
          ["zero diagnostics", /0 diagnostics/u],
        ]) {
          if (!pattern.test(visibleFacts)) {
            throw new Error(
              `${actor.asset}/${clip} omitted ${required}: ${sample.readout} | ${sample.skinningFacts}`,
            );
          }
        }
        const bytes = pngBytes(sample.pngDataUrl);
        const fileName = `${actor.stem}-${clip}-contact-sheet.png`;
        writeFileSync(resolve(stagedOutput, fileName), bytes);
        captures.push({
          asset: actor.asset,
          entity: actor.entity,
          clip,
          normalizedTimes: [0, 0.25, 0.5, 0.75, 1],
          file: fileName,
          imageSha256: createHash("sha256").update(bytes).digest("hex"),
          readout: sample.readout,
          skinningFacts: sample.skinningFacts,
        });
      }
    }

    const hashAfterInspection = await attribute(
      client,
      "[data-project-hash]",
      "data-project-hash",
    );
    await client.send("Emulation.setDeviceMetricsOverride", {
      width: 1000,
      height: 844,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await delay(300);
    const narrowScreenshot = await client.send("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: false,
    });
    writeFileSync(
      resolve(stagedOutput, "animation-inspection-narrow.png"),
      Buffer.from(narrowScreenshot.data, "base64"),
    );

    await client.send("Page.navigate", { url: "about:blank" });
    await client.send("Page.navigate", {
      url:
        `${HOST}/?root=${encodeURIComponent(proofRoot)}` +
        "&project=content%2Fprojects%2Floading-bay.project.json",
    });
    await waitForExpression(
      client,
      `document.querySelector('[data-project-hash]')?.getAttribute('data-project-hash') === ${JSON.stringify(openedHash)}`,
      "fresh-page project reopen",
      60_000,
    );
    await selectHierarchyEntity(client, ACTORS[0].entity);
    await openAnimationInspection(client);
    await waitForExpression(
      client,
      `document.querySelector('[data-visual-id="animation-inspection-workflow"]')?.innerText.includes(${JSON.stringify(ACTORS[0].asset)}) === true`,
      "fresh-page public animation inspection",
    );

    writeFileSync(
      resolve(stagedOutput, "certification.json"),
      `${JSON.stringify(
        {
          schemaVersion: 2,
          engineRevision,
          projectFile: "content/projects/loading-bay.project.json",
          projectSourceSha256: expectedProjectHash,
          openedProjectHash: openedHash,
          projectHashAfterDisposableInspection: hashAfterInspection,
          authoringStateUnchanged: openedHash === hashAfterInspection,
          viewport: [1600, 1000],
          narrowViewport: [1000, 844],
          workflow: "Tools > Animation Inspection through the shared Studio viewport",
          actors: ACTORS,
          clips: CLIPS,
          captures,
          lifecycle: {
            freshPageReopen: true,
            freshPageProjectHash: openedHash,
            freshPageAnimationInspection: true,
          },
        },
        null,
        2,
      )}\n`,
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
  console.log(
    `captured ${String(ACTORS.length * CLIPS.length)} public Studio animation contact sheets`,
  );
} finally {
  if (browser !== undefined) await terminate(browser);
  rmSync(proofRoot, { recursive: true, force: true });
  rmSync(stagedOutput, { recursive: true, force: true });
}

async function selectHierarchyEntity(client, label) {
  await evaluateValue(
    client,
    `(() => {
      const input = document.querySelector('input[aria-label="Filter hierarchy"]');
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      setter.call(input, ${JSON.stringify(label)});
      input.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    })()`,
  );
  await waitForExpression(
    client,
    `Array.from(document.querySelectorAll('[role="treeitem"]')).some((row) => row.textContent.includes(${JSON.stringify(label)}))`,
    `${label} hierarchy row`,
  );
  await evaluateValue(
    client,
    `(() => {
      const row = Array.from(document.querySelectorAll('[role="treeitem"]')).find((candidate) => candidate.textContent.includes(${JSON.stringify(label)}));
      row.click();
      row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, button: 0 }));
      return true;
    })()`,
  );
  await delay(250);
}

async function openAnimationInspection(client) {
  const active = await evaluateValue(
    client,
    `document.querySelector('[data-animation-inspection-tool]')?.getAttribute('data-animation-inspection-tool') === 'active'`,
  );
  if (active) return;
  await clickExactButton(client, "Tools");
  await clickExactButton(client, "Animation Inspection");
  await waitForExpression(
    client,
    `document.querySelector('[data-animation-inspection-tool]')?.getAttribute('data-animation-inspection-tool') === 'active'`,
    "Animation Inspection tool",
  );
}

async function clickExactButton(client, label) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const button = Array.from(document.querySelectorAll('button')).find(
        (candidate) => candidate.textContent.trim() === ${JSON.stringify(label)} && candidate.getClientRects().length > 0
      );
      if (button === undefined) return false;
      button.click();
      return true;
    })()`,
  );
  if (!clicked) throw new Error(`could not find visible ${label} button`);
}

async function setControlValue(client, selector, value) {
  await evaluateValue(
    client,
    `(() => {
      const control = document.querySelector(${JSON.stringify(selector)});
      const prototype = control instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(prototype, 'value').set.call(control, ${JSON.stringify(value)});
      control.dispatchEvent(new Event('input', { bubbles: true }));
      control.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    })()`,
  );
}

function pngBytes(dataUrl) {
  const match = /^data:image\/png;base64,(.+)$/u.exec(dataUrl);
  if (match?.[1] === undefined) throw new Error("Studio capture did not return a PNG");
  return Buffer.from(match[1], "base64");
}

async function attribute(client, selector, name) {
  return evaluateValue(
    client,
    `document.querySelector(${JSON.stringify(selector)})?.getAttribute(${JSON.stringify(name)}) ?? null`,
  );
}

function captureOutput(child) {
  let output = "";
  child.stdout.on("data", (chunk) => {
    output += String(chunk);
  });
  child.stderr.on("data", (chunk) => {
    output += String(chunk);
  });
  return () => output;
}

async function reservePort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error("could not reserve a port");
  }
  await new Promise((resolveClose) => server.close(resolveClose));
  return address.port;
}

async function waitForChromiumTarget(port, child, output) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Chromium exited early\n${output()}`);
    try {
      const targets = await (
        await fetch(`http://127.0.0.1:${String(port)}/json/list`)
      ).json();
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
    close() {
      socket.close();
    },
  };
}

async function evaluateValue(client, expression) {
  const result = await client.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails !== undefined) {
    throw new Error(
      result.exceptionDetails.exception?.description ?? "browser evaluation failed",
    );
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
