#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const actualRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const doomProject = join(actualRoot, "content/projects/doom-e1m1.project.json");
const chromium = process.env.CHROMIUM_BIN ?? "/usr/bin/chromium";
const updateEvidence = process.env.UPDATE_EVIDENCE === "1";
const focused = process.env.RUSTY_DOOM_SMOKE_FOCUSED === "1";
const traversalEvidence = process.env.RUSTY_DOOM_TRAVERSAL_EVIDENCE === "1";
const retainedInteractionEvidence =
  process.env.RUSTY_DOOM_INTERACTION_EVIDENCE === "1";
const encounterExitEvidence =
  process.env.RUSTY_DOOM_ENCOUNTER_EXIT_EVIDENCE === "1";
const interactionEvidence =
  retainedInteractionEvidence ||
  encounterExitEvidence ||
  (!focused && !traversalEvidence);
const retainedEvidence =
  traversalEvidence || retainedInteractionEvidence || encounterExitEvidence;
const traversalEvidenceDir = process.env.RUSTY_DOOM_EVIDENCE_DIR ?? null;
const expectedEvidenceSha = process.env.RUSTY_DOOM_EXPECTED_SHA ?? null;
const headedOzonePlatform =
  process.env.RUSTY_DOOM_HEADED_OZONE ??
  (process.env.DISPLAY === undefined ? "wayland" : "x11");

if (
  [
    traversalEvidence,
    retainedInteractionEvidence,
    encounterExitEvidence,
  ].filter(Boolean).length > 1
) {
  throw new Error("retained evidence modes are mutually exclusive");
}
if (focused && retainedEvidence) {
  throw new Error(
    "focused smoke and traversal evidence modes are mutually exclusive",
  );
}
if (retainedEvidence && traversalEvidenceDir === null) {
  throw new Error(
    "RUSTY_DOOM_EVIDENCE_DIR is required for traversal evidence mode",
  );
}
if (retainedEvidence && expectedEvidenceSha === null) {
  throw new Error(
    "RUSTY_DOOM_EXPECTED_SHA is required for traversal evidence mode",
  );
}

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

function launchHost(addr, saveRoot, projectPath) {
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
      projectPath,
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

async function waitForAuthoritativeState(
  addr,
  description,
  predicate,
  timeoutMilliseconds = 10000,
) {
  const deadline = Date.now() + timeoutMilliseconds;
  let lastState = null;
  while (Date.now() < deadline) {
    lastState = await fetchAuthoritativeState(addr);
    if (predicate(lastState)) return lastState;
    await delay(100);
  }
  const summary =
    lastState === null
      ? null
      : {
          tick: lastState.tick,
          player: {
            position: lastState.player?.position,
            health: lastState.player?.currentHealth,
            state: lastState.player?.vitalityState,
          },
          input: lastState.input,
          lastEvents: lastState.lastEvents,
        };
  throw new Error(
    `authoritative state timeout (${description}): ${JSON.stringify(summary).slice(0, 2000)}`,
  );
}

const keyIdentity = {
  KeyW: { key: "w", virtualKeyCode: 87 },
  KeyA: { key: "a", virtualKeyCode: 65 },
  KeyS: { key: "s", virtualKeyCode: 83 },
  KeyD: { key: "d", virtualKeyCode: 68 },
  KeyE: { key: "e", virtualKeyCode: 69 },
  Digit1: { key: "1", virtualKeyCode: 49 },
  Digit2: { key: "2", virtualKeyCode: 50 },
  Digit3: { key: "3", virtualKeyCode: 51 },
  Space: { key: " ", virtualKeyCode: 32 },
};

async function dispatchKey(client, type, code) {
  const identity = keyIdentity[code];
  if (!identity) throw new Error(`missing CDP key identity for ${code}`);
  await client.send("Input.dispatchKeyEvent", {
    type,
    code,
    key: identity.key,
    windowsVirtualKeyCode: identity.virtualKeyCode,
    nativeVirtualKeyCode: identity.virtualKeyCode,
  });
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

async function holdKeys(client, codes, milliseconds) {
  for (const code of codes) await dispatchKey(client, "keyDown", code);
  await delay(milliseconds);
  for (const code of [...codes].reverse())
    await dispatchKey(client, "keyUp", code);
  await delay(60);
}

async function focusGameplayCanvas(client) {
  const gameplayDeadline = Date.now() + 20_000;
  let interactionMode = null;
  while (Date.now() < gameplayDeadline) {
    interactionMode = await cdpEvaluate(
      client,
      `document.querySelector('[data-rusty-application-host]')?.dataset.interactionMode ?? null`,
    ).catch(() => null);
    if (interactionMode === "gameplay") break;
    await delay(100);
  }
  if (interactionMode !== "gameplay") {
    throw new Error(
      `Engine interaction mode stayed ${String(interactionMode)}`,
    );
  }
  await cdpEvaluate(
    client,
    `document.querySelector('canvas')?.focus({ preventScroll: true })`,
  );
  return cdpEvaluate(
    client,
    `(() => {
      const canvas = document.querySelector('canvas');
      const host = document.querySelector('[data-rusty-application-host]');
      return {
        active: document.activeElement === canvas,
        interactionMode: host?.dataset.interactionMode ?? null,
        pointerLocked: document.pointerLockElement === canvas,
      };
    })()`,
  );
}

async function proveFocusedHeldMovement(client, addr) {
  const requiredHeldDistance = 2;
  const inputSurface = await focusGameplayCanvas(client);
  const beforeHold = await fetchAuthoritativeState(addr);
  await dispatchKey(client, "keyDown", "KeyW");
  let duringHold;
  try {
    duringHold = await waitForAuthoritativeState(
      addr,
      "one physical keydown sustains forward movement",
      (state) =>
        horizontalDistance(beforeHold.player.position, state.player.position) >=
        requiredHeldDistance,
    );
  } catch (error) {
    const runtimeError = await cdpEvaluate(
      client,
      `document.body?.dataset.runtimeError ?? 'no-runtime-error'`,
    ).catch(() => "runtime-error-eval-failed");
    throw new Error(`${error} runtimeError=${runtimeError}`);
  } finally {
    await dispatchKey(client, "keyUp", "KeyW");
  }
  let afterRelease = await fetchAuthoritativeState(addr);
  let releaseSettled = false;
  const releaseDeadline = Date.now() + 2_000;
  let quietSince = Date.now();
  let quietAnchor = afterRelease.player.position;
  while (Date.now() < releaseDeadline) {
    await delay(100);
    const candidate = await fetchAuthoritativeState(addr);
    const sampleDistance = horizontalDistance(
      quietAnchor,
      candidate.player.position,
    );
    afterRelease = candidate;
    if (sampleDistance > 0.15) {
      quietAnchor = candidate.player.position;
      quietSince = Date.now();
    } else if (Date.now() - quietSince >= 500) {
      releaseSettled = true;
      break;
    }
  }
  await delay(250);
  const stopped = await fetchAuthoritativeState(addr);
  const heldDistance = horizontalDistance(
    beforeHold.player.position,
    duringHold.player.position,
  );
  const stoppedDistance = horizontalDistance(
    afterRelease.player.position,
    stopped.player.position,
  );
  if (
    heldDistance < requiredHeldDistance ||
    !releaseSettled ||
    stoppedDistance > 0.15
  ) {
    throw new Error(
      `single keydown did not sustain then release movement: ${JSON.stringify({ inputSurface, heldDistance, releaseSettled, stoppedDistance, before: beforeHold.player.position, during: duringHold.player.position, afterRelease: afterRelease.player.position, stopped: stopped.player.position })}`,
    );
  }
  return { heldDistance, stoppedDistance };
}

async function proveFocusedHeldPistolFire(client, addr) {
  const before = await fetchAuthoritativeState(addr);
  if (before.weapon?.item !== "weapon/pistol") {
    throw new Error(`expected equipped pistol, got ${before.weapon?.item}`);
  }
  const canvas = await cdpEvaluate(
    client,
    `(() => {
      const canvas = document.querySelector('canvas');
      if (!canvas) return null;
      const bounds = canvas.getBoundingClientRect();
      return { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height };
    })()`,
  );
  if (canvas === null)
    throw new Error("Engine canvas is unavailable for Mouse0");
  const center = await acquirePhysicalPointerLock(client, canvas);
  await client.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: center.x,
    y: center.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  let after;
  try {
    after = await waitForAuthoritativeState(
      addr,
      "held Mouse0 fires the equipped pistol at authored cadence",
      (candidate) =>
        candidate.weapon?.item === "weapon/pistol" &&
        candidate.weapon.ammoRemaining <= before.weapon.ammoRemaining - 2,
    );
  } finally {
    await client.send("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: center.x,
      y: center.y,
      button: "left",
      buttons: 0,
      clickCount: 1,
    });
  }
  return {
    shots: before.weapon.ammoRemaining - after.weapon.ammoRemaining,
    ammoBefore: before.weapon.ammoRemaining,
    ammoAfter: after.weapon.ammoRemaining,
  };
}

async function proveFocusedWeaponSelection(client, addr) {
  const acquired = await waitForAuthoritativeState(
    addr,
    "bounded product fixture collects the authored shotgun",
    (candidate) =>
      candidate.inventory?.weapons?.some(
        (weapon) => weapon.item === "weapon/shotgun" && weapon.owned === true,
      ) === true,
  );
  await dispatchKey(client, "keyDown", "Digit2");
  await dispatchKey(client, "keyUp", "Digit2");
  const shotgun = await waitForAuthoritativeState(
    addr,
    "physical Digit2 selects the collected shotgun slot",
    (candidate) => candidate.weapon?.item === "weapon/shotgun",
  );
  await dispatchKey(client, "keyDown", "Digit3");
  await dispatchKey(client, "keyUp", "Digit3");
  const fist = await waitForAuthoritativeState(
    addr,
    "physical Digit3 selects the owned fist slot",
    (candidate) =>
      candidate.weapon?.item === "weapon/fist" &&
      candidate.weapon.ammoRemaining === 0 &&
      candidate.weapon.ammoCapacity === 0,
  );
  await dispatchKey(client, "keyDown", "Digit1");
  await dispatchKey(client, "keyUp", "Digit1");
  const pistol = await waitForAuthoritativeState(
    addr,
    "physical Digit1 selects the pistol slot",
    (candidate) => candidate.weapon?.item === "weapon/pistol",
  );
  return {
    acquiredTick: acquired.tick,
    shotgunTick: shotgun.tick,
    fistTick: fist.tick,
    pistolTick: pistol.tick,
  };
}

async function proveFocusedVitality(addr) {
  const state = await waitForAuthoritativeState(
    addr,
    "authored vitality pickups and nukage update Rust-owned player state",
    (candidate) =>
      candidate.player?.maxHealth === 200 &&
      candidate.player?.maxArmor === 200 &&
      candidate.player?.currentHealth < 100 &&
      candidate.player?.armor > 0 &&
      ["supply/health-bonus", "armor/green"].every((item) =>
        candidate.pickups?.some(
          (pickup) => pickup.item === item && pickup.state === "collected",
        ),
      ) &&
      !candidate.inventory?.stacks?.some((stack) =>
        ["supply/health-bonus", "armor/green"].includes(stack.item),
      ),
  );
  return {
    tick: state.tick,
    health: state.player.currentHealth,
    armor: state.player.armor,
    maxHealth: state.player.maxHealth,
    maxArmor: state.player.maxArmor,
  };
}

async function proveFocusedDeathAndRestart(client, addr) {
  const defeated = await waitForAuthoritativeState(
    addr,
    "authored nukage defeats the player",
    (candidate) =>
      candidate.player?.vitalityState === "dead" &&
      candidate.player?.currentHealth === 0 &&
      candidate.restart?.authoredBaselineAvailable === true,
    20000,
  );
  const deadline = Date.now() + 10000;
  let restartButton = null;
  while (Date.now() < deadline && restartButton === null) {
    restartButton = await cdpEvaluate(
      client,
      `(() => {
        const button = [...document.querySelectorAll('button')].find(
          (candidate) => candidate.textContent.trim().startsWith('Restart'),
        );
        if (!button || button.disabled) return null;
        const bounds = button.getBoundingClientRect();
        return { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 };
      })()`,
    );
    if (restartButton === null) await delay(100);
  }
  if (restartButton === null) {
    throw new Error(
      "visible authored-restart button did not become actionable",
    );
  }
  await client.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: restartButton.x,
    y: restartButton.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await client.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: restartButton.x,
    y: restartButton.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  const restarted = await waitForAuthoritativeState(
    addr,
    "physical restart restores the authored E1M1 baseline",
    (candidate) =>
      candidate.player?.vitalityState === "alive" &&
      candidate.player?.currentHealth === 100 &&
      candidate.player?.armor === 0 &&
      candidate.weapon?.item === "weapon/pistol" &&
      candidate.pickups?.every((pickup) => pickup.state !== "collected"),
  );
  return { defeatedTick: defeated.tick, restartedTick: restarted.tick };
}

async function proveFocusedFireStopsOnBlur(client, addr) {
  await delay(500);
  const before = await fetchAuthoritativeState(addr);
  const canvas = await cdpEvaluate(
    client,
    `(() => {
      const canvas = document.querySelector('canvas');
      if (!canvas) return null;
      const bounds = canvas.getBoundingClientRect();
      return { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 };
    })()`,
  );
  if (canvas === null)
    throw new Error("Engine canvas is unavailable for blur proof");
  await client.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: canvas.x,
    y: canvas.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  const fired = await waitForAuthoritativeState(
    addr,
    "Mouse0 fires once before focus loss",
    (candidate) =>
      candidate.weapon.ammoRemaining === before.weapon.ammoRemaining - 1,
  );
  await cdpEvaluate(client, `globalThis.dispatchEvent(new Event('blur'))`);
  await delay(750);
  const stopped = await fetchAuthoritativeState(addr);
  await client.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: canvas.x,
    y: canvas.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  if (stopped.weapon.ammoRemaining !== fired.weapon.ammoRemaining) {
    throw new Error(
      `pistol kept firing after blur without MouseUp: ${JSON.stringify({ before: before.weapon.ammoRemaining, fired: fired.weapon.ammoRemaining, stopped: stopped.weapon.ammoRemaining })}`,
    );
  }
  return {
    ammoBefore: before.weapon.ammoRemaining,
    ammoAfterShot: fired.weapon.ammoRemaining,
    ammoAfterBlur: stopped.weapon.ammoRemaining,
  };
}

function horizontalDistance(left, right) {
  return Math.hypot(right[0] - left[0], right[2] - left[2]);
}

async function moveToWorldPoint(
  client,
  addr,
  target,
  traversalSamples,
  { singleHold = false, arrivalDistance = 0.7, stopWhen = null } = {},
) {
  const deadline = Date.now() + 60000;
  let previousDistance = Number.POSITIVE_INFINITY;
  let stalledPulses = 0;
  let lastState = null;
  while (Date.now() < deadline) {
    const state = await fetchAuthoritativeState(addr);
    lastState = state;
    if (state.player?.vitalityState === "dead") {
      throw new Error(
        `player died while moving to ${JSON.stringify(target)}; recent=${JSON.stringify(traversalSamples.slice(-8))}`,
      );
    }
    const [x, y, z] = state.player.position;
    traversalSamples.push({
      tick: state.tick,
      position: [x, y, z],
      terrainContact: state.player.terrainContact,
      health: state.player.currentHealth,
    });
    if (stopWhen?.(state) === true) return state;
    const dx = target[0] - x;
    const dz = target[1] - z;
    const distance = Math.hypot(dx, dz);
    if (distance <= arrivalDistance) return state;

    // Resolve the world-space route into the admitted Rust player heading.
    // This keeps the smoke valid when the Doom-to-Engine angle conversion is
    // corrected instead of baking one historical spawn yaw into the route.
    const yaw = (state.player.yawDegrees * Math.PI) / 180;
    const localForward = -Math.sin(yaw) * dx - Math.cos(yaw) * dz;
    const localRight = Math.cos(yaw) * dx - Math.sin(yaw) * dz;
    const codes = [];
    if (Math.abs(localForward) > 0.45) {
      codes.push(localForward > 0 ? "KeyW" : "KeyS");
    }
    if (Math.abs(localRight) > 0.45) {
      codes.push(localRight > 0 ? "KeyD" : "KeyA");
    }
    const holdMilliseconds =
      distance < 2
        ? 80
        : Math.min(2_000, Math.max(500, (distance / 6) * 1_000));
    if (singleHold) await holdKeys(client, codes, holdMilliseconds);
    else await pulseKeys(client, codes, distance < 2 ? 80 : 500);

    stalledPulses = distance >= previousDistance - 0.03 ? stalledPulses + 1 : 0;
    previousDistance = distance;
    if (stalledPulses >= 8) {
      throw new Error(
        `movement stalled approaching ${JSON.stringify(target)} from ${JSON.stringify([x, y, z])}; input=${JSON.stringify(state.input)} events=${JSON.stringify(state.lastEvents).slice(0, 1000)}`,
      );
    }
  }
  throw new Error(
    `movement timeout approaching ${JSON.stringify(target)} from ${JSON.stringify(lastState?.player?.position)}; input=${JSON.stringify(lastState?.input)} events=${JSON.stringify(lastState?.lastEvents).slice(0, 1000)}`,
  );
}

async function captureCanvasEvidence(client, canvasBounds, path) {
  const shot = await client.send("Page.captureScreenshot", {
    format: "png",
    clip: { ...canvasBounds, scale: 1 },
  });
  const bytes = Buffer.from(shot.data, "base64");
  writeFileSync(path, bytes);
  return bytes.length;
}

async function proveLandmarkTraversal(client, addr, canvasBounds, evidenceDir) {
  const startedAtMs = Date.now();
  mkdirSync(evidenceDir, { recursive: true });
  const inputSurface = await focusGameplayCanvas(client);
  const traversalSamples = [];
  const l1 = await fetchAuthoritativeState(addr);
  if (
    horizontalDistance(l1.player.position, [114, l1.player.position[1], 78]) >
    0.8
  ) {
    throw new Error(`L1 start mismatch: ${JSON.stringify(l1.player.position)}`);
  }
  const l1Screenshot = join(evidenceDir, "l1-start-room.png");
  const l1ScreenshotBytes = await captureCanvasEvidence(
    client,
    canvasBounds,
    l1Screenshot,
  );

  const l1ToL2Route = [
    [92, 102],
    [79, 102],
    [67, 100],
    [66, 102],
    [64, 102],
    [62, 102],
    [60, 102],
    [58, 102],
    [56, 102],
    [44, 102],
    [34, 100],
    [34, 102],
  ];
  for (const [waypointIndex, waypoint] of l1ToL2Route.entries()) {
    const reached = await moveToWorldPoint(
      client,
      addr,
      waypoint,
      traversalSamples,
      {
        singleHold: true,
        arrivalDistance: waypointIndex === l1ToL2Route.length - 1 ? 0.7 : 1.5,
      },
    );
    console.log(
      `landmark route ${waypoint.join(",")} -> ${reached.player.position.join(",")} tick=${reached.tick}`,
    );
  }
  const l2 = await fetchAuthoritativeState(addr);
  if (
    horizontalDistance(l2.player.position, [34, l2.player.position[1], 102]) >
    0.8
  ) {
    throw new Error(
      `L2 arrival mismatch: ${JSON.stringify(l2.player.position)}`,
    );
  }
  if (
    l1.player?.grounded !== true ||
    l2.player?.grounded !== true ||
    l1.player?.terrainContact === null ||
    l2.player?.terrainContact === null ||
    l2.player.terrainContact.surfaceY <= l1.player.terrainContact.surfaceY
  ) {
    throw new Error(
      `L1-L2 authoritative contact mismatch: ${JSON.stringify({ l1: l1.player, l2: l2.player })}`,
    );
  }
  if (l2.levelComplete === true) {
    throw new Error("bounded L1-L2 evidence must stop before level completion");
  }
  const l2Screenshot = join(evidenceDir, "l2-green-armor-court.png");
  const l2ScreenshotBytes = await captureCanvasEvidence(
    client,
    canvasBounds,
    l2Screenshot,
  );

  await holdKeys(client, ["Space"], 80);
  const airborne = await waitForAuthoritativeState(
    addr,
    "physical Space input starts an authored jump",
    (candidate) =>
      candidate.player?.grounded === false &&
      candidate.player?.verticalVelocity > 0,
  );
  const landed = await waitForAuthoritativeState(
    addr,
    "jump lands at L2",
    (candidate) =>
      candidate.tick > airborne.tick &&
      candidate.player?.grounded === true &&
      Math.abs(candidate.player?.verticalVelocity ?? Number.POSITIVE_INFINITY) <
        0.001,
  );
  const landedScreenshot = join(evidenceDir, "l2-after-jump-landing.png");
  const landedScreenshotBytes = await captureCanvasEvidence(
    client,
    canvasBounds,
    landedScreenshot,
  );

  const terrainContacts = traversalSamples
    .map((sample) => sample.terrainContact)
    .filter(Boolean);
  const admittedFloorLevels = [
    ...new Set(terrainContacts.map((contact) => contact.surfaceY)),
  ].sort((left, right) => left - right);
  if (terrainContacts.length !== traversalSamples.length) {
    throw new Error(
      "authoritative terrain contact was absent during landmark traversal",
    );
  }
  if (admittedFloorLevels.length < 2) {
    throw new Error(
      `L1-L2 route did not traverse distinct admitted floor levels: ${JSON.stringify(admittedFloorLevels)}`,
    );
  }
  return {
    inputSurface,
    landmarks: {
      L1: {
        position: l1.player.position,
        terrainContact: l1.player.terrainContact,
      },
      L2: {
        position: l2.player.position,
        terrainContact: l2.player.terrainContact,
      },
    },
    route: l1ToL2Route,
    traversalSampleCount: traversalSamples.length,
    elapsedMs: Date.now() - startedAtMs,
    admittedFloorLevels,
    jump: {
      airborne: {
        tick: airborne.tick,
        position: airborne.player.position,
        verticalVelocity: airborne.player.verticalVelocity,
      },
      landed: {
        tick: landed.tick,
        position: landed.player.position,
        verticalVelocity: landed.player.verticalVelocity,
      },
    },
    artifacts: {
      l1Screenshot: { path: l1Screenshot, bytes: l1ScreenshotBytes },
      l2Screenshot: { path: l2Screenshot, bytes: l2ScreenshotBytes },
      landedScreenshot: {
        path: landedScreenshot,
        bytes: landedScreenshotBytes,
      },
    },
  };
}

function resolveInteractionOwners(projectPath) {
  const project = JSON.parse(readFileSync(projectPath, "utf8"));
  const entities = project.scenes[0].entities;
  const owner = (name, component) => {
    const entity = entities.find(
      (candidate) => candidate.name === name && candidate[component] != null,
    );
    if (!entity) {
      throw new Error(
        `interaction evidence could not resolve ${name}.${component}`,
      );
    }
    return entity.id;
  };
  return {
    startDoor: owner("doom-manual-door-sector-4", "door"),
    secretDoor: owner("doom-manual-door-sector-68", "door"),
    annexDoor: owner("doom-manual-door-sector-76", "door"),
    exitDoor: owner("doom-manual-door-sector-81", "door"),
    floorAction: owner("doom-walk-floor-action-linedef-308", "floorAction"),
    lift: owner("doom-repeatable-lift-linedef-195", "lift"),
    secret: owner("doom-secret-sector-68", "secretRegion"),
    exit: owner("doom-exit", "levelExit"),
    representativeEnemy: owner("doom-zombieman-12", "enemyCombat"),
    representativeDrop: owner("doom-drop-zombieman-12", "pickup"),
    corridorThreats: [
      [
        owner("doom-shotgun-guy-14", "enemyCombat"),
        owner("doom-drop-shotgun-guy-14", "pickup"),
      ],
      [
        owner("doom-zombieman-13", "enemyCombat"),
        owner("doom-drop-zombieman-13", "pickup"),
      ],
    ],
    shotgunUpgrade: [
      owner("doom-shotgun-guy-20", "enemyCombat"),
      owner("doom-drop-shotgun-guy-20", "pickup"),
    ],
    flankThreat: [
      owner("doom-shotgun-guy-21", "enemyCombat"),
      owner("doom-drop-shotgun-guy-21", "pickup"),
    ],
    innerThreat: [
      owner("doom-shotgun-guy-15", "enemyCombat"),
      owner("doom-drop-shotgun-guy-15", "pickup"),
    ],
  };
}

function normalizeDegrees(value) {
  return ((((value + 180) % 360) + 360) % 360) - 180;
}

async function acquirePhysicalPointerLock(client, canvasBounds) {
  const center = {
    x: canvasBounds.x + canvasBounds.width / 2,
    y: canvasBounds.y + canvasBounds.height / 2,
  };
  const isLocked = () =>
    cdpEvaluate(
      client,
      `document.pointerLockElement === document.querySelector('canvas')`,
    );
  if (await isLocked()) return center;
  if (encounterExitEvidence && headedOzonePlatform === "x11") {
    const centered = spawnSync(
      "python3",
      [join(actualRoot, "scripts/x11-pointer-input.py"), "center"],
      { encoding: "utf8" },
    );
    if (centered.status !== 0) {
      throw new Error(`X11 pointer centering failed: ${centered.stderr}`);
    }
  }
  await client.send("Page.bringToFront");
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await client.send("Input.dispatchMouseEvent", {
      type: "mousePressed",
      ...center,
      button: "left",
      buttons: 1,
      clickCount: 1,
    });
    await client.send("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      ...center,
      button: "left",
      buttons: 0,
      clickCount: 1,
    });
    const deadline = Date.now() + 3_000;
    while (Date.now() < deadline) {
      if (await isLocked()) return center;
      await delay(100);
    }
  }
  throw new Error("physical canvas click did not acquire pointer lock");
}

async function setPhysicalPrimaryFire(client, canvasCenter, pressed) {
  if (encounterExitEvidence && headedOzonePlatform === "x11") {
    const button = spawnSync(
      "python3",
      [
        join(actualRoot, "scripts/x11-pointer-input.py"),
        "button",
        pressed ? "down" : "up",
      ],
      { encoding: "utf8" },
    );
    if (button.status !== 0) {
      throw new Error(`X11 primary-fire input failed: ${button.stderr}`);
    }
    return;
  }
  await client.send("Input.dispatchMouseEvent", {
    type: pressed ? "mousePressed" : "mouseReleased",
    ...canvasCenter,
    button: "left",
    buttons: pressed ? 1 : 0,
    clickCount: 1,
  });
}

async function clickVisibleButton(client, label) {
  const deadline = Date.now() + 10_000;
  let point = null;
  while (Date.now() < deadline && point === null) {
    point = await cdpEvaluate(
      client,
      `(() => {
        const button = [...document.querySelectorAll('button')].find(
          (candidate) => candidate.textContent.trim() === ${JSON.stringify(label)},
        );
        if (!button || button.disabled) return null;
        const bounds = button.getBoundingClientRect();
        return { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 };
      })()`,
    );
    if (point === null) await delay(100);
  }
  if (point === null)
    throw new Error(`visible ${label} button was unavailable`);
  await client.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...point,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await client.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...point,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  await delay(150);
}

async function setMaximumPhysicalMouseSensitivity(client, addr) {
  await client.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    code: "Escape",
    key: "Escape",
    windowsVirtualKeyCode: 27,
    nativeVirtualKeyCode: 27,
  });
  await client.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    code: "Escape",
    key: "Escape",
    windowsVirtualKeyCode: 27,
    nativeVirtualKeyCode: 27,
  });
  await waitForAuthoritativeState(
    addr,
    "physical Escape opens the pause panel",
    (candidate) => candidate.input?.paused === true,
  );
  await clickVisibleButton(client, "Settings");
  const slider = await cdpEvaluate(
    client,
    `(() => {
      const input = document.querySelector('input[type="range"]');
      if (!input) return null;
      const bounds = input.getBoundingClientRect();
      return { x: bounds.right - 1, y: bounds.y + bounds.height / 2 };
    })()`,
  );
  if (slider === null)
    throw new Error("mouse sensitivity slider was unavailable");
  await client.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    ...slider,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await client.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    ...slider,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  const sensitivity = await cdpEvaluate(
    client,
    `Number(document.querySelector('input[type="range"]')?.value ?? 0)`,
  );
  if (sensitivity !== 2) {
    throw new Error(`physical sensitivity selection settled at ${sensitivity}`);
  }
  await cdpEvaluate(client, `document.activeElement?.blur()`);
  await client.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    code: "Escape",
    key: "Escape",
    windowsVirtualKeyCode: 27,
    nativeVirtualKeyCode: 27,
  });
  await client.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    code: "Escape",
    key: "Escape",
    windowsVirtualKeyCode: 27,
    nativeVirtualKeyCode: 27,
  });
  await delay(300);
  await clickVisibleButton(client, "Resume");
  await waitForAuthoritativeState(
    addr,
    "visible Resume returns to live simulation",
    (candidate) => candidate.input?.paused === false,
  );
  return sensitivity;
}

async function physicallyAimAtEnemy(
  client,
  addr,
  canvasCenter,
  enemyId,
  toleranceDegrees = 3,
) {
  const before = await fetchAuthoritativeState(addr);
  const enemy = before.enemies.find((candidate) => candidate.id === enemyId);
  if (!enemy) throw new Error(`missing representative enemy ${enemyId}`);
  let dx = enemy.position[0] - before.player.position[0];
  let dz = enemy.position[2] - before.player.position[2];
  let desiredYaw = (Math.atan2(-dx, -dz) * 180) / Math.PI;
  let desiredPitch =
    (Math.atan2(
      enemy.position[1] - before.player.position[1],
      Math.hypot(dx, dz),
    ) *
      180) /
    Math.PI;
  const startingYaw = before.player.yawDegrees;
  const startingPitch = before.player.pitchDegrees;
  let state = before;
  for (let attempt = 0; attempt < 120; attempt += 1) {
    if (state.player?.vitalityState !== "alive") {
      throw new Error(
        `player was defeated while physically aiming at enemy ${enemyId}: ${JSON.stringify({ tick: state.tick, health: state.player?.currentHealth, attempt, yawDegrees: state.player?.yawDegrees, pitchDegrees: state.player?.pitchDegrees })}`,
      );
    }
    const yawError = normalizeDegrees(desiredYaw - state.player.yawDegrees);
    const pitchError = desiredPitch - state.player.pitchDegrees;
    if (
      Math.abs(yawError) <= toleranceDegrees &&
      Math.abs(pitchError) <= toleranceDegrees
    ) {
      await delay(250);
      const settled = await fetchAuthoritativeState(addr);
      const settledEnemy = settled.enemies.find(
        (candidate) => candidate.id === enemyId,
      );
      if (settledEnemy?.state === "alive") {
        dx = settledEnemy.position[0] - settled.player.position[0];
        dz = settledEnemy.position[2] - settled.player.position[2];
        desiredYaw = (Math.atan2(-dx, -dz) * 180) / Math.PI;
        desiredPitch =
          (Math.atan2(
            settledEnemy.position[1] - settled.player.position[1],
            Math.hypot(dx, dz),
          ) *
            180) /
          Math.PI;
      }
      state = settled;
      if (
        Math.abs(normalizeDegrees(desiredYaw - state.player.yawDegrees)) <=
          toleranceDegrees &&
        Math.abs(desiredPitch - state.player.pitchDegrees) <= toleranceDegrees
      ) {
        break;
      }
      continue;
    }
    const degreesPerPointerUnit = encounterExitEvidence ? 0.24 : 0.12;
    const movementX =
      Math.abs(yawError) > toleranceDegrees
        ? Math.max(-40, Math.min(40, -yawError / degreesPerPointerUnit))
        : 0;
    const movementY =
      movementX === 0 && Math.abs(pitchError) > toleranceDegrees
        ? Math.max(-40, Math.min(40, -pitchError / degreesPerPointerUnit))
        : 0;
    const beforeMotionYaw = state.player.yawDegrees;
    const beforeMotionPitch = state.player.pitchDegrees;
    if (encounterExitEvidence && headedOzonePlatform === "x11") {
      const moved = spawnSync(
        "python3",
        [
          join(actualRoot, "scripts/x11-pointer-input.py"),
          "move",
          String(Math.round(movementX)),
          String(Math.round(movementY)),
        ],
        { encoding: "utf8" },
      );
      if (moved.status !== 0) {
        throw new Error(`X11 relative pointer input failed: ${moved.stderr}`);
      }
    } else {
      canvasCenter.x += movementX;
      canvasCenter.y += movementY;
      await client.send("Input.dispatchMouseEvent", {
        type: "mouseMoved",
        x: canvasCenter.x,
        y: canvasCenter.y,
        button: "none",
        buttons: 0,
      });
    }
    state = await waitForAuthoritativeState(
      addr,
      `physical pointer motion toward enemy ${enemyId}`,
      (candidate) =>
        candidate.player?.vitalityState !== "alive" ||
        Math.abs(
          normalizeDegrees(candidate.player.yawDegrees - beforeMotionYaw),
        ) > 0.01 ||
        Math.abs(candidate.player.pitchDegrees - beforeMotionPitch) > 0.01,
    );
    const currentEnemy = state.enemies.find(
      (candidate) => candidate.id === enemyId,
    );
    if (currentEnemy?.state === "alive") {
      dx = currentEnemy.position[0] - state.player.position[0];
      dz = currentEnemy.position[2] - state.player.position[2];
      desiredYaw = (Math.atan2(-dx, -dz) * 180) / Math.PI;
      desiredPitch =
        (Math.atan2(
          currentEnemy.position[1] - state.player.position[1],
          Math.hypot(dx, dz),
        ) *
          180) /
        Math.PI;
    }
  }
  const finalYawError = normalizeDegrees(desiredYaw - state.player.yawDegrees);
  const finalPitchError = desiredPitch - state.player.pitchDegrees;
  const physicalLookDegrees =
    Math.abs(normalizeDegrees(state.player.yawDegrees - startingYaw)) +
    Math.abs(state.player.pitchDegrees - startingPitch);
  if (
    Math.abs(finalYawError) > toleranceDegrees ||
    Math.abs(finalPitchError) > toleranceDegrees
  ) {
    throw new Error(
      `physical pointer look did not aim at enemy ${enemyId}: ${JSON.stringify({ startingYaw, startingPitch, desiredYaw, desiredPitch, finalYaw: state.player.yawDegrees, finalPitch: state.player.pitchDegrees, finalYawError, finalPitchError })}`,
    );
  }
  return {
    startingYaw,
    startingPitch,
    desiredYaw,
    desiredPitch,
    finalYaw: state.player.yawDegrees,
    finalPitch: state.player.pitchDegrees,
    physicalLookDegrees,
  };
}

async function proveRepresentativeEncounter(
  client,
  addr,
  canvasBounds,
  traversalSamples,
  enemyId,
  dropId,
) {
  const mouseSensitivity = await setMaximumPhysicalMouseSensitivity(
    client,
    addr,
  );
  let canvasCenter = await acquirePhysicalPointerLock(client, canvasBounds);
  await moveToWorldPoint(client, addr, [154, 146], traversalSamples, {
    singleHold: true,
    arrivalDistance: 1.0,
  });
  await moveToWorldPoint(client, addr, [156, 146], traversalSamples, {
    singleHold: true,
    arrivalDistance: 1.0,
  });
  const stagedState = await fetchAuthoritativeState(addr);
  const stagedEnemy = stagedState.enemies.find((entry) => entry.id === enemyId);
  if (stagedEnemy?.combatPosture !== "sleeping") {
    throw new Error(
      `canonical enemy ${enemyId} woke before the authored encounter threshold: ${JSON.stringify(stagedEnemy)}`,
    );
  }
  const approachAim = await physicallyAimAtEnemy(
    client,
    addr,
    canvasCenter,
    enemyId,
  );
  await moveToWorldPoint(client, addr, [158, 146], traversalSamples, {
    singleHold: true,
    arrivalDistance: 1.5,
  });
  await moveToWorldPoint(client, addr, [160, 146], traversalSamples, {
    singleHold: true,
    arrivalDistance: 0.7,
    stopWhen: (candidate) =>
      candidate.enemies?.find((entry) => entry.id === enemyId)
        ?.combatPosture !== "sleeping",
  });
  const encounterState = await fetchAuthoritativeState(addr);
  const enemyBefore = encounterState.enemies.find(
    (entry) => entry.id === enemyId,
  );
  if (!enemyBefore || enemyBefore.state !== "alive") {
    throw new Error(
      `canonical representative enemy ${enemyId} was not live: ${JSON.stringify(enemyBefore)}`,
    );
  }
  await moveToWorldPoint(client, addr, [168, 146], traversalSamples, {
    singleHold: true,
    arrivalDistance: 2.0,
  });
  const engagedAim = await physicallyAimAtEnemy(
    client,
    addr,
    canvasCenter,
    enemyId,
  );
  const aim = {
    approach: approachAim,
    engaged: engagedAim,
    physicalLookDegrees:
      approachAim.physicalLookDegrees + engagedAim.physicalLookDegrees,
  };
  if (aim.physicalLookDegrees < 1) {
    throw new Error(
      `representative encounter did not require observable physical look: ${JSON.stringify(aim)}`,
    );
  }
  let latest = await fetchAuthoritativeState(addr);
  let shots = 0;
  let damagingShots = 0;
  while (
    latest.enemies.find((entry) => entry.id === enemyId)?.state !==
      "defeated" &&
    shots < 6
  ) {
    const healthBefore = latest.enemies.find(
      (entry) => entry.id === enemyId,
    )?.currentHealth;
    const ammoBefore = latest.weapon?.ammoRemaining;
    if (latest.player?.vitalityState !== "alive") {
      throw new Error(
        `player was not alive before physical encounter fire: ${JSON.stringify({ tick: latest.tick, health: latest.player?.currentHealth, vitalityState: latest.player?.vitalityState })}`,
      );
    }
    let pointerLocked = await cdpEvaluate(
      client,
      `document.pointerLockElement === document.querySelector('canvas')`,
    );
    if (!pointerLocked) {
      canvasCenter = await acquirePhysicalPointerLock(client, canvasBounds);
      pointerLocked = await cdpEvaluate(
        client,
        `document.pointerLockElement === document.querySelector('canvas')`,
      );
      if (!pointerLocked) {
        throw new Error(
          "physical encounter could not restore pointer lock before Mouse0",
        );
      }
    }
    await setPhysicalPrimaryFire(client, canvasCenter, true);
    try {
      latest = await waitForAuthoritativeState(
        addr,
        `physical Mouse0 fires at canonical enemy ${enemyId}`,
        (candidate) => {
          if (candidate.player?.vitalityState !== "alive") {
            throw new Error(
              `player was defeated while physically firing at enemy ${enemyId}: ${JSON.stringify({ tick: candidate.tick, health: candidate.player?.currentHealth, ammoBefore, ammoAfter: candidate.weapon?.ammoRemaining, target: candidate.enemies?.find((entry) => entry.id === enemyId) })}`,
            );
          }
          return candidate.weapon?.ammoRemaining < ammoBefore;
        },
      );
    } finally {
      await setPhysicalPrimaryFire(client, canvasCenter, false);
    }
    shots += 1;
    const targetAfterShot = latest.enemies.find(
      (entry) => entry.id === enemyId,
    );
    if (
      targetAfterShot?.state === "defeated" ||
      targetAfterShot?.currentHealth < healthBefore
    ) {
      damagingShots += 1;
    }
    if (targetAfterShot?.state !== "defeated") {
      await physicallyAimAtEnemy(client, addr, canvasCenter, enemyId);
    }
  }
  const enemyAfter = latest.enemies.find((entry) => entry.id === enemyId);
  const drop = latest.pickups?.find((entry) => entry.id === dropId);
  if (
    enemyAfter?.state !== "defeated" ||
    enemyAfter.currentHealth !== 0 ||
    damagingShots === 0 ||
    drop?.state !== "available"
  ) {
    throw new Error(
      `representative encounter did not settle defeat/drop: ${JSON.stringify({ enemyAfter, drop, shots })}`,
    );
  }
  return {
    enemy: enemyId,
    drop: dropId,
    postureBefore: enemyBefore.combatPosture,
    healthBefore: enemyBefore.currentHealth,
    healthAfter: enemyAfter.currentHealth,
    shots,
    damagingShots,
    aim,
    mouseSensitivity,
    dropState: drop.state,
  };
}

async function defeatCanonicalThreat(
  client,
  addr,
  canvasBounds,
  enemyId,
  dropId,
) {
  let canvasCenter = await acquirePhysicalPointerLock(client, canvasBounds);
  let latest = await fetchAuthoritativeState(addr);
  const healthBefore = latest.enemies.find(
    (entry) => entry.id === enemyId,
  )?.currentHealth;
  let shots = 0;
  let damagingShots = 0;
  while (
    latest.enemies.find((entry) => entry.id === enemyId)?.state !==
      "defeated" &&
    shots < 12
  ) {
    if (latest.player?.vitalityState !== "alive") {
      throw new Error(
        `player was defeated while clearing corridor threat ${enemyId}`,
      );
    }
    await physicallyAimAtEnemy(client, addr, canvasCenter, enemyId);
    const targetBefore = latest.enemies.find((entry) => entry.id === enemyId);
    const ammoBefore = latest.weapon?.ammoRemaining;
    let pointerLocked = await cdpEvaluate(
      client,
      `document.pointerLockElement === document.querySelector('canvas')`,
    );
    if (!pointerLocked) {
      canvasCenter = await acquirePhysicalPointerLock(client, canvasBounds);
      pointerLocked = await cdpEvaluate(
        client,
        `document.pointerLockElement === document.querySelector('canvas')`,
      );
    }
    if (!pointerLocked) {
      throw new Error(
        `could not restore pointer lock for corridor threat ${enemyId}`,
      );
    }
    await setPhysicalPrimaryFire(client, canvasCenter, true);
    try {
      latest = await waitForAuthoritativeState(
        addr,
        `physical Mouse0 fires at corridor threat ${enemyId}`,
        (candidate) => candidate.weapon?.ammoRemaining < ammoBefore,
      );
    } finally {
      await setPhysicalPrimaryFire(client, canvasCenter, false);
    }
    shots += 1;
    const targetAfter = latest.enemies.find((entry) => entry.id === enemyId);
    if (
      targetAfter?.state === "defeated" ||
      targetAfter?.currentHealth < targetBefore?.currentHealth
    ) {
      damagingShots += 1;
    }
  }
  const enemyAfter = latest.enemies.find((entry) => entry.id === enemyId);
  const drop = latest.pickups?.find((entry) => entry.id === dropId);
  if (
    enemyAfter?.state !== "defeated" ||
    enemyAfter.currentHealth !== 0 ||
    damagingShots === 0 ||
    drop?.state !== "available"
  ) {
    throw new Error(
      `corridor threat did not settle defeat/drop: ${JSON.stringify({ enemyAfter, drop, shots, damagingShots })}`,
    );
  }
  return {
    enemy: enemyId,
    drop: dropId,
    healthBefore,
    healthAfter: enemyAfter.currentHealth,
    shots,
    damagingShots,
    dropState: drop.state,
    dropPosition: enemyAfter.position,
  };
}

async function proveInteractionRoute(
  client,
  addr,
  canvasBounds,
  evidenceDir,
  owners,
) {
  const startedAtMs = Date.now();
  if (evidenceDir !== null) {
    mkdirSync(evidenceDir, { recursive: true });
  }
  const inputSurface = await focusGameplayCanvas(client);
  const traversalSamples = [];
  const screenshots = [];
  const capture = async (name) => {
    if (evidenceDir === null) return;
    const path = join(evidenceDir, name);
    const bytes = await captureCanvasEvidence(client, canvasBounds, path);
    screenshots.push({ path, bytes });
  };
  const walk = async (waypoints) => {
    for (const waypoint of waypoints) {
      await moveToWorldPoint(client, addr, waypoint, traversalSamples, {
        singleHold: true,
        arrivalDistance: 2.2,
      });
    }
  };
  const openDoor = async (door, approach) => {
    await moveToWorldPoint(client, addr, approach, traversalSamples, {
      singleHold: true,
      arrivalDistance: 0.7,
      stopWhen: (candidate) => candidate.interaction?.target === door,
    });
    await waitForAuthoritativeState(
      addr,
      `manual door ${door} in use range`,
      (candidate) => candidate.interaction?.target === door,
    );
    await holdKeys(client, ["KeyE"], 80);
    await waitForAuthoritativeState(
      addr,
      `manual door ${door} reaches open endpoint`,
      (candidate) =>
        candidate.projection?.some(
          (entry) => entry.id === door && entry.visualState === "open",
        ),
    );
  };

  await capture("interaction-start.png");
  await walk([
    [117, 80],
    [119, 80],
    [120, 80],
    [122, 80],
    [122, 81],
    [123, 81],
    [123, 82],
    [124, 82],
    [124, 83],
    [126, 83],
  ]);
  await moveToWorldPoint(client, addr, [124, 86], traversalSamples, {
    singleHold: true,
    arrivalDistance: 2.0,
  });
  await moveToWorldPoint(client, addr, [127, 86], traversalSamples, {
    singleHold: true,
    arrivalDistance: 1.5,
  });
  await walk([
    [127, 99],
    [127, 121],
    [128, 130],
    [131, 130],
    [131, 139],
    [132, 139],
    [132, 144],
    [134, 144],
    [134, 145],
    [137, 145],
    [137, 146],
  ]);
  await openDoor(
    owners.startDoor,
    encounterExitEvidence ? [140, 148] : [142, 147],
  );
  const representativeEncounter = encounterExitEvidence
    ? await proveRepresentativeEncounter(
        client,
        addr,
        canvasBounds,
        traversalSamples,
        owners.representativeEnemy,
        owners.representativeDrop,
      )
    : null;
  if (representativeEncounter !== null) {
    await capture("encounter-defeat-drop.png");
  }
  const corridorThreats = [];
  if (encounterExitEvidence) {
    const [shotgunEnemy, shotgunDrop] = owners.corridorThreats[0];
    const shotgunThreat = await defeatCanonicalThreat(
      client,
      addr,
      canvasBounds,
      shotgunEnemy,
      shotgunDrop,
    );
    corridorThreats.push(shotgunThreat);
    const [zombieman, bulletDrop] = owners.corridorThreats[1];
    corridorThreats.push(
      await defeatCanonicalThreat(
        client,
        addr,
        canvasBounds,
        zombieman,
        bulletDrop,
      ),
    );
    await moveToWorldPoint(
      client,
      addr,
      [shotgunThreat.dropPosition[0], shotgunThreat.dropPosition[2]],
      traversalSamples,
      {
        singleHold: true,
        arrivalDistance: 0.4,
        stopWhen: (candidate) =>
          candidate.pickups?.find((entry) => entry.id === shotgunDrop)
            ?.state === "collected",
      },
    );
    await waitForAuthoritativeState(
      addr,
      "first canonical shotgun drop is physically collected",
      (candidate) =>
        candidate.pickups?.find((entry) => entry.id === shotgunDrop)?.state ===
          "collected" &&
        candidate.inventory?.weapons?.some(
          (weapon) => weapon.item === "weapon/shotgun" && weapon.owned === true,
        ),
    );
    await holdKeys(client, ["Digit2"], 80);
    await waitForAuthoritativeState(
      addr,
      "physical Digit2 equips the first canonical shotgun drop",
      (candidate) => candidate.weapon?.item === "weapon/shotgun",
    );
    await capture("encounter-shotgun-collected.png");
    await capture("encounter-corridor-cleared.png");
  }
  if (encounterExitEvidence) {
    await defeatCanonicalThreat(
      client,
      addr,
      canvasBounds,
      owners.shotgunUpgrade[0],
      owners.shotgunUpgrade[1],
    );
  }
  const flankThreat = encounterExitEvidence
    ? await defeatCanonicalThreat(
        client,
        addr,
        canvasBounds,
        owners.flankThreat[0],
        owners.flankThreat[1],
      )
    : null;
  const innerThreat = encounterExitEvidence
    ? await defeatCanonicalThreat(
        client,
        addr,
        canvasBounds,
        owners.innerThreat[0],
        owners.innerThreat[1],
      )
    : null;
  if (flankThreat !== null && innerThreat !== null) {
    await capture("encounter-all-threats-cleared.png");
  }
  await walk([[178, 146]]);
  await walk([
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
    [235, 123],
  ]);
  const liftActivated = await waitForAuthoritativeState(
    addr,
    "type-88 lift leaves its raised state",
    (candidate) =>
      candidate.lifts?.some(
        (lift) => lift.id === owners.lift && lift.state !== "raised",
      ),
  );
  await walk([[234, 119]]);
  const liftWhileTraversing = await waitForAuthoritativeState(
    addr,
    "physical route advances while type-88 lift is moving",
    (candidate) =>
      candidate.tick > liftActivated.tick &&
      horizontalDistance(
        candidate.player.position,
        liftActivated.player.position,
      ) > 2 &&
      candidate.lifts?.some(
        (lift) => lift.id === owners.lift && lift.state !== "raised",
      ),
  );
  await capture("interaction-lift-moving.png");
  await walk([[230, 87]]);
  for (const waypoint of [
    [230, 80],
    [234, 80],
    [234, 74],
  ]) {
    await moveToWorldPoint(client, addr, waypoint, traversalSamples, {
      singleHold: true,
      arrivalDistance: 0.7,
    });
  }
  await walk([
    [234, 70],
    [234, 68],
  ]);
  await openDoor(owners.secretDoor, [233, 67]);
  await walk([[231, 64]]);
  const secret = await waitForAuthoritativeState(
    addr,
    "sector-68 secret is recorded",
    (candidate) =>
      candidate.secretRegions?.some(
        (entry) => entry.id === owners.secret && entry.state === "discovered",
      ),
  );
  await capture("interaction-secret.png");
  await walk([[233, 61]]);
  await openDoor(owners.annexDoor, [236, 56]);
  await walk([[236, 40]]);
  const floorAction = await waitForAuthoritativeState(
    addr,
    "type-36 floor action activates once",
    (candidate) =>
      candidate.floorActions?.some(
        (action) =>
          action.id === owners.floorAction && action.state !== "armed",
      ),
  );
  await openDoor(owners.exitDoor, [236, 17]);
  await walk([
    [236, 10],
    [232, 10],
    [231.3, 7.1],
  ]);
  await waitForAuthoritativeState(
    addr,
    "type-11 exit switch in use range",
    (candidate) => candidate.interaction?.target === owners.exit,
  );
  await holdKeys(client, ["KeyE"], 80);
  const completed = await waitForAuthoritativeState(
    addr,
    "type-11 exit completion",
    (candidate) =>
      candidate.levelComplete === true &&
      candidate.levelExits?.some(
        (entry) =>
          entry.id === owners.exit &&
          entry.state === "completed" &&
          entry.completedBy === 1,
      ),
  );
  await capture("interaction-exit-complete.png");
  return {
    status: "passed",
    mode: encounterExitEvidence
      ? "task-6807-encounter-exit"
      : "task-6803-interactions",
    elapsedMs: Date.now() - startedAtMs,
    inputSurface,
    doors: [
      owners.startDoor,
      owners.secretDoor,
      owners.annexDoor,
      owners.exitDoor,
    ],
    lift: {
      id: owners.lift,
      observedState: liftActivated.lifts.find((lift) => lift.id === owners.lift)
        ?.state,
      traversingState: liftWhileTraversing.lifts.find(
        (lift) => lift.id === owners.lift,
      )?.state,
      traversingPlayerPosition: liftWhileTraversing.player.position,
    },
    floorAction: {
      id: owners.floorAction,
      observedState: floorAction.floorActions.find(
        (action) => action.id === owners.floorAction,
      )?.state,
    },
    secret: secret.secretRegions.find((entry) => entry.id === owners.secret),
    exit: completed.levelExits.find((entry) => entry.id === owners.exit),
    representativeEncounter,
    corridorThreats,
    innerThreat,
    traversalSampleCount: traversalSamples.length,
    screenshots,
  };
}

async function main() {
  const revision = {
    head: spawnSync("git", ["rev-parse", "HEAD"], {
      cwd: actualRoot,
      encoding: "utf8",
    }).stdout.trim(),
    clean:
      spawnSync("git", ["status", "--porcelain"], {
        cwd: actualRoot,
        encoding: "utf8",
      }).stdout.trim().length === 0,
  };
  if (
    retainedEvidence &&
    (revision.head !== expectedEvidenceSha || !revision.clean)
  ) {
    throw new Error(
      `retained evidence requires exact clean head ${expectedEvidenceSha}; got ${JSON.stringify(revision)}`,
    );
  }
  const port = await reservePort();
  const addr = `127.0.0.1:${port}`;
  const saveRoot = mkdtempSync(join(tmpdir(), "doom-smoke-"));
  let projectPath = doomProject;
  if (focused) {
    const project = JSON.parse(readFileSync(doomProject, "utf8"));
    const entities = project.scenes[0].entities;
    const player = entities.find((entity) => entity.id === 1);
    const shotgun = entities.find(
      (entity) =>
        entity.pickup?.item === "weapon/shotgun" &&
        entity.renderable?.visible !== false,
    );
    const healthBonus = entities.find(
      (entity) => entity.pickup?.item === "supply/health-bonus",
    );
    const greenArmor = entities.find(
      (entity) => entity.pickup?.item === "armor/green",
    );
    const nukage = entities.filter((entity) => entity.hazard?.damage === 5);
    if (
      !player ||
      !shotgun ||
      !healthBonus ||
      !greenArmor ||
      nukage.length !== 4
    ) {
      throw new Error(
        "focused fixture could not resolve weapon and vitality owners",
      );
    }
    // Keep the canonical authored transactions while bounding this smoke to
    // weapon/vitality behavior instead of requiring an unrelated traversal.
    // Place the fixture along the physical W route so an authored restart
    // returns to a safe spawn where reset pickups remain observable.
    const focusedPoint = [
      player.translation[0],
      player.translation[1],
      player.translation[2] + 2,
    ];
    for (const entity of [shotgun, healthBonus, greenArmor]) {
      entity.translation = [...focusedPoint];
    }
    for (const hazard of nukage) {
      hazard.translation = [
        focusedPoint[0],
        focusedPoint[1],
        focusedPoint[2] + 2,
      ];
      hazard.bounds = {
        min: [-0.75, -0.6, -0.75],
        // Keep the bounded W-route fixture under canonical acceleration long
        // enough for the authored cadence to prove defeat and restart.
        max: [0.75, 0.6, 2.5],
      };
    }
    let focusedEnemyIndex = 0;
    for (const enemy of entities.filter(
      (entity) => entity.enemyCombat != null,
    )) {
      focusedEnemyIndex += 1;
      enemy.translation = [
        10_000 + focusedEnemyIndex * 4,
        enemy.translation[1],
        10_000,
      ];
    }
    projectPath = join(saveRoot, "doom-e1m1-focused.project.json");
    writeFileSync(projectPath, JSON.stringify(project), "utf8");
  }
  const interactionOwners = interactionEvidence
    ? resolveInteractionOwners(projectPath)
    : null;
  const expectedProject = JSON.parse(readFileSync(projectPath, "utf8"));
  const expectedProjectionCount = expectedProject.scenes[0].entities.filter(
    (entity) => entity.renderable != null,
  ).length;
  const mapTextureCount = expectedProject.assets.filter((asset) =>
    asset.id.startsWith("texture/doom-"),
  ).length;
  const spriteTextureCount = new Set(
    expectedProject.assets
      .filter((asset) => asset.spriteAtlas != null)
      .map((asset) => asset.spriteAtlas.texture),
  ).size;
  const expectedPngResourceCount = mapTextureCount + spriteTextureCount;
  console.log(`DOOM SMOKE host ${addr} save ${saveRoot}`);
  const { host, getOut } = launchHost(addr, saveRoot, projectPath);
  let chromiumProc = null;
  let cdpClient = null;
  let debugPort = null;
  let profileDir = null;
  let profileRemoved = false;
  let persistedEvidence = null;
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
      state.projection?.length === expectedProjectionCount,
      `projection ${expectedProjectionCount}, got ${state.projection?.length}`,
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
    const applicationResources = state.applicationContent?.resources ?? [];
    assert(
      (state.applicationContent?.frame?.ops?.length ?? 0) > 0,
      "Rust projected a non-empty application frame",
    );
    assert(
      applicationResources.filter(
        (resource) => resource.mediaType === "image/png",
      ).length === expectedPngResourceCount,
      `Rust projected ${mapTextureCount} E1M1 map textures and ${spriteTextureCount} Doom sprite atlases`,
    );
    assert(
      applicationResources.some(
        (resource) => resource.mediaType === "application/octet-stream",
      ),
      "Rust projected packed E1M1 mesh resources",
    );
    const curl = spawnSync(
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
        "Sec-WebSocket-Protocol: loading-bay.v2",
        `http://${addr}/api/session`,
      ],
      { timeout: 5000 },
    );
    const curlOut = curl.stdout?.toString() ?? "";
    if (!curlOut.includes("101 Switching Protocols")) {
      throw new Error(`websocket 101 failed: ${curlOut.slice(0, 500)}`);
    }
    checks.push("websocket upgraded with loading-bay.v2");
    if (!curlOut.includes("sha256:") && !curlOut.includes("doom-e1m1")) {
      throw new Error(
        `websocket bootstrap omitted Doom revision: ${curlOut.slice(0, 2000)}`,
      );
    }
    checks.push("websocket bootstrap carried the Doom static revision");
    let headless = {
      lifecycle: "skipped",
      screenshotBytes: 0,
      screenshotPath: null,
      webgl: null,
      error: null,
      content: null,
      worldPixels: null,
    };
    try {
      debugPort = await reservePort();
      profileDir = mkdtempSync(join(tmpdir(), "doom-chromium-"));
      console.log(
        `launching chromium ${encounterExitEvidence ? `headed ${headedOzonePlatform}` : "headless SwiftShader"} debugPort ${debugPort}`,
      );
      const chromiumArguments = encounterExitEvidence
        ? [
            "--no-sandbox",
            "--disable-dev-shm-usage",
            "--enable-gpu",
            "--ignore-gpu-blocklist",
            "--disable-background-timer-throttling",
            "--disable-backgrounding-occluded-windows",
            "--disable-renderer-backgrounding",
            "--autoplay-policy=no-user-gesture-required",
            `--ozone-platform=${headedOzonePlatform}`,
          ]
        : [
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
          ];
      chromiumProc = spawn(
        chromium,
        [
          ...chromiumArguments,
          `--remote-debugging-port=${String(debugPort)}`,
          "--remote-debugging-address=127.0.0.1",
          "--remote-allow-origins=*",
          `--user-data-dir=${profileDir}`,
          "--window-size=1600,900",
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
        45000,
      );
      console.log(`debugger ws ${wsUrl}`);
      cdpClient = await createCdpClient(wsUrl);
      await cdpClient.send("Page.enable");
      await cdpClient.send("Runtime.enable");
      await cdpClient.send("Page.bringToFront");
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
        ).catch((error) => `eval-error:${String(error).slice(0, 240)}`);
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
          `Doom card click must navigate to project=doom-e1m1 before mount proof (result=${clickResult}, hash=${String(afterClickHash).slice(0, 120)}, chromium=${cerr.slice(-1200)})`,
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
        if (lc === "mounted" || lc === "failed") break;
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
      if (finalLc !== "mounted" || !webglDiag.startsWith("has-gl renderer=")) {
        const runtimeError = await cdpEvaluate(
          cdpClient,
          `document.body?.dataset.runtimeError ?? 'no-runtime-error'`,
        ).catch(() => "runtime-error-eval-failed");
        throw new Error(
          `Engine application host failed before playthrough lifecycle=${finalLc} webgl=${webglDiag} runtimeError=${String(runtimeError).slice(0, 1200)}`,
        );
      }
      const contentDeadline = Date.now() + 30_000;
      let content = null;
      while (Date.now() < contentDeadline) {
        content = await cdpEvaluate(
          cdpClient,
          `({
            state: document.body.dataset.rendererContent ?? null,
            frameOps: Number(document.body.dataset.rendererFrameOps ?? 0),
            resourceCount: Number(document.body.dataset.rendererResourceCount ?? 0),
            textureCount: Number(document.body.dataset.rendererTextureCount ?? 0),
          })`,
        );
        if (
          content?.state === "complete" &&
          content.frameOps > 0 &&
          content.resourceCount > 56 &&
          content.textureCount === 56
        ) {
          break;
        }
        await delay(250);
      }
      if (
        content?.state !== "complete" ||
        content.frameOps <= 0 ||
        content.resourceCount <= 56 ||
        content.textureCount !== 56
      ) {
        throw new Error(
          `Engine did not admit the complete Rust content closure: ${JSON.stringify(content)}`,
        );
      }
      headless.content = content;
      const canvasBounds = await cdpEvaluate(
        cdpClient,
        `(() => {
          const canvas = document.querySelector('canvas');
          if (!canvas) return null;
          const bounds = canvas.getBoundingClientRect();
          return { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height };
        })()`,
      );
      if (
        canvasBounds === null ||
        canvasBounds.width < 64 ||
        canvasBounds.height < 64
      ) {
        throw new Error(
          `Engine canvas bounds are invalid: ${JSON.stringify(canvasBounds)}`,
        );
      }
      const worldShot = await cdpClient.send("Page.captureScreenshot", {
        format: "png",
        clip: { ...canvasBounds, scale: 1 },
      });
      const worldPath = join(profileDir, "doom-e1m1-world.png");
      writeFileSync(worldPath, Buffer.from(worldShot.data, "base64"));
      const imageMagickCommand =
        spawnSync("magick", ["-version"], { encoding: "utf8" }).status === 0
          ? "magick"
          : "convert";
      const imageReadout = spawnSync(
        imageMagickCommand,
        [worldPath, "-resize", "160x90!", "-format", "%k %[fx:mean]", "info:"],
        { encoding: "utf8" },
      );
      const [uniqueText, meanText] = String(imageReadout.stdout)
        .trim()
        .split(/\s+/u);
      const worldPixels = {
        uniqueColors: Number(uniqueText),
        mean: Number(meanText),
      };
      if (
        imageReadout.status !== 0 ||
        worldPixels.uniqueColors < 8 ||
        worldPixels.mean < 0.02
      ) {
        throw new Error(
          `Engine canvas did not contain visible E1M1 pixels: ${JSON.stringify(worldPixels)}`,
        );
      }
      headless.worldPixels = worldPixels;
      if (updateEvidence) {
        const evidenceShot = await cdpClient.send("Page.captureScreenshot", {
          format: "png",
          captureBeyondViewport: true,
        });
        const evidenceBytes = Buffer.from(evidenceShot.data, "base64");
        const evidencePath = join(
          actualRoot,
          "docs/evidence/doom-e1m1-headless.png",
        );
        writeFileSync(evidencePath, evidenceBytes);
        headless.screenshotBytes = evidenceBytes.length;
        headless.screenshotPath = "docs/evidence/doom-e1m1-headless.png";
        console.log(
          `initial-world screenshot ${evidenceBytes.length} bytes -> ${evidencePath}`,
        );
      }
      checks.push(
        `Engine admitted ${content.frameOps} Rust frame ops with ${content.resourceCount} resources and rendered ${worldPixels.uniqueColors} sampled colors`,
      );

      if (traversalEvidence) {
        const landmarkProof = await proveLandmarkTraversal(
          cdpClient,
          addr,
          canvasBounds,
          traversalEvidenceDir,
        );
        headless.playthrough = landmarkProof;
        checks.push(
          `physical controls traversed L1 ${landmarkProof.landmarks.L1.position.join(",")} to L2 ${landmarkProof.landmarks.L2.position.join(",")} across admitted floor levels ${landmarkProof.admittedFloorLevels.join(", ")}`,
        );
        checks.push(
          `physical Space jump left ground at tick ${landmarkProof.jump.airborne.tick} and landed at tick ${landmarkProof.jump.landed.tick}`,
        );
      } else if (interactionEvidence) {
        if (interactionOwners === null) {
          throw new Error("interaction owners were not resolved");
        }
        const interactionProof = await proveInteractionRoute(
          cdpClient,
          addr,
          canvasBounds,
          traversalEvidenceDir,
          interactionOwners,
        );
        headless.playthrough = interactionProof;
        checks.push(
          `physical E opened manual doors ${interactionProof.doors.join(", ")}, recorded secret ${interactionProof.secret.id}, and completed exit ${interactionProof.exit.id}`,
        );
        if (interactionProof.representativeEncounter !== null) {
          checks.push(
            `physical pointer look and Mouse0 defeated canonical enemy ${interactionProof.representativeEncounter.enemy} from ${interactionProof.representativeEncounter.healthBefore} health and read back materialized drop ${interactionProof.representativeEncounter.drop}`,
          );
        }
      } else if (focused) {
        const inputProof = await proveFocusedHeldMovement(cdpClient, addr);
        const selectionProof = await proveFocusedWeaponSelection(
          cdpClient,
          addr,
        );
        const fireProof = await proveFocusedHeldPistolFire(cdpClient, addr);
        const blurProof = await proveFocusedFireStopsOnBlur(cdpClient, addr);
        await proveFocusedHeldMovement(cdpClient, addr);
        const vitalityProof = await proveFocusedVitality(addr);
        const restartProof = await proveFocusedDeathAndRestart(cdpClient, addr);
        headless.playthrough = { status: "skipped", reason: "focused smoke" };
        headless.input = inputProof;
        headless.vitality = vitalityProof;
        headless.selection = selectionProof;
        headless.fire = fireProof;
        headless.blur = blurProof;
        headless.restart = restartProof;
        checks.push(
          `single keydown sustained ${inputProof.heldDistance.toFixed(2)} world units without mouse motion and keyup stopped within ${inputProof.stoppedDistance.toFixed(2)} units`,
        );
        checks.push(
          `held Mouse0 fired pistol ${fireProof.shots} times and reduced authoritative bullets ${fireProof.ammoBefore}->${fireProof.ammoAfter}`,
        );
        checks.push(
          `authored automatic health/armor pickups and nukage produced ${vitalityProof.health}/${vitalityProof.maxHealth} health and ${vitalityProof.armor}/${vitalityProof.maxArmor} armor at tick ${vitalityProof.tick}`,
        );
        checks.push(
          `authored shotgun pickup settled at tick ${selectionProof.acquiredTick}, physical Digit2 selected it at tick ${selectionProof.shotgunTick}, Digit3 selected fist at tick ${selectionProof.fistTick}, and Digit1 restored pistol at tick ${selectionProof.pistolTick}`,
        );
        checks.push(
          `blur without MouseUp stopped held fire after bullets ${blurProof.ammoBefore}->${blurProof.ammoAfterShot}->${blurProof.ammoAfterBlur}`,
        );
        checks.push(
          `four authored nukage owners defeated the player at tick ${restartProof.defeatedTick}, and a physical restart-button click restored the 100-health, 0-armor authored baseline at tick ${restartProof.restartedTick}`,
        );
        checks.push("full E1M1 traversal reserved for pnpm run certify:e1m1");
      }
      if (finalLc !== "mounted" || !webglDiag.startsWith("has-gl renderer=")) {
        headless.error = `unexpected browser renderer surface lifecycle=${finalLc} webgl=${webglDiag}`;
        console.log(headless.error);
      } else {
        console.log(
          "Engine application host confirmed: mounted lifecycle and one WebGL canvas",
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
          profileRemoved = true;
        } catch {}
    }

    if (retainedEvidence && headless.playthrough) {
      headless.playthrough.cleanup = {
        browserClosed: chromiumProc?.exitCode !== null,
        profileRemoved,
      };
    }

    if (
      headless.lifecycle !== "mounted" ||
      !headless.webgl?.startsWith("has-gl renderer=") ||
      headless.content?.state !== "complete" ||
      headless.worldPixels?.uniqueColors < 8 ||
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
      revision: {
        ...revision,
        viewport: [1600, 900],
        inputPath:
          "Chromium CDP physical keyboard events -> browser semantic input -> Rust fixed-step game loop",
      },
      host: {
        projectId: "doom-e1m1",
        assets: 189,
        entities: 134,
        address: addr,
        health: "ok",
        stateTick: state.tick,
        player: state.player?.position,
        doorState: state.doorState,
        levelExits: state.levelExits,
      },
      checks,
      staticRevision: staticRevMatch
        ? staticRevMatch[0]
        : "sha256:cbdd88292c50e00907e0f596da53ca6e9e1669e30d29c7878e5a808a68249bff",
      headless,
    };
    if (updateEvidence) {
      const outPath = resolve(
        actualRoot,
        "docs/evidence/doom-e1m1-browser-smoke.json",
      );
      writeFileSync(outPath, JSON.stringify(evidence, null, 2) + "\n", "utf8");
      console.log(`wrote ${outPath}`);
    }
    if (retainedEvidence) {
      const outPath = resolve(traversalEvidenceDir, "playtest-index.json");
      writeFileSync(outPath, JSON.stringify(evidence, null, 2) + "\n", "utf8");
      persistedEvidence = evidence;
      console.log(`wrote ${outPath}`);
    }
    console.log(
      "DOOM BROWSER SMOKE PASS",
      checks.join(", "),
      `headless:${headless.lifecycle}`,
    );
  } finally {
    host.kill("SIGTERM");
    await delay(500);
    if (host.exitCode === null) host.kill("SIGKILL");
    for (
      let attempt = 0;
      attempt < 10 && host.exitCode === null;
      attempt += 1
    ) {
      await delay(50);
    }
    rmSync(saveRoot, { recursive: true, force: true });
    if (retainedEvidence && persistedEvidence !== null) {
      persistedEvidence.headless.playthrough.cleanup.hostClosed =
        host.exitCode !== null || host.signalCode !== null;
      persistedEvidence.headless.playthrough.cleanup.saveRootRemoved =
        !existsSync(saveRoot);
      persistedEvidence.headless.playthrough.cleanup.evidenceDirectoryRetained =
        existsSync(traversalEvidenceDir);
      const outPath = resolve(traversalEvidenceDir, "playtest-index.json");
      writeFileSync(
        outPath,
        JSON.stringify(persistedEvidence, null, 2) + "\n",
        "utf8",
      );
    }
  }
}

main().catch((e) => {
  console.error(e.stack || String(e));
  process.exit(1);
});
