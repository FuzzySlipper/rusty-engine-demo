import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const KIT_ROOT = resolve(ROOT, "content/assets/actor-kit");
const REQUIRED_CLIPS = ["idle", "run", "jump", "attack", "hit", "death"];
const AUTHORED_LIMB_BONES = [
  "UpperChest",
  "Head",
  "LeftShoulder",
  "LeftArm",
  "LeftForeArm",
  "RightShoulder",
  "RightArm",
  "RightForeArm",
  "LeftUpLeg",
  "LeftLeg",
  "LeftFoot",
  "RightUpLeg",
  "RightLeg",
  "RightFoot",
];
const MAX_ACTOR_BYTES = 512 * 1024;

function invariant(condition, message) {
  if (!condition) throw new Error(`actor-kit invariant failed: ${message}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function parseGlb(bytes, label) {
  invariant(bytes.readUInt32LE(0) === 0x46546c67, `${label} must be GLB`);
  invariant(bytes.readUInt32LE(4) === 2, `${label} must use glTF 2`);
  invariant(bytes.readUInt32LE(8) === bytes.length, `${label} length drifted`);
  const jsonLength = bytes.readUInt32LE(12);
  invariant(
    bytes.readUInt32LE(16) === 0x4e4f534a,
    `${label} must begin with a JSON chunk`,
  );
  const gltf = JSON.parse(
    bytes
      .subarray(20, 20 + jsonLength)
      .toString("utf8")
      .replace(/\0+$/u, ""),
  );
  const binaryHeader = 20 + jsonLength;
  invariant(
    bytes.readUInt32LE(binaryHeader + 4) === 0x004e4942,
    `${label} must include a binary chunk`,
  );
  const binaryLength = bytes.readUInt32LE(binaryHeader);
  return {
    gltf,
    binary: bytes.subarray(binaryHeader + 8, binaryHeader + 8 + binaryLength),
  };
}

function readFloatAccessor(gltf, binary, accessorIndex, label) {
  const accessor = gltf.accessors[accessorIndex];
  invariant(accessor.componentType === 5126, `${label} must use float32`);
  invariant(accessor.type === "VEC4", `${label} must contain quaternions`);
  invariant(accessor.sparse === undefined, `${label} must not be sparse`);
  const view = gltf.bufferViews[accessor.bufferView];
  const stride = view.byteStride ?? 16;
  const start = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  return Array.from({ length: accessor.count }, (_, index) =>
    Array.from({ length: 4 }, (__, component) =>
      binary.readFloatLE(start + index * stride + component * 4),
    ),
  );
}

function assertArticulatedClip(gltf, binary, clip, file) {
  const jointNodes = new Set(gltf.skins[0].joints);
  const jointChannels = clip.channels.filter(
    ({ target }) =>
      target.path === "rotation" && jointNodes.has(target.node),
  );
  const jointNames = new Set(
    jointChannels.map(({ target }) => gltf.nodes[target.node].name),
  );
  invariant(
    jointNames.size >= 8,
    `${file} ${clip.name} must animate at least eight skin joints`,
  );
  let changedJointCount = 0;
  for (const channel of jointChannels) {
    const values = readFloatAccessor(
      gltf,
      binary,
      clip.samplers[channel.sampler].output,
      `${file} ${clip.name} ${gltf.nodes[channel.target.node].name}`,
    );
    const first = values[0];
    if (
      values.some((value) =>
        value.some((component, index) => Math.abs(component - first[index]) > 1e-4),
      )
    ) {
      changedJointCount += 1;
    }
  }
  invariant(
    changedJointCount >= 8,
    `${file} ${clip.name} must contain nontrivial rotation deltas on eight joints`,
  );
}

const manifestBytes = await readFile(resolve(KIT_ROOT, "source-manifest.json"));
const manifest = JSON.parse(manifestBytes);
invariant(manifest.schemaVersion === 1, "manifest schema must remain 1");
invariant(
  manifest.generator === "scripts/blender/build-loading-bay-actor-library.py",
  "manifest must name the reproducible Blender recipe",
);
invariant(
  manifest.source?.pack === "Kenney Animated Characters Retro" &&
    manifest.source.author === "Kenney" &&
    manifest.source.license === "CC0 1.0",
  "source identity and license must remain explicit",
);
invariant(
  manifest.variants?.length === 2,
  "exactly two reviewed actor variants are required",
);

const licenseBytes = await readFile(
  resolve(KIT_ROOT, "KENNEY-CC0-LICENSE.txt"),
);
invariant(
  manifest.licenseNotice?.path === "KENNEY-CC0-LICENSE.txt" &&
    manifest.licenseNotice.normalization ===
      "UTF-8 LF with source indentation and trailing whitespace removed" &&
    manifest.licenseNotice.bytes === licenseBytes.byteLength &&
    manifest.licenseNotice.sha256 === sha256(licenseBytes),
  "shipped normalized license notice must match its reviewed record",
);

const assetIds = new Set();
for (const variant of manifest.variants) {
  invariant(!assetIds.has(variant.assetId), `${variant.assetId} is duplicated`);
  assetIds.add(variant.assetId);
  invariant(
    variant.assetId === `mesh-animation/${variant.file.replace(/\.glb$/u, "")}`,
    `${variant.file} asset identity drifted`,
  );
  invariant(variant.targetHeight === 1.78, `${variant.file} height drifted`);
  invariant(variant.fps === 30, `${variant.file} FPS drifted`);
  invariant(
    variant.clips.map(({ id }) => id).join("\0") === REQUIRED_CLIPS.join("\0"),
    `${variant.file} manifest clips drifted`,
  );

  const bytes = await readFile(resolve(KIT_ROOT, variant.file));
  invariant(bytes.length === variant.bytes, `${variant.file} size drifted`);
  invariant(
    bytes.length <= MAX_ACTOR_BYTES,
    `${variant.file} exceeds ${MAX_ACTOR_BYTES} bytes`,
  );
  invariant(
    sha256(bytes) === variant.sha256,
    `${variant.file} content hash drifted`,
  );
  const { gltf, binary } = parseGlb(bytes, variant.file);
  invariant(
    gltf.animations?.map(({ name }) => name).sort().join("\0") ===
      [...REQUIRED_CLIPS].sort().join("\0"),
    `${variant.file} shipped animation set drifted`,
  );
  invariant(gltf.meshes?.length === 1, `${variant.file} must have one mesh`);
  invariant(gltf.skins?.length === 1, `${variant.file} must have one skin`);
  invariant(
    gltf.materials?.length === 1 && gltf.materials[0].name === "actor-skin",
    `${variant.file} must have one reviewed material`,
  );
  invariant(
    gltf.images?.length === 1 &&
      gltf.images[0].name === variant.skin.replace(/\.png$/u, "") &&
      gltf.images[0].bufferView !== undefined &&
      gltf.images[0].uri === undefined,
    `${variant.file} must embed its reviewed skin`,
  );
  invariant(
    gltf.buffers?.every(({ uri }) => uri === undefined),
    `${variant.file} must not depend on external buffers`,
  );
  invariant(
    gltf.animations.every(
      ({ channels, samplers }) => channels.length > 0 && samplers.length > 0,
    ),
    `${variant.file} must retain nonempty animation channels`,
  );
  for (const clip of gltf.animations) {
    const manifestClip = variant.clips.find(({ id }) => id === clip.name);
    const duration = Math.max(
      ...clip.samplers.map(({ input }) => gltf.accessors[input].max?.[0] ?? 0),
    );
    invariant(
      Math.abs(duration - manifestClip.durationSeconds) <= 0.000_001,
      `${variant.file} ${clip.name} manifest duration drifted from shipped GLB`,
    );
  }
  for (const clipName of ["attack", "hit"]) {
    const manifestClip = variant.clips.find(({ id }) => id === clipName);
    invariant(
      manifestClip.authoredJointChannels?.join("\0") ===
        AUTHORED_LIMB_BONES.join("\0"),
      `${variant.file} ${clipName} authored joint record drifted`,
    );
    assertArticulatedClip(
      gltf,
      binary,
      gltf.animations.find(({ name }) => name === clipName),
      variant.file,
    );
  }
}

console.log(
  `actor kit check passed: ${manifest.variants.length} actors, ` +
    `${REQUIRED_CLIPS.length} clips each, embedded skins, exact hashes`,
);
