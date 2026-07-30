import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const KIT_ROOT = resolve(ROOT, "content/assets/actor-kit");
const REQUIRED_CLIPS = ["idle", "run", "jump", "attack", "hit", "death"];
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
  return JSON.parse(
    bytes
      .subarray(20, 20 + jsonLength)
      .toString("utf8")
      .replace(/\0+$/u, ""),
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
  const gltf = parseGlb(bytes, variant.file);
  invariant(
    gltf.animations?.map(({ name }) => name).join("\0") ===
      REQUIRED_CLIPS.join("\0"),
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
}

console.log(
  `actor kit check passed: ${manifest.variants.length} actors, ` +
    `${REQUIRED_CLIPS.length} clips each, embedded skins, exact hashes`,
);
