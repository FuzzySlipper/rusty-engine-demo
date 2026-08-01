import { createHash } from "node:crypto";
import { readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

import { renderHandle } from "@rusty-engine/render-contracts";
import {
  loadAnimatedMeshGlbResource,
  MapAnimatedMeshAssetSource,
  ThreeRenderer,
} from "@rusty-engine/renderer-three/backend";

globalThis.self = globalThis;
globalThis.ProgressEvent ??= class ProgressEvent {
  constructor(type, init = {}) {
    this.type = type;
    Object.assign(this, init);
  }
};

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUTPUT = resolve(
  ROOT,
  "docs/evidence/animated-mesh-contact-sheets/source-equivalence.json",
);
const ENGINE_REVISION = JSON.parse(
  readFileSync(resolve(ROOT, "engine-source.json"), "utf8"),
).commit;
const TIMES = [0, 0.25, 0.5, 0.75, 1];
const CLIPS = ["idle", "run", "jump", "attack", "hit", "death"];
const ASSETS = [
  "mesh-animation/bay-rusher",
  "mesh-animation/arc-warden",
];
const requireFromRenderer = createRequire(
  import.meta.resolve("@rusty-engine/renderer-three"),
);
const THREE = await import(requireFromRenderer.resolve("three"));
const { GLTFLoader } = await import(
  requireFromRenderer.resolve("three/examples/jsm/loaders/GLTFLoader.js")
);
const SkeletonUtils = await import(
  requireFromRenderer.resolve("three/examples/jsm/utils/SkeletonUtils.js")
);

const committedProject = spawnSync(
  "git",
  ["show", "HEAD:content/projects/loading-bay.project.json"],
  { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
);
if (committedProject.status !== 0) {
  throw new Error(`could not read committed project: ${committedProject.stderr}`);
}
const project = JSON.parse(committedProject.stdout);
const results = [];

for (const assetId of ASSETS) {
  const authored = project.assets.find((asset) => asset.id === assetId);
  if (authored?.animatedMesh === undefined || authored.catalog?.sourcePath === undefined) {
    throw new Error(`committed project omits animated source ${assetId}`);
  }
  const sourcePath = resolve(ROOT, authored.catalog.sourcePath);
  const bytes = readFileSync(sourcePath);
  const sourceHash = createHash("sha256").update(bytes).digest("hex");
  if (sourceHash !== authored.catalog.hash) {
    throw new Error(`${assetId} source bytes do not match the committed catalog hash`);
  }
  const data = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  const [engineResource, directResource] = await Promise.all([
    loadAnimatedMeshGlbResource(assetId, data.slice(0)),
    new GLTFLoader().parseAsync(data.slice(0), ""),
  ]);
  const engineClips = engineResource.clips.map(({ name, duration }) => ({ name, duration }));
  const directClips = directResource.animations.map(({ name, duration }) => ({ name, duration }));
  assertClipInventory(assetId, engineClips, directClips);

  const renderer = new ThreeRenderer({
    animatedMeshSource: new MapAnimatedMeshAssetSource([engineResource]),
  });
  const first = renderHandle(6_001);
  const second = renderHandle(6_002);
  try {
    renderer.applyDiff({ op: "defineAnimatedMesh", asset: authored.animatedMesh });
    renderer.applyDiff(createInstance(first, assetId, [0, 0, 0], `${assetId}/first`));
    renderer.applyDiff(createInstance(second, assetId, [10, 0, -5], `${assetId}/second`));

    const samples = [];
    let maximumAbsoluteBoundsError = 0;
    for (const clipName of CLIPS) {
      const directClip = directResource.animations.find((clip) => clip.name === clipName);
      if (directClip === undefined) throw new Error(`${assetId} omits direct clip ${clipName}`);
      for (const normalizedTime of TIMES) {
        const engine = renderer.sampleAnimatedMesh(first, clipName, normalizedTime);
        const direct = directSample(directResource.scene, directClip, normalizedTime);
        const error = boundsError(engine.sampledWorldBounds, direct.bounds);
        maximumAbsoluteBoundsError = Math.max(maximumAbsoluteBoundsError, error);
        if (error > 0.000_001 || engine.sampledVertexCount !== direct.vertexCount) {
          throw new Error(
            `${assetId}/${clipName}@${String(normalizedTime)} diverged: bounds=${String(error)} vertices=${String(engine.sampledVertexCount)}/${String(direct.vertexCount)}`,
          );
        }
        samples.push({
          clip: clipName,
          normalizedTime,
          durationSeconds: engine.durationSeconds,
          engineBounds: roundedBounds(engine.sampledWorldBounds),
          directSourceBounds: roundedBounds(direct.bounds),
          sampledVertexCount: engine.sampledVertexCount,
        });
      }
    }

    const beforeSwitch = renderer.sampleAnimatedMesh(first, "attack", 0.5);
    renderer.sampleAnimatedMesh(first, "hit", 0.5);
    const afterSwitch = renderer.sampleAnimatedMesh(first, "attack", 0.5);
    const switchingError = boundsError(
      beforeSwitch.sampledWorldBounds,
      afterSwitch.sampledWorldBounds,
    );
    if (switchingError !== 0) {
      throw new Error(`${assetId} exact clip switching retained stale posture`);
    }

    const secondBefore = renderer.sampleAnimatedMesh(second, "idle", 0.5);
    renderer.sampleAnimatedMesh(first, "run", 0.75);
    const secondAfter = renderer.sampleAnimatedMesh(second, "idle", 0.5);
    const independentInstanceError = boundsError(
      secondBefore.sampledWorldBounds,
      secondAfter.sampledWorldBounds,
    );
    if (independentInstanceError !== 0) {
      throw new Error(`${assetId} sampling one instance changed another instance`);
    }
    const translated = translateBounds(
      sampleFor(samples, "idle", 0.5).directSourceBounds,
      [10, 0, -5],
    );
    const translatedInstanceError = boundsError(secondAfter.sampledWorldBounds, translated);
    if (translatedInstanceError > 0.000_001) {
      throw new Error(`${assetId} translated instance bounds diverged from source posture`);
    }

    renderer.applyDiff(playback(first, "idle", null));
    renderer.advanceAnimation(0.25);
    const otherBeforeFade = roundedBounds(bounds(renderer.objectFor(second)).bounds);
    renderer.applyDiff(playback(first, "attack", 0.1));
    renderer.advanceAnimation(0.05);
    const blendedBounds = roundedBounds(bounds(renderer.objectFor(first)).bounds);
    const otherAfterFade = roundedBounds(bounds(renderer.objectFor(second)).bounds);
    if (!finiteBounds(blendedBounds) || boundsError(otherBeforeFade, otherAfterFade) !== 0) {
      throw new Error(`${assetId} fade sampling was non-finite or crossed instance authority`);
    }

    const directQuality = sourceQuality(samples);
    results.push({
      asset: assetId,
      sourcePath: authored.catalog.sourcePath,
      sourceHash: `sha256:${sourceHash}`,
      import: {
        engineClips,
        directClips,
        clipInventoryMatches: true,
      },
      cloneBindInterpolation: {
        samples,
        maximumAbsoluteBoundsError: round(maximumAbsoluteBoundsError),
        exactVertexCounts: true,
      },
      switching: {
        attackHitAttackMaximumBoundsError: round(switchingError),
        stalePostureObserved: false,
      },
      fade: {
        fromClip: "idle",
        toClip: "attack",
        fadeSeconds: 0.1,
        sampledAfterSeconds: 0.05,
        blendedBounds,
        otherInstanceMaximumBoundsError: 0,
      },
      multiInstance: {
        translation: [10, 0, -5],
        translatedInstanceMaximumBoundsError: round(translatedInstanceError),
        independentInstanceMaximumBoundsError: round(independentInstanceError),
      },
      sourceQuality: directQuality,
    });
  } finally {
    renderer.dispose();
  }
}

const report = {
  schemaVersion: 1,
  engineRevision: ENGINE_REVISION,
  independentSampler: "Three GLTFLoader plus SkeletonUtils clone and AnimationMixer, separate from the Engine loader/registry instance",
  normalizedTimes: TIMES,
  clips: CLIPS,
  actors: results,
  conclusion: {
    importDivergenceObserved: false,
    cloneBindDivergenceObserved: false,
    interpolationDivergenceObserved: false,
    switchingOrFadeCrossInstanceDefectObserved: false,
    rendererCompensationRequired: false,
    remainingVisualDefectsAreAuthoredSourceMotion: true,
  },
};
const staged = `${OUTPUT}.pending`;
writeFileSync(staged, `${JSON.stringify(report, null, 2)}\n`);
renameSync(staged, OUTPUT);
rmSync(staged, { force: true });
console.log(
  `actor animation source equivalence passed: ${String(results.length)} actors, ${String(results.length * CLIPS.length * TIMES.length)} samples`,
);

function createInstance(handle, asset, translation, label) {
  return {
    op: "createAnimatedMeshInstance",
    handle,
    parent: null,
    instance: {
      asset,
      transform: {
        translation,
        rotation: [0, 0, 0, 1],
        scale: [1, 1, 1],
      },
      materialOverrides: [],
      playback: null,
      visible: true,
      metadata: {
        sourceEntity: null,
        sourceSceneNode: null,
        tags: ["animation-source-equivalence"],
        label,
      },
    },
  };
}

function playback(handle, clip, fadeSeconds) {
  return {
    op: "setAnimatedMeshPlayback",
    handle,
    playback: {
      kind: "play",
      clip,
      loop: "repeat",
      speed: 1,
      weight: 1,
      restart: true,
      fadeSeconds,
    },
  };
}

function directSample(source, clip, normalizedTime) {
  const clone = SkeletonUtils.clone(source);
  const mixer = new THREE.AnimationMixer(clone);
  const action = mixer.clipAction(clip);
  action.clampWhenFinished = true;
  action.setLoop(THREE.LoopOnce, 1);
  action.play();
  mixer.setTime(clip.duration * normalizedTime);
  const result = bounds(clone);
  mixer.stopAllAction();
  mixer.uncacheRoot(clone);
  return result;
}

function bounds(root) {
  if (root === undefined) throw new Error("retained animated object is missing");
  root.updateMatrixWorld(true);
  const box = new THREE.Box3();
  const vertex = new THREE.Vector3();
  let vertexCount = 0;
  root.traverse((node) => {
    if (node.isMesh !== true) return;
    const position = node.geometry.getAttribute("position");
    if (position === undefined) return;
    if (node.isSkinnedMesh === true) node.skeleton.update();
    for (let index = 0; index < position.count; index += 1) {
      vertex.fromBufferAttribute(position, index);
      if (node.isSkinnedMesh === true) node.applyBoneTransform(index, vertex);
      node.localToWorld(vertex);
      box.expandByPoint(vertex);
      vertexCount += 1;
    }
  });
  if (vertexCount === 0) throw new Error("animated source contains no vertices");
  return { bounds: { min: box.min.toArray(), max: box.max.toArray() }, vertexCount };
}

function assertClipInventory(asset, engine, direct) {
  if (engine.length !== direct.length) throw new Error(`${asset} clip count diverged`);
  for (let index = 0; index < engine.length; index += 1) {
    const left = engine[index];
    const right = direct[index];
    if (left.name !== right.name || Math.abs(left.duration - right.duration) > 0.000_001) {
      throw new Error(`${asset} clip inventory diverged at ${String(index)}`);
    }
  }
}

function sourceQuality(samples) {
  const byClip = Object.fromEntries(
    CLIPS.map((clip) => [clip, samples.filter((sample) => sample.clip === clip)]),
  );
  const movement = Object.fromEntries(
    CLIPS.map((clip) => [clip, maximumSampleBoundsDelta(byClip[clip])]),
  );
  const deathMinimumY = Math.min(
    ...byClip.death.map((sample) => sample.directSourceBounds.min[1]),
  );
  return {
    maximumBoundsDeltaByClip: movement,
    deathMinimumY: round(deathMinimumY),
    attackAndHitRemainAuthoredSourceMotion: true,
    deathMovesBelowSourceGroundPlane: deathMinimumY < 0,
    rendererCompensationApplied: false,
  };
}

function maximumSampleBoundsDelta(samples) {
  const first = samples[0].directSourceBounds;
  return round(Math.max(...samples.map((sample) => boundsError(first, sample.directSourceBounds))));
}

function sampleFor(samples, clip, normalizedTime) {
  const sample = samples.find(
    (candidate) => candidate.clip === clip && candidate.normalizedTime === normalizedTime,
  );
  if (sample === undefined) throw new Error(`missing recorded sample ${clip}@${String(normalizedTime)}`);
  return sample;
}

function translateBounds(value, translation) {
  return {
    min: value.min.map((coordinate, index) => coordinate + translation[index]),
    max: value.max.map((coordinate, index) => coordinate + translation[index]),
  };
}

function boundsError(left, right) {
  return Math.max(
    ...left.min.map((coordinate, index) => Math.abs(coordinate - right.min[index])),
    ...left.max.map((coordinate, index) => Math.abs(coordinate - right.max[index])),
  );
}

function roundedBounds(value) {
  return { min: value.min.map(round), max: value.max.map(round) };
}

function finiteBounds(value) {
  return [...value.min, ...value.max].every(Number.isFinite);
}

function round(value) {
  return Math.round(value * 1_000_000) / 1_000_000;
}
