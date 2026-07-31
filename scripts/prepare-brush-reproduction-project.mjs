import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const projectArgument = process.argv
  .slice(2)
  .find((argument) => argument !== "--");
const projectPath =
  projectArgument === undefined ? undefined : resolve(projectArgument);
if (projectPath === undefined) {
  throw new Error(
    "usage: node scripts/prepare-brush-reproduction-project.mjs <disposable-project.json>",
  );
}
const canonicalProjectPath = resolve(
  root,
  "content/projects/loading-bay.project.json",
);
if (projectPath === canonicalProjectPath) {
  throw new Error(
    "refusing to strip the canonical project; pass a disposable copy",
  );
}

const manifest = JSON.parse(
  await readFile(
    resolve(root, "content/assets/brush-kit/source-manifest.json"),
    "utf8",
  ),
);
const names = new Set(manifest.modules.map(({ name }) => name));
const project = JSON.parse(await readFile(projectPath, "utf8"));
const scene = project.scenes.find(({ id }) => id === project.entryScene);
if (scene === undefined) {
  throw new Error(`entry scene ${project.entryScene} is absent`);
}
if (
  !Array.isArray(scene.voxelObjectInstances) ||
  scene.voxelObjectInstances.length === 0
) {
  throw new Error("disposable project has no brush instances to remove");
}

const ownerIds = new Set(
  scene.voxelObjectInstances.map(({ ownerEntityId }) => ownerEntityId),
);
const allowedOwnerKeys = new Set([
  "childOrder",
  "id",
  "name",
  "rotation",
  "scale",
  "translation",
]);
for (const entity of scene.entities) {
  if (!ownerIds.has(entity.id)) continue;
  const unexpected = Object.keys(entity).filter(
    (key) => !allowedOwnerKeys.has(key),
  );
  if (unexpected.length > 0) {
    throw new Error(
      `brush owner ${String(entity.id)} has non-presentation fields: ${unexpected.join(", ")}`,
    );
  }
}
if (
  scene.voxelObjectInstances.some(
    ({ voxelObjectAssetId }) =>
      !voxelObjectAssetId.startsWith("voxel-object/brush-"),
  )
) {
  throw new Error(
    "disposable project contains a non-brush voxel-object instance",
  );
}

scene.entities = scene.entities.filter(({ id }) => !ownerIds.has(id));
scene.voxelObjectInstances = [];
project.assets = project.assets.filter(({ id }) => {
  if (id.startsWith("voxel-object/brush-")) return false;
  if (id.startsWith("material/brush-kit/")) {
    return !names.has(id.slice("material/brush-kit/".length));
  }
  if (id.startsWith("mesh/")) {
    return !names.has(id.slice("mesh/".length));
  }
  return true;
});

await writeFile(projectPath, `${JSON.stringify(project, null, 2)}\n`);
console.log(
  JSON.stringify({
    project: projectPath,
    removedInstances: ownerIds.size,
    remainingEntities: scene.entities.length,
    remainingAssets: project.assets.length,
  }),
);
