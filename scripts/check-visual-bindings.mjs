import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const project = JSON.parse(
  readFileSync(
    resolve(repoRoot, "content/projects/loading-bay.project.json"),
    "utf8",
  ),
);

if (project.schemaVersion !== 25) {
  throw new Error(`expected project schema 25, found ${project.schemaVersion}`);
}

const actors = [];
const props = [];
for (const scene of project.scenes) {
  for (const entity of scene.entities) {
    const binding = entity.renderable?.visualBinding;
    if (entity.enemy === true) {
      actors.push(entity);
    } else if (binding !== undefined) {
      props.push(entity);
    }
  }
}

if (actors.length !== 8 || props.length !== 25) {
  throw new Error(
    `expected 8 bound actors and 25 bound props, found ${actors.length}/${props.length}`,
  );
}

const expectedActorStates = [
  "idle",
  "moving",
  "alert",
  "attacking",
  "hit",
  "defeated",
];
for (const actor of actors) {
  if (
    !actor.renderable.asset.startsWith("mesh-animation/") ||
    actor.renderable.initialClip !== "idle"
  ) {
    throw new Error(`actor ${actor.id} does not use its animated mesh asset`);
  }
  const states = actor.renderable.visualBinding.states;
  if (
    states.some(({ kind }) => kind !== "animation") ||
    states.map(({ state }) => state).join(",") !== expectedActorStates.join(",")
  ) {
    throw new Error(`actor ${actor.id} has a non-canonical clip binding`);
  }
}

for (const prop of props) {
  const states = prop.renderable.visualBinding.states;
  if (states.length < 2 || states.some(({ kind }) => kind !== "material")) {
    throw new Error(`prop ${prop.id} has a non-canonical material binding`);
  }
}

const staleAliases = project.assets.filter(({ id }) =>
  ["mesh/arc-warden", "mesh/bay-rusher"].includes(id),
);
if (staleAliases.length !== 0) {
  throw new Error("legacy empty actor mesh aliases remain in canonical content");
}

console.log(
  `visual bindings passed: ${actors.length} actors, ${props.length} props`,
);
