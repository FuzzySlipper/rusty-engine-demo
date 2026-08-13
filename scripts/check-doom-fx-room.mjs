import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const room = JSON.parse(
  readFileSync(resolve(root, "content/projects/doom-fx-room.project.json"), "utf8"),
);
const doom = JSON.parse(
  readFileSync(resolve(root, "content/projects/doom-e1m1.project.json"), "utf8"),
);
const manifest = JSON.parse(
  readFileSync(resolve(root, "content/doom-e1m1/sprites/manifest.json"), "utf8"),
);

const closeArray = (actual, expected, message) => {
  assert.equal(actual.length, expected.length, message);
  actual.forEach((value, index) => {
    assert.ok(Math.abs(value - expected[index]) <= 1e-6, message);
  });
};
const closeOffsets = (actual, expected, message) => {
  assert.equal(actual.length, expected.length, message);
  actual.forEach((offset, index) =>
    closeArray(offset, expected[index], `${message} [${index}]`),
  );
};

assert.equal(room.projectId, "doom-fx-room");
assert.equal(room.entryScene, "scene/doom-fx-room");
assert.equal(room.scenes.length, 1, "FX room must contain one bounded scene");
assert.ok(
  !room.assets.some((asset) => asset.id === "voxel-volume/doom-e1m1"),
  "FX room must not embed the E1M1 map volume",
);
const scene = room.scenes[0];
assert.equal(scene.id, room.entryScene);
assert.equal(scene.name, "Doom Combat FX Clean Room");
assert.ok(scene.voxelEnvironment, "FX room must author its own bounded environment");
assert.equal(scene.voxelEnvironment.kind, "material");
assert.equal(scene.voxelEnvironment.voxelSize, 0.25);
assert.equal(scene.voxelEnvironment.chunkSize, 16);
assert.equal(scene.voxelEnvironment.gameplayProxy, true);
assert.deepEqual(
  scene.voxelEnvironment.materialVoxels,
  [],
  "FX room collision must stay on its two bounded static entities",
);

const entities = scene.entities;
const players = entities.filter((entity) => entity.playerController != null);
assert.equal(players.length, 1, "FX room must contain exactly one player");
const player = players[0];
assert.equal(player.name, "doom-fx-room-player");
assert.deepEqual(player.translation, [0, 0.5, -10]);
assert.equal(player.renderable.visible, false);
assert.deepEqual(
  player.playerController.bindings,
  doom.scenes[0].entities.find((entity) => entity.playerController != null)
    .playerController.bindings,
  "FX room must preserve ordinary Doom controls",
);

const enemies = entities.filter((entity) => entity.enemyCombat != null);
assert.equal(enemies.length, 2, "FX room must contain exactly two live combat enemies");
assert.equal(
  enemies.filter((entity) => entity.enemyCombat.attack.kind === "rangedHitscan").length,
  1,
  "FX room must contain one live hitscan combat owner",
);
assert.equal(
  enemies.filter((entity) => entity.enemyCombat.attack.kind === "projectile").length,
  1,
  "FX room must contain one live projectile combat owner",
);
assert.ok(enemies.every((entity) => entity.enemy === true));
assert.ok(
  enemies.every((entity) => entity.navigation.maxVisited === 64),
  "live FX enemies must preserve the bounded production navigation budget",
);

const bloodEnemy = entities.find(
  (entity) => entity.name === "doom-fx-blood-lane-zombieman",
);
const projectileEnemy = entities.find(
  (entity) => entity.name === "doom-fx-projectile-lane-imp",
);
assert.ok(bloodEnemy && projectileEnemy);
const sourceZombieman = doom.scenes[0].entities.find(
  (entity) => entity.name === "doom-zombieman-12",
);
const sourceImp = doom.scenes[0].entities.find(
  (entity) => entity.name === "doom-imp-1",
);
assert.ok(sourceZombieman && sourceImp);
assert.deepEqual(
  bloodEnemy.enemyCombat,
  sourceZombieman.enemyCombat,
  "blood lane must reuse the canonical Zombieman combat owner",
);
assert.deepEqual(
  projectileEnemy.enemyCombat,
  sourceImp.enemyCombat,
  "projectile lane must reuse the canonical Imp combat owner",
);
assert.deepEqual(bloodEnemy.translation, [-12, 1.25, 5]);
assert.deepEqual(projectileEnemy.translation, [12, 1.25, 5]);
assert.deepEqual(bloodEnemy.rotation ?? [0, 0, 0, 1], [0, 0, 0, 1]);
assert.deepEqual(projectileEnemy.rotation ?? [0, 0, 0, 1], [0, 0, 0, 1]);
assert.ok(
  Math.abs(bloodEnemy.translation[0] - projectileEnemy.translation[0]) >= 20,
  "live FX families must occupy separate lanes",
);

const targetWall = entities.filter((entity) => entity.name === "doom-fx-target-wall");
assert.equal(targetWall.length, 1, "FX room must contain one target-wall lane");
assert.deepEqual(targetWall[0].translation, [0, 2.25, 5]);
assert.equal(targetWall[0].renderable.visible, true);
assert.deepEqual(targetWall[0].bounds, {
  min: [-2.125, -2, -0.125],
  max: [2.125, 2, 0.125],
});
assert.deepEqual(targetWall[0].collision, {
  enabled: true,
  staticCollider: true,
});
assert.ok(
  bloodEnemy.translation[0] < -8 && projectileEnemy.translation[0] > 8,
  "blood and projectile lanes must flank the target lane",
);

const floor = entities.find((entity) => entity.name === "doom-fx-room-floor");
assert.ok(floor, "FX room must contain one bounded floor");
assert.deepEqual(floor.bounds, {
  min: [-20, -0.25, -16],
  max: [20, 0.25, 16],
});
assert.deepEqual(floor.collision, { enabled: true, staticCollider: true });

assert.ok(
  !entities.some((entity) => entity.name === "doom-fx-spawn-cover"),
  "FX room must expose unobstructed firing lanes once awareness is enabled",
);

const sourceFrames = manifest.atlases.flatMap((atlas) =>
  atlas.frames.map((frame) => ({
    ...frame,
    textureId: atlas.textureId,
    atlasFile: atlas.file,
    atlasSha256: atlas.pngSha256,
  })),
);
const sourceByName = new Map(sourceFrames.map((frame) => [frame.name, frame]));
const familyByPrefix = new Map(
  manifest.contract.families.map((family) => [family.prefix, family]),
);
const presentationScale =
  manifest.contract.scale.presentationDoomUnitsPerEngineUnit;
assert.equal(presentationScale, 28, "canonical presentation scale must remain 28:1");

const familySources = (prefix) => {
  const sources = sourceFrames.filter((frame) => frame.family === prefix);
  assert.ok(sources.length > 0, `missing canonical ${prefix} source frames`);
  return sources;
};

const sourceFor = (family, frameName, rotation) => {
  const directional = family.directionalFrames.find(
    (candidate) => candidate.frame === frameName,
  );
  const selected =
    directional?.rotations.find((candidate) => candidate.rotation === rotation) ??
    directional?.rotations.find((candidate) => candidate.rotation === 0);
  const source = selected && sourceByName.get(selected.sourceLump);
  assert.ok(source && selected, `missing ${family.prefix}${frameName} source frame`);
  return { selected, source };
};

function expectedClip(prefix, clipId) {
  const family = familyByPrefix.get(prefix);
  assert.ok(family, `missing canonical ${prefix} family`);
  const clip = family.clips.find((candidate) => candidate.id === clipId);
  assert.ok(clip, `missing canonical ${prefix} ${clipId} clip`);
  const localFrameBySource = new Map(
    familySources(prefix).map((frame, index) => [frame.name, index]),
  );
  const views = Array.from({ length: 8 }, (_, index) => ({
    rotation: index + 1,
    frames: [],
    mirrored: false,
    offsets: [],
  }));
  let sourceTicks = 0;
  let runtimeTicks = 0;
  for (const step of clip.steps) {
    const duration = (() => {
      if (step.tics < 0) return 1;
      sourceTicks += step.tics;
      const nextRuntimeTicks = Math.round(
        (sourceTicks * 60) / manifest.contract.tickRateHz,
      );
      const ticks = Math.max(1, nextRuntimeTicks - runtimeTicks);
      runtimeTicks = nextRuntimeTicks;
      return ticks;
    })();
    for (const view of views) {
      const { selected, source } = sourceFor(family, step.frame, view.rotation);
      const localFrame = localFrameBySource.get(source.name);
      assert.notEqual(localFrame, undefined, `missing local ${source.name} frame`);
      const size = [
        source.pixelSize[0] / presentationScale,
        source.pixelSize[1] / presentationScale,
      ];
      const offset = [
        (0.5 - source.pivot[0]) * size[0],
        (0.5 - source.pivot[1]) * size[1],
      ];
      for (let tick = 0; tick < duration; tick += 1) {
        view.frames.push(localFrame);
        view.offsets.push(offset);
      }
      if (selected.mirrored) view.mirrored = true;
    }
  }
  return { clip, views };
}

const expectedAssetSpecs = [
  ["POSS", "sprite/doom-zombieman"],
  ["TROO", "sprite/doom-imp"],
  ["BLUD", "sprite/doom-blood"],
  ["PUFF", "sprite/doom-puff"],
  ["BAL1", "sprite/doom-imp-fireball"],
];
for (const [prefix, assetId] of expectedAssetSpecs) {
  const asset = room.assets.find((candidate) => candidate.id === assetId);
  assert.ok(asset?.spriteAtlas, `${assetId} must be included as a sprite atlas`);
  const sources = familySources(prefix);
  assert.equal(asset.catalog.version, 1);
  assert.equal(asset.catalog.hash, `sha256:${sources[0].atlasSha256}`);
  assert.equal(
    asset.catalog.sourcePath,
    `content/doom-e1m1/sprites/${sources[0].atlasFile}`,
  );
  assert.equal(asset.spriteAtlas.id, assetId);
  assert.equal(asset.spriteAtlas.texture, sources[0].textureId);
  assert.equal(asset.spriteAtlas.frames.length, sources.length);
  asset.spriteAtlas.frames.forEach((authored, index) => {
    const source = sources[index];
    assert.equal(authored.frame, index, `${prefix} atlas frame order drifted`);
    closeArray(authored.uvMin, source.uv.min, `${source.name} uvMin drifted`);
    closeArray(authored.uvMax, source.uv.max, `${source.name} uvMax drifted`);
    closeArray(
      authored.size,
      [source.pixelSize[0] / presentationScale, source.pixelSize[1] / presentationScale],
      `${source.name} presentation size drifted`,
    );
  });
}

function assertBinding(entity, prefix, assetId, stateToClip) {
  assert.equal(entity.renderable.asset, assetId);
  assert.equal(entity.renderable.visualBinding.version, 2);
  const states = entity.renderable.visualBinding.states;
  assert.equal(states.length, Object.keys(stateToClip).length);
  for (const [stateName, clipId] of Object.entries(stateToClip)) {
    const state = states.find((candidate) => candidate.state === stateName);
    assert.ok(state, `missing ${prefix} ${stateName} state`);
    const expected = expectedClip(prefix, clipId);
    assert.equal(state.kind, "spriteFrames");
    assert.equal(state.ticksPerFrame, 1);
    assert.equal(state.loopMode, expected.clip.loopMode);
    assert.deepEqual(state.frames, expected.views[0].frames);
    assert.equal(state.directionalViews.length, 8);
    state.directionalViews.forEach((view, index) => {
      const expectedView = expected.views[index];
      assert.equal(view.rotation, index + 1);
      assert.equal(view.mirrored, expectedView.mirrored);
      assert.deepEqual(view.frames, expectedView.frames);
      closeOffsets(
        view.sourceOriginOffsets,
        expectedView.offsets,
        `${prefix} ${stateName} rotation ${index + 1} pivot offsets drifted`,
      );
    });
  }
}

assertBinding(
  bloodEnemy,
  "POSS",
  "sprite/doom-zombieman",
  { idle: "idle", moving: "walk", alert: "idle", attacking: "attack", hit: "pain", defeated: "death" },
);
assertBinding(
  projectileEnemy,
  "TROO",
  "sprite/doom-imp",
  { idle: "idle", moving: "walk", alert: "idle", attacking: "attack", hit: "pain", defeated: "death" },
);

const effectSpecs = [
  ["doom-fx-template-blood", "BLUD", "hit", "sprite/doom-blood"],
  ["doom-fx-template-bullet-puff", "PUFF", "impact", "sprite/doom-puff"],
  ["doom-fx-template-projectile-flight", "BAL1", "flight", "sprite/doom-imp-fireball"],
  ["doom-fx-template-projectile-impact", "BAL1", "impact", "sprite/doom-imp-fireball"],
];
assert.equal(
  entities.filter((entity) => entity.name.startsWith("doom-fx-template-")).length,
  effectSpecs.length,
  "FX room must contain exactly four transient effect templates",
);
for (const [name, prefix, clipId, assetId] of effectSpecs) {
  const template = entities.find((entity) => entity.name === name);
  assert.ok(template, `missing runtime effect template ${name}`);
  assert.equal(template.renderable.asset, assetId);
  assert.equal(template.renderable.visible, false);
  assert.equal(template.renderable.visualBinding.version, 2);
  const states = template.renderable.visualBinding.states;
  assert.equal(states.length, 1, `${name} must bind one canonical transient clip`);
  const state = states[0];
  const expected = expectedClip(prefix, clipId);
  assert.equal(state.state, "default");
  assert.equal(state.kind, "spriteFrames");
  assert.equal(state.ticksPerFrame, 1);
  assert.equal(state.loopMode, expected.clip.loopMode);
  assert.deepEqual(state.frames, expected.views[0].frames);
  assert.equal(state.directionalViews.length, 8);
  state.directionalViews.forEach((view, index) => {
    const expectedView = expected.views[index];
    assert.equal(view.rotation, index + 1);
    assert.equal(view.mirrored, expectedView.mirrored);
    assert.deepEqual(view.frames, expectedView.frames);
    closeOffsets(
      view.sourceOriginOffsets,
      expectedView.offsets,
      `${name} pivot offsets drifted`,
    );
  });
}

assert.equal(
  entities.filter((entity) => entity.doomSpriteInspection != null).length,
  0,
  "FX room must not use test-only sprite inspection fixtures",
);
assert.equal(
  entities.filter((entity) => entity.enemy != null && entity.enemyCombat == null).length,
  0,
  "every enemy must have an authoritative combat owner",
);

console.log(
  "Doom FX room contract passed: canonical BLUD blood, PUFF hitscan wall impacts, BAL1 projectile flight/impact, two live owners, exact source frames/pivots, separated lanes, and no E1M1 map",
);
