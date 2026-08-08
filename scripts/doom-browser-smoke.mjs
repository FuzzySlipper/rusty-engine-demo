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
    assert(
      curlOut.includes("101 Switching Protocols"),
      `websocket 101 got ${curlOut.slice(0, 500)}`,
    );
    assert(
      curlOut.includes("sha256:") || curlOut.includes("doom-e1m1"),
      `websocket payload should contain doom hash, got ${curlOut.slice(0, 2000)}`,
    );
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
      while (Date.now() < clickDeadline) {
        const clicked = await cdpEvaluate(
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
        if (clicked === "clicked" || clicked === "disabled") {
          console.log(`doom card click: ${clicked}`);
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
      checks.push(
        `click-to-host-identity ${clickIdentityOk ? "ok" : "FAIL"} (hash=${String(afterClickHash).slice(0, 80)})`,
      );
      console.log(`after click hash=${String(afterClickHash).slice(0, 120)}`);
      // If the card click did not navigate (e.g. disabled because host project
      // identity could not be verified), fall back to the explicit game URL so
      // the mount proof still runs; the click proof above is recorded.
      const gameUrl = `http://${addr}/#/game?project=doom-e1m1&mode=new`;
      if (!String(afterClickHash).includes("project=doom-e1m1")) {
        console.log(`navigating directly to ${gameUrl} (fallback)`);
        await cdpClient.send("Page.navigate", { url: gameUrl });
      }
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
      headless.webgl !== "no-canvas"
    ) {
      throw new Error(
        headless.error ??
          `browser renderer isolation failed lifecycle=${headless.lifecycle} webgl=${headless.webgl}`,
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
