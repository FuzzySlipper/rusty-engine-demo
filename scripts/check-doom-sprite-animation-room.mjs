import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const project = JSON.parse(
  readFileSync(
    resolve(root, "content/projects/doom-sprite-animation-room.project.json"),
    "utf8",
  ),
);
const manifest = JSON.parse(
  readFileSync(resolve(root, "content/doom-e1m1/sprites/manifest.json"), "utf8"),
);
const expected = [
  ...["POSS", "SPOS", "TROO"].flatMap((family) =>
    ["idle", "walk", "attack", "pain", "death"].map((clip) => [family, clip]),
  ),
  ["BAL1", "flight"],
  ["BAL1", "impact"],
  ["BLUD", "hit"],
];
const invariant = (condition, message) => {
  if (!condition) throw new Error(message);
};
const closeArray = (actual, expectedValue, message) => {
  invariant(actual.length === expectedValue.length, message);
  actual.forEach((value, index) =>
    invariant(Math.abs(value - expectedValue[index]) < 1e-6, message),
  );
};

invariant(project.projectId === "doom-sprite-animation-room", "wrong project id");
invariant(project.scenes.length === 1, "animation room must have one scene");
const scene = project.scenes[0];
invariant(scene.id === project.entryScene, "entry scene must be direct");
invariant(project.voxelEnvironment == null, "E1M1 voxel volume must be absent");
invariant(
  scene.entities.every(
    (entity) => entity.enemy == null && entity.enemyCombat == null,
  ),
  "animation room must not use enemy AI",
);

const fixtures = scene.entities
  .filter((entity) => entity.doomSpriteInspection != null)
  .sort(
    (left, right) =>
      left.doomSpriteInspection.sequenceOrder -
      right.doomSpriteInspection.sequenceOrder,
  );
invariant(fixtures.length === expected.length, "wrong fixture count");
const sourceFrames = manifest.atlases.flatMap((atlas) => atlas.frames);
const sourceByName = new Map(sourceFrames.map((frame) => [frame.name, frame]));
const familyByPrefix = new Map(
  manifest.contract.families.map((family) => [family.prefix, family]),
);
const scale = manifest.contract.scale.presentationDoomUnitsPerEngineUnit;

for (const [sequenceOrder, fixture] of fixtures.entries()) {
  const inspection = fixture.doomSpriteInspection;
  const [prefix, clipId] = expected[sequenceOrder];
  invariant(inspection.sequenceOrder === sequenceOrder, "sequence order drifted");
  invariant(
    inspection.family === prefix && inspection.clip === clipId,
    `fixture ${sequenceOrder} label drifted`,
  );
  invariant(fixture.renderable.visible === false, "fixtures must start hidden");
  const family = familyByPrefix.get(prefix);
  const clip = family?.clips.find((candidate) => candidate.id === clipId);
  invariant(clip, `missing manifest ${prefix} ${clipId}`);
  const asset = project.assets.find(
    (candidate) => candidate.id === fixture.renderable.asset,
  );
  invariant(asset?.spriteAtlas, `${prefix} fixture asset is not a sprite`);
  const localSourceFrames = sourceFrames.filter((frame) => frame.family === prefix);
  const localFrameByName = new Map(
    localSourceFrames.map((frame, index) => [frame.name, index]),
  );
  const expectedFrames = [];
  const expectedOffsets = [];
  let sourceTics = 0;
  let runtimeTicks = 0;
  for (const step of clip.steps) {
    const rotation = family.directionalFrames
      .find((candidate) => candidate.frame === step.frame)
      ?.rotations.find(
        (candidate) => candidate.rotation === 1 || candidate.rotation === 0,
      );
    const source = rotation && sourceByName.get(rotation.sourceLump);
    invariant(source, `missing source frame for ${prefix} ${clipId}`);
    const localFrame = localFrameByName.get(source.name);
    const duration = (() => {
      if (step.tics < 0) return 1;
      sourceTics += step.tics;
      const nextTicks = Math.round(
        (sourceTics * 60) / manifest.contract.tickRateHz,
      );
      const ticks = Math.max(1, nextTicks - runtimeTicks);
      runtimeTicks = nextTicks;
      return ticks;
    })();
    const authoredFrame = asset.spriteAtlas.frames.find(
      (candidate) => candidate.frame === localFrame,
    );
    closeArray(authoredFrame.uvMin, source.uv.min, `${source.name} uvMin drifted`);
    closeArray(authoredFrame.uvMax, source.uv.max, `${source.name} uvMax drifted`);
    const size = [source.pixelSize[0] / scale, source.pixelSize[1] / scale];
    closeArray(authoredFrame.size, size, `${source.name} scale drifted`);
    const offset = [
      (0.5 - source.pivot[0]) * size[0],
      (0.5 - source.pivot[1]) * size[1],
    ];
    for (let tick = 0; tick < duration; tick += 1) {
      expectedFrames.push(localFrame);
      expectedOffsets.push(offset);
    }
  }
  const states = fixture.renderable.visualBinding.states;
  invariant(states.length === 1, "fixture must bind exactly one clip");
  const state = states[0];
  invariant(
    state.state === "default" && state.kind === "spriteFrames",
    "fixture must use the default sprite state",
  );
  invariant(state.ticksPerFrame === 1, "expanded timing must use one tick samples");
  invariant(state.loopMode === clip.loopMode, "loop mode drifted");
  invariant(
    JSON.stringify(state.frames) === JSON.stringify(expectedFrames),
    `${prefix} ${clipId} membership or timing drifted`,
  );
  invariant(state.directionalViews.length === 8, "grounding views must cover 1..8");
  state.directionalViews.forEach((view, index) => {
    invariant(view.rotation === index + 1, "grounding view order drifted");
    invariant(view.mirrored === false, "front inspection view must not mirror");
    invariant(
      JSON.stringify(view.frames) === JSON.stringify(expectedFrames),
      "grounding view membership drifted",
    );
    view.sourceOriginOffsets.forEach((offset, offsetIndex) =>
      closeArray(offset, expectedOffsets[offsetIndex], "source origin drifted"),
    );
  });
  const minimumDisplay =
    clip.loopMode === "once" ? expectedFrames.length + 75 : 150;
  invariant(
    inspection.displayTicks >= minimumDisplay,
    `${prefix} ${clipId} lacks a bounded playback/terminal window`,
  );
}

invariant(
  scene.entities.filter((entity) => entity.playerController != null).length === 1,
  "animation room must have one live player camera",
);
console.log(
  `Doom sprite animation room contract passed: ${fixtures.length} canonical clips, exact 35 Hz to 60 Hz timing, one-shot holds, projectile and blood FX`,
);
