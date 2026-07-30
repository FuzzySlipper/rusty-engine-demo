import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const loadingBayPath = resolve(
  repoRoot,
  "content/projects/loading-bay.project.json",
);
const relayAnnexPath = resolve(
  repoRoot,
  "content/projects/relay-annex.project.json",
);
const proofRoot = mkdtempSync(join(tmpdir(), "loading-bay-visual-bindings-"));

const materialState = (
  state,
  textureTint,
  emissionColor,
  emissionIntensity,
) => ({
  state,
  kind: "material",
  textureTint,
  emissionColor,
  emissionIntensity,
});

const animationState = (state, clip, loopMode, fadeSeconds) => ({
  state,
  kind: "animation",
  clip,
  loopMode,
  speed: 1,
  fadeSeconds,
});

const active = (state) =>
  materialState(
    state,
    [0.62, 1, 0.82, 1],
    [0.12, 0.82, 0.52],
    0.35,
  );
const warning = (state) =>
  materialState(
    state,
    [1, 0.78, 0.48, 1],
    [0.75, 0.28, 0.05],
    0.12,
  );
const dim = (state) =>
  materialState(
    state,
    [0.58, 0.62, 0.68, 1],
    [0.12, 0.14, 0.18],
    0.04,
  );
const neutral = (state) =>
  materialState(state, [1, 1, 1, 1], [0, 0, 0], 0);

const actorBinding = {
  version: 1,
  states: [
    animationState("idle", "idle", "repeat", 0.12),
    animationState("moving", "run", "repeat", 0.1),
    animationState("alert", "idle", "repeat", 0.08),
    animationState("attacking", "attack", "repeat", 0.06),
    animationState("hit", "hit", "once", 0.04),
    animationState("defeated", "death", "once", 0.08),
  ],
};

const bindingsByCapability = [
  ["door", { version: 1, states: [warning("closed"), active("open")] }],
  ["switch", { version: 1, states: [warning("inactive"), active("active")] }],
  [
    "pickup",
    {
      version: 1,
      states: [
        dim("dormant"),
        active("available"),
        neutral("collected"),
      ],
    },
  ],
  ["hazard", { version: 1, states: [warning("active"), dim("cooling")] }],
  [
    "extractionBeacon",
    { version: 1, states: [warning("standby"), active("active")] },
  ],
  [
    "levelExit",
    { version: 1, states: [warning("available"), active("completed")] },
  ],
];

try {
  const loadingBay = JSON.parse(readFileSync(loadingBayPath, "utf8"));
  loadingBay.schemaVersion = 23;
  loadingBay.assets = loadingBay.assets.filter(
    ({ id }) => id !== "mesh/arc-warden" && id !== "mesh/bay-rusher",
  );

  let actorCount = 0;
  let propCount = 0;
  for (const scene of loadingBay.scenes) {
    for (const entity of scene.entities) {
      if (entity.renderable === undefined) {
        continue;
      }
      if (entity.enemy === true) {
        const actor = entity.renderable.asset.split("/").at(-1);
        entity.renderable.asset = `mesh-animation/${actor}`;
        entity.renderable.initialClip = "idle";
        entity.renderable.visualBinding = actorBinding;
        actorCount += 1;
        continue;
      }
      for (const [capability, binding] of bindingsByCapability) {
        if (entity[capability] !== undefined) {
          entity.renderable.visualBinding = binding;
          propCount += 1;
          break;
        }
      }
    }
  }

  if (actorCount !== 8 || propCount !== 25) {
    throw new Error(
      `expected 8 actors and 25 props, found ${String(actorCount)} actors and ${String(propCount)} props`,
    );
  }

  const relayAnnex = JSON.parse(readFileSync(relayAnnexPath, "utf8"));
  relayAnnex.schemaVersion = 23;

  for (const [name, project, target] of [
    ["loading-bay", loadingBay, loadingBayPath],
    ["relay-annex", relayAnnex, relayAnnexPath],
  ]) {
    const candidate = resolve(proofRoot, `${name}.candidate.json`);
    const canonical = resolve(proofRoot, `${name}.canonical.json`);
    writeFileSync(candidate, `${JSON.stringify(project, null, 2)}\n`);
    const result = spawnSync(
      "cargo",
      [
        "run",
        "--quiet",
        "--locked",
        "-p",
        "loading-bay-game",
        "--bin",
        "project-store",
        "--",
        "--input",
        candidate,
        "--output",
        canonical,
      ],
      {
        cwd: repoRoot,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      },
    );
    if (result.status !== 0) {
      throw new Error(
        `${name} failed Rust admission\n${result.stderr}${result.stdout}`,
      );
    }
    writeFileSync(target, readFileSync(canonical));
  }

  console.log(
    `authored schema-23 visual bindings: ${String(actorCount)} actors, ${String(propCount)} props`,
  );
} finally {
  rmSync(proofRoot, { recursive: true, force: true });
}
