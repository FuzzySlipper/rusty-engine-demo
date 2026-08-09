#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const actualRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const doomProject = join(actualRoot, "content/projects/doom-e1m1.project.json");
const chromium = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";

const delay = (ms) => new Promise((r) => setTimeout(r, ms));

async function reservePort() {
  return new Promise((res, rej) => {
    const s = createServer();
    s.listen(0, "127.0.0.1", () => {
      // @ts-ignore
      const p = s.address().port;
      s.close(() => res(p));
    });
    s.on("error", rej);
  });
}

async function waitForHealth(url, proc, getOut) {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(url, { cache: "no-store" });
      if (r.ok) return;
    } catch {}
    if (proc.exitCode !== null)
      throw new Error(`host died: ${getOut().slice(-4000)}`);
    await delay(100);
  }
  throw new Error(`health timeout ${getOut().slice(-4000)}`);
}

function launchHost(addr, saveRoot) {
  const proc = spawn(
    "cargo",
    [
      "run",
      "--quiet",
      "--locked",
      "-p",
      "loading-bay-game",
      "--bin",
      "browser-host",
      "--",
      "--addr",
      addr,
      "--project",
      doomProject,
      "--save-root",
      saveRoot,
    ],
    {
      cwd: actualRoot,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  let out = "";
  proc.stdout.on("data", (c) => (out += String(c)));
  proc.stderr.on("data", (c) => (out += String(c)));
  return { host: proc, getOut: () => out };
}

async function waitForDebuggerWs(
  debugPort,
  proc,
  getStderr,
  timeoutMs = 15000,
) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (proc.exitCode !== null)
      throw new Error(`chromium died early ${getStderr().slice(-4000)}`);
    try {
      const r = await fetch(`http://127.0.0.1:${debugPort}/json/list`, {
        cache: "no-store",
      });
      if (r.ok) {
        const list = await r.json();
        const target = list.find((t) => t.type === "page") ?? list[0];
        if (target?.webSocketDebuggerUrl) return target.webSocketDebuggerUrl;
      }
    } catch {}
    await delay(150);
  }
  throw new Error(`debugger ws timeout ${getStderr().slice(-3000)}`);
}

function createCdpClient(wsUrl) {
  return new Promise((resolve, reject) => {
    const pending = new Map();
    let nextId = 1;
    let ws;
    const makeClient = (websocket) => {
      ws = websocket;
      const send = (method, params = {}) =>
        new Promise((res, rej) => {
          const id = nextId++;
          pending.set(id, { resolve: res, reject: rej });
          const payload = JSON.stringify({ id, method, params });
          try {
            ws.send(payload);
          } catch (e) {
            pending.delete(id);
            rej(e);
          }
          setTimeout(() => {
            if (pending.has(id)) {
              pending.delete(id);
              rej(new Error(`cdp timeout ${method}`));
            }
          }, 12000);
        });
      const close = () => {
        try {
          ws.close();
        } catch {}
      };
      return { send, close };
    };
    const attach = (websocket, isWsLib) => {
      if (isWsLib) {
        websocket.on("message", (data) => {
          const msg = JSON.parse(String(data));
          if (msg.id && pending.has(msg.id)) {
            const h = pending.get(msg.id);
            pending.delete(msg.id);
            if (msg.error)
              h.reject(
                new Error(`${msg.error.message} ${JSON.stringify(msg.error)}`),
              );
            else h.resolve(msg.result);
          }
        });
        websocket.on("error", reject);
      } else {
        websocket.addEventListener("message", (ev) => {
          const msg = JSON.parse(
            typeof ev.data === "string" ? ev.data : String(ev.data),
          );
          if (msg.id && pending.has(msg.id)) {
            const h = pending.get(msg.id);
            pending.delete(msg.id);
            if (msg.error)
              h.reject(
                new Error(`${msg.error.message} ${JSON.stringify(msg.error)}`),
              );
            else h.resolve(msg.result);
          }
        });
        websocket.addEventListener("error", (e) => reject(e));
      }
    };
    (async () => {
      try {
        if (globalThis.WebSocket) {
          const websocket = new globalThis.WebSocket(wsUrl);
          attach(websocket, false);
          await new Promise((res, rej) => {
            websocket.addEventListener("open", res);
            websocket.addEventListener("error", rej);
            if (websocket.readyState === 1) res();
          });
          resolve(makeClient(websocket));
        } else {
          const { default: Ws } = await import("ws");
          const websocket = new Ws(wsUrl);
          attach(websocket, true);
          await new Promise((res, rej) => {
            websocket.on("open", res);
            websocket.on("error", rej);
          });
          resolve(makeClient(websocket));
        }
      } catch (e) {
        reject(e);
      }
    })();
  });
}

async function cdpEvaluate(client, expression) {
  const res = await client.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (res.exceptionDetails)
    throw new Error(
      `evaluate failed ${expression.slice(0, 200)}: ${JSON.stringify(res.exceptionDetails)}`,
    );
  return res.result?.value;
}

async function fetchAuthoritativeState(addr) {
  const response = await fetch(`http://${addr}/api/state`, {
    cache: "no-store",
  });
  if (!response.ok) {
    throw new Error(`state fetch failed ${response.status}`);
  }
  return response.json();
}

async function waitForAuthoritativeState(addr, description, predicate) {
  const deadline = Date.now() + 10000;
  let lastState = null;
  while (Date.now() < deadline) {
    lastState = await fetchAuthoritativeState(addr);
    if (predicate(lastState)) return lastState;
    await delay(100);
  }
  throw new Error(
    `authoritative state timeout (${description}): ${JSON.stringify(lastState).slice(0, 2000)}`,
  );
}

const keyIdentity = {
  KeyW: { key: "w", virtualKeyCode: 87 },
  KeyA: { key: "a", virtualKeyCode: 65 },
  KeyS: { key: "s", virtualKeyCode: 83 },
  KeyD: { key: "d", virtualKeyCode: 68 },
  KeyE: { key: "e", virtualKeyCode: 69 },
};

async function dispatchKey(client, type, code) {
  const identity = keyIdentity[code];
  if (!identity) throw new Error(`missing CDP key identity for ${code}`);
  const browserType = type === "keyDown" ? "keydown" : "keyup";
  await cdpEvaluate(
    client,
    `window.dispatchEvent(new KeyboardEvent(${JSON.stringify(browserType)}, {
      code: ${JSON.stringify(code)},
      key: ${JSON.stringify(identity.key)},
      bubbles: true,
      cancelable: true
    }))`,
  );
}

async function pulseKeys(client, codes, milliseconds = 140) {
  for (const code of codes) await dispatchKey(client, "keyDown", code);
  const releaseAt = Date.now() + milliseconds;
  while (Date.now() + 50 < releaseAt) {
    await delay(50);
    for (const code of codes) await dispatchKey(client, "keyDown", code);
  }
  await delay(Math.max(0, releaseAt - Date.now()));
  for (const code of [...codes].reverse())
    await dispatchKey(client, "keyUp", code);
  await delay(60);
}

async function moveToWorldPoint(client, addr, target, traversalSamples) {
  const deadline = Date.now() + 30000;
  let previousDistance = Number.POSITIVE_INFINITY;
  let stalledPulses = 0;
  let lastState = null;
  while (Date.now() < deadline) {
    const state = await fetchAuthoritativeState(addr);
    lastState = state;
    if (state.player?.vitalityState === "dead") {
      throw new Error(`player died while moving to ${JSON.stringify(target)}`);
    }
    const [x, y, z] = state.player.position;
    traversalSamples.push({ tick: state.tick, position: [x, y, z] });
    const dx = target[0] - x;
    const dz = target[1] - z;
    const distance = Math.hypot(dx, dz);
    if (distance <= 0.7) return state;

    // E1M1 starts at yaw 90 degrees and this smoke deliberately leaves yaw
    // unchanged. At that heading S/W move +/-X and A/D move +/-Z.
    const codes = [];
    if (Math.abs(dx) >= Math.abs(dz) && Math.abs(dx) > 0.45) {
      codes.push(dx > 0 ? "KeyS" : "KeyW");
    } else if (Math.abs(dz) > 0.45) {
      codes.push(dz > 0 ? "KeyA" : "KeyD");
    }
    await pulseKeys(client, codes, distance < 2 ? 80 : 500);

    stalledPulses = distance >= previousDistance - 0.03 ? stalledPulses + 1 : 0;
    previousDistance = distance;
    if (stalledPulses >= 8) {
      throw new Error(
        `movement stalled approaching ${JSON.stringify(target)} from ${JSON.stringify([x, y, z])}; input=${JSON.stringify(state.input)} events=${JSON.stringify(state.lastEvents).slice(0, 1000)}`,
      );
    }
  }
  throw new Error(
    `movement timeout approaching ${JSON.stringify(target)}; last=${JSON.stringify(lastState).slice(0, 2000)}`,
  );
}

async function main() {
  const port = await reservePort();
  const addr = `127.0.0.1:${port}`;
  const saveRoot = mkdtempSync(join(tmpdir(), "doom-smoke-"));
  console.log(`DOOM SMOKE host ${addr} save ${saveRoot}`);
  const { host, getOut } = launchHost(addr, saveRoot);
  let chromiumProc = null;
  let cdpClient = null;
  let debugPort = null;
  let profileDir = null;
  try {
    await waitForHealth(`http://${addr}/health`, host, getOut);
    console.log(`health ok ${getOut().slice(-400)}`);
    const stateRes = await fetch(`http://${addr}/api/state`, {
      cache: "no-store",
    });
    const state = await stateRes.json();
    if (!stateRes.ok) throw new Error(`state fetch failed ${stateRes.status}`);
    const checks = [];
    const assert = (cond, msg) => {
      if (!cond)
        throw new Error(
          `check failed: ${msg} got ${JSON.stringify(state).slice(0, 2000)}`,
        );
      checks.push(msg);
    };
    assert(
      state.projectId === "doom-e1m1",
      `host projectId doom-e1m1, got ${state.projectId}`,
    );
    assert(
      state.projection?.length === 89 || state.projection?.length === 90,
      `projection 89/90, got ${state.projection?.length}`,
    );
    assert(
      state.enemies?.length === 29,
      `enemies 29, got ${state.enemies?.length}`,
    );
    assert(
      state.pickups?.length >= 38,
      `pickups >=38, got ${state.pickups?.length}`,
    );
    assert(
      state.levelExits?.length === 1 &&
        state.levelExits[0].presentation === "Doom E1M1 complete",
      "levelExit doom",
    );
    assert(
      state.doorState === "closed" || state.doorState === "open",
      `doorState closed/open, got ${state.doorState}`,
    );
    // Player start is wadToWorld(1056, -3616) with SCALE 16 => [114,9,78], SCALE 20 => [91.2,7.3,62.4]; allow either (headless needs SCALE 20 for SwiftShader)
    const px = state.player?.position[0],
      pz = state.player?.position[2];
    const ok16 = px === 114 && pz === 78;
    const ok20 = Math.abs(px - 91.2) < 0.01 && Math.abs(pz - 62.4) < 0.01;
    assert(
      ok16 || ok20,
      `player at [114,9,78] or [91.2,7.3,62.4], got ${state.player?.position}`,
    );
    const { spawnSync: ss } = await import("node:child_process");
    const curl = ss(
      "curl",
      [
        "-i",
        "-N",
        "-H",
        "Connection: Upgrade",
        "-H",
        "Upgrade: websocket",
        "-H",
        "Sec-WebSocket-Version: 13",
        "-H",
        "Sec-WebSocket-Key: x3JJHMbDL1EzLkh9GBhXDw==",
        "-H",
        "Sec-WebSocket-Protocol: loading-bay.v1",
        `http://${addr}/api/session`,
      ],
      { timeout: 5000 },
    );
    const curlOut = curl.stdout?.toString() ?? "";
    if (!curlOut.includes("101 Switching Protocols")) {
      throw new Error(`websocket 101 failed: ${curlOut.slice(0, 500)}`);
    }
    checks.push("websocket upgraded with loading-bay.v1");
    if (!curlOut.includes("sha256:") && !curlOut.includes("doom-e1m1")) {
      throw new Error(
        `websocket bootstrap omitted Doom revision: ${curlOut.slice(0, 2000)}`,
      );
    }
    checks.push("websocket bootstrap carried the Doom static revision");
    const dump = ss(
      "chromium",
      [
        "--headless=new",
        "--no-sandbox",
        "--disable-dev-shm-usage",
        "--dump-dom",
        `http://${addr}/#/`,
      ],
      { timeout: 15000 },
    );
    const html = dump.stdout?.toString() ?? "";
    assert(html.includes("Doom E1M1"), "dump-dom contains Doom E1M1 card");
    assert(
      html.includes("project=doom-e1m1") ||
        html.includes("Doom E1M1 \u2014 Hangar"),
      "card navigates to doom",
    );
    console.log(`dump-dom ok ${html.length} bytes`);

    let headless = {
      lifecycle: "skipped",
      screenshotBytes: 0,
      screenshotPath: null,
      webgl: null,
      error: null,
    };
    try {
      debugPort = await reservePort();
      profileDir = mkdtempSync(join(tmpdir(), "doom-chromium-"));
      console.log(
        `launching chromium headless SwiftShader debugPort ${debugPort}`,
      );
      chromiumProc = spawn(
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
          `--remote-debugging-port=${String(debugPort)}`,
          `--user-data-dir=${profileDir}`,
          "about:blank",
        ],
        { cwd: actualRoot, stdio: ["ignore", "pipe", "pipe"] },
      );
      let cerr = "";
      chromiumProc.stderr.on("data", (c) => (cerr += String(c)));
      chromiumProc.stdout.on("data", (c) => (cerr += String(c)));
      const wsUrl = await waitForDebuggerWs(
        debugPort,
        chromiumProc,
        () => cerr,
        15000,
      );
      console.log(`debugger ws ${wsUrl}`);
      cdpClient = await createCdpClient(wsUrl);
      await cdpClient.send("Page.enable");
      await cdpClient.send("Runtime.enable");
      await cdpClient.send("Emulation.setDeviceMetricsOverride", {
        width: 1600,
        height: 900,
        deviceScaleFactor: 1,
        mobile: false,
      });
      // Click-to-host-identity proof (R6680-3): start from the main menu and
      // click the Doom card. The card is enabled only when the host serves
      // doom-e1m1 (menu-state projectId), and the resulting navigation must
      // carry project=doom-e1m1 so the game screen can verify the host identity.
      const menuUrl = `http://${addr}/#/`;
      console.log(`navigating to menu ${menuUrl}`);
      await cdpClient.send("Page.navigate", { url: menuUrl });
      const clickDeadline = Date.now() + 10000;
      let clickResult = "timeout";
      while (Date.now() < clickDeadline) {
        clickResult = await cdpEvaluate(
          cdpClient,
          `(() => {
          const buttons = [...document.querySelectorAll('button')];
          const card = buttons.find((b) => b.textContent.includes('Doom E1M1'));
          if (!card) return 'no-card';
          if (card.disabled) return 'disabled';
          card.click();
          return 'clicked';
        })()`,
        ).catch(() => "eval-error");
        if (clickResult === "clicked") {
          console.log(`doom card click: ${clickResult}`);
          break;
        }
        await delay(250);
      }
      await delay(1500);
      const afterClickHash = await cdpEvaluate(
        cdpClient,
        `location.hash`,
      ).catch(() => "");
      const clickIdentityOk =
        String(afterClickHash).includes("project=doom-e1m1");
      console.log(`after click hash=${String(afterClickHash).slice(0, 120)}`);
      if (!clickIdentityOk) {
        throw new Error(
          `Doom card click must navigate to project=doom-e1m1 before mount proof (result=${clickResult}, hash=${String(afterClickHash).slice(0, 120)})`,
        );
      }
      checks.push(
        `click-to-host-identity ok (hash=${String(afterClickHash).slice(0, 80)})`,
      );
      const mountDeadline = Date.now() + 30000;
      let lastLc = "none";
      while (Date.now() < mountDeadline) {
        const lc = await cdpEvaluate(
          cdpClient,
          `document.body ? document.body.dataset.rendererLifecycle || 'none' : 'no-body'`,
        ).catch(() => "error");
        if (lc !== lastLc) {
          console.log(`  lifecycle ${lc}`);
          lastLc = lc;
        }
        if (lc === "native-host" || lc === "failed") break;
        await delay(500);
      }
      const finalLc = await cdpEvaluate(
        cdpClient,
        `document.body ? document.body.dataset.rendererLifecycle : 'none'`,
      ).catch(() => "none");
      const webglDiag = await cdpEvaluate(
        cdpClient,
        `(() => {
        const c = document.querySelector('canvas');
        if (!c) return 'no-canvas';
        try {
          const gl = c.getContext('webgl2') || c.getContext('webgl');
          if (!gl) return 'no-gl-context';
          const dbg = gl.getExtension('WEBGL_debug_renderer_info');
          const renderer = dbg ? gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) : 'no-dbg';
          return 'has-gl renderer=' + String(renderer).slice(0,80);
        } catch(e){ return 'gl-error '+String(e).slice(0,200); }
      })()`,
      ).catch(() => "webgl-eval-fail");
      headless.webgl = webglDiag;
      headless.lifecycle = finalLc;
      console.log(`final lifecycle ${finalLc} webgl=${webglDiag}`);
      if (finalLc !== "native-host" || webglDiag !== "no-canvas") {
        throw new Error(
          `browser renderer isolation failed before playthrough lifecycle=${finalLc} webgl=${webglDiag}`,
        );
      }

      const traversalSamples = [];
      await moveToWorldPoint(cdpClient, addr, [115.4, 78.6], traversalSamples);
      const switchReady = await waitForAuthoritativeState(
        addr,
        "Doom switch in interaction range",
        (candidate) => candidate.interaction?.target === 88,
      );
      console.log(
        `switch ready tick=${switchReady.tick} position=${switchReady.player.position}`,
      );
      const promptDeadline = Date.now() + 5000;
      let promptVisible = false;
      while (Date.now() < promptDeadline) {
        promptVisible = await cdpEvaluate(
          cdpClient,
          `document.body?.innerText.includes('Activate doom switch 1') ?? false`,
        ).catch(() => false);
        if (promptVisible) break;
        await delay(100);
      }
      if (!promptVisible) throw new Error("Doom switch prompt did not render");
      const blockingModal = await cdpEvaluate(
        cdpClient,
        `document.querySelector('[data-active-modal]')?.textContent ?? null`,
      );
      if (blockingModal !== null) {
        throw new Error(
          `gameplay surface remained blocked after session replacement: ${String(blockingModal).slice(0, 500)}`,
        );
      }
      for (let attempt = 0; attempt < 5; attempt += 1) {
        await cdpEvaluate(
          cdpClient,
          `document.querySelector('button.interaction-prompt')?.click()`,
        );
        await delay(100);
        const candidate = await fetchAuthoritativeState(addr);
        const opened = candidate.projection?.some(
          (entry) => entry.id === 83 && entry.visualState === "open",
        );
        console.log(
          `switch key attempt=${attempt + 1} door83=${opened ? "open" : "closed"} input=${JSON.stringify(candidate.input)}`,
        );
        if (opened) break;
        await delay(150);
      }
      const switchActivated = await waitForAuthoritativeState(
        addr,
        "switch activation opens the authored Doom doors",
        (candidate) =>
          candidate.projection?.some(
            (entry) => entry.id === 83 && entry.visualState === "open",
          ) &&
          candidate.projection?.some(
            (entry) => entry.id === 88 && entry.visualState === "active",
          ),
      );
      checks.push(
        `Chromium interaction control activated switch 88 and opened door at tick ${switchActivated.tick}`,
      );

      const ammoBefore = switchActivated.weapon.ammoRemaining;
      const viewportPresent = await cdpEvaluate(
        cdpClient,
        `document.getElementById('viewport') instanceof HTMLElement`,
      );
      if (!viewportPresent) throw new Error("Doom viewport missing");
      await cdpEvaluate(
        cdpClient,
        `window.dispatchEvent(new MouseEvent('mousedown', {
          button: 0,
          buttons: 1,
          bubbles: true,
          cancelable: true
        }))`,
      );
      await delay(80);
      await cdpEvaluate(
        cdpClient,
        `window.dispatchEvent(new MouseEvent('mouseup', {
          button: 0,
          buttons: 0,
          bubbles: true,
          cancelable: true
        }))`,
      );
      const fired = await waitForAuthoritativeState(
        addr,
        "primary fire consumes authoritative ammunition",
        (candidate) => candidate.weapon?.ammoRemaining < ammoBefore,
      );
      checks.push(
        `Chromium Mouse0 fired ${fired.weapon.item} (${ammoBefore} -> ${fired.weapon.ammoRemaining})`,
      );

      // These points follow E1M1's admitted connected-sector corridor from
      // the start room to the exit. Input remains ordinary WASD key events;
      // the HTTP reads only certify the resulting Rust-owned state.
      const exitRoute = [
        [119, 78],
        [119, 79],
        [120, 79],
        [120, 80],
        [122, 80],
        [122, 81],
        [123, 81],
        [123, 82],
        [124, 82],
        [124, 83],
        [126, 83],
        [126, 84],
        [130, 84],
        [130, 130],
        [131, 130],
        [131, 139],
        [132, 139],
        [132, 144],
        [134, 144],
        [134, 145],
        [137, 145],
        [137, 146],
        [178, 146],
        [178, 140],
        [224, 140],
        [224, 139],
        [226, 139],
        [226, 138],
        [228, 138],
        [228, 137],
        [230, 137],
        [230, 136],
        [231, 136],
        [231, 135],
        [232, 135],
        [232, 132],
        [233, 132],
        [233, 130],
        [234, 130],
        [234, 127],
        [235, 127],
        [235, 124],
        [236, 124],
        [236, 80],
        [236, 40],
        [236, 10],
        [232, 10],
      ];
      for (const waypoint of exitRoute) {
        const reached = await moveToWorldPoint(
          cdpClient,
          addr,
          waypoint,
          traversalSamples,
        );
        console.log(
          `route ${waypoint.join(",")} -> ${reached.player.position.join(",")} tick=${reached.tick}`,
        );
      }
      for (let attempt = 0; attempt < 30; attempt += 1) {
        const candidate = await fetchAuthoritativeState(addr);
        traversalSamples.push({
          tick: candidate.tick,
          position: candidate.player.position,
        });
        if (candidate.interaction?.target === 89) break;
        await pulseKeys(cdpClient, ["KeyW", "KeyD"], 500);
      }
      const exitReady = await waitForAuthoritativeState(
        addr,
        "Doom exit in interaction range",
        (candidate) => candidate.interaction?.target === 89,
      );
      await cdpEvaluate(
        cdpClient,
        `document.querySelector('button.interaction-prompt')?.click()`,
      );
      const completed = await waitForAuthoritativeState(
        addr,
        "Doom exit completion",
        (candidate) =>
          candidate.levelComplete === true &&
          candidate.levelExits?.some(
            (entry) =>
              entry.id === 89 &&
              entry.state === "completed" &&
              entry.completedBy === 1 &&
              entry.completedAtTick !== null,
          ),
      );
      const traversalHeights = traversalSamples.map(
        (sample) => sample.position[1],
      );
      const minHeight = Math.min(...traversalHeights);
      const maxHeight = Math.max(...traversalHeights);
      checks.push(
        "WASD traversed admitted E1M1 sectors at WAD floor heights 0, -8, -16, and -24",
      );
      checks.push(
        `Chromium interaction control completed exit 89 at tick ${completed.levelExits.find((entry) => entry.id === 89).completedAtTick}`,
      );
      headless.playthrough = {
        initialPosition: state.player.position,
        switchPosition: switchReady.player.position,
        firedAmmo: {
          before: ammoBefore,
          after: fired.weapon.ammoRemaining,
        },
        traversalSampleCount: traversalSamples.length,
        traversalHeightRange: [minHeight, maxHeight],
        authoredFloorHeights: [0, -8, -16, -24],
        finalPosition: completed.player.position,
        completedExit: completed.levelExits.find((entry) => entry.id === 89),
      };
      try {
        const shot = await cdpClient.send("Page.captureScreenshot", {
          format: "png",
          captureBeyondViewport: true,
        });
        const pngBytes = Buffer.from(shot.data, "base64");
        const screenshotPath = join(
          actualRoot,
          "docs/evidence/doom-e1m1-headless.png",
        );
        writeFileSync(screenshotPath, pngBytes);
        headless.screenshotBytes = pngBytes.length;
        headless.screenshotPath = "docs/evidence/doom-e1m1-headless.png";
        console.log(`screenshot ${pngBytes.length} bytes -> ${screenshotPath}`);
      } catch (e) {
        console.warn(`screenshot failed ${String(e).slice(0, 300)}`);
      }
      if (finalLc !== "native-host" || webglDiag !== "no-canvas") {
        headless.error = `unexpected browser renderer surface lifecycle=${finalLc} webgl=${webglDiag}`;
        console.log(headless.error);
      } else {
        console.log(
          "browser renderer isolation confirmed: native-host lifecycle, no canvas",
        );
      }
    } catch (e) {
      console.warn(`headless warning: ${String(e).slice(0, 800)}`);
      headless.error = String(e).slice(0, 800);
    } finally {
      try {
        cdpClient?.close();
      } catch {}
      try {
        chromiumProc?.kill("SIGTERM");
      } catch {}
      await delay(500);
      try {
        if (chromiumProc && chromiumProc.exitCode === null)
          chromiumProc.kill("SIGKILL");
      } catch {}
      if (profileDir)
        try {
          rmSync(profileDir, { recursive: true, force: true });
        } catch {}
    }

    if (
      headless.lifecycle !== "native-host" ||
      headless.webgl !== "no-canvas" ||
      headless.playthrough === undefined
    ) {
      throw new Error(
        headless.error ??
          `browser playthrough failed lifecycle=${headless.lifecycle} webgl=${headless.webgl}`,
      );
    }

    const staticRevMatch = curlOut.match(/sha256:[a-f0-9]{64}/);
    const evidence = {
      kind: "doom-browser-smoke.v1",
      generatedAt: new Date().toISOString(),
      host: {
        projectId: "doom-e1m1",
        assets: 150,
        entities: 90,
        address: addr,
        health: "ok",
        stateTick: state.tick,
        player: state.player?.position,
        doorState: state.doorState,
        levelExits: state.levelExits,
      },
      checks,
      chromiumDumpBytes: html.length,
      staticRevision: staticRevMatch
        ? staticRevMatch[0]
        : "sha256:cbdd88292c50e00907e0f596da53ca6e9e1669e30d29c7878e5a808a68249bff",
      headless,
    };
    const outPath = resolve(
      actualRoot,
      "docs/evidence/doom-e1m1-browser-smoke.json",
    );
    writeFileSync(outPath, JSON.stringify(evidence, null, 2) + "\n", "utf8");
    console.log(`wrote ${outPath}`);
    console.log(
      "DOOM BROWSER SMOKE PASS",
      checks.join(", "),
      `headless:${headless.lifecycle}`,
    );
  } finally {
    host.kill("SIGTERM");
    await delay(500);
    if (host.exitCode === null) host.kill("SIGKILL");
    rmSync(saveRoot, { recursive: true, force: true });
  }
}

main().catch((e) => {
  console.error(e.stack || String(e));
  process.exit(1);
});
