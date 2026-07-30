import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, relative, resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const OUTPUT = resolve(ROOT, "content/assets/prop-kit");
const FACTORY_SOURCE = resolve(OUTPUT, "source/kenney-factory-kit");
const INDUSTRIAL_SOURCE = resolve(OUTPUT, "source/kenney-city-kit-industrial");

const EXTERNAL_ASSETS = [
  {
    name: "security-door",
    source: resolve(FACTORY_SOURCE, "door-wide-closed.glb"),
    sourcePack: "Kenney Factory Kit 3.0",
    targetSize: [2.4, 3, 0.55],
    color: [0.78, 0.43, 0.12, 1],
  },
  {
    name: "control-panel",
    source: resolve(FACTORY_SOURCE, "screen-panel-wide.glb"),
    sourcePack: "Kenney Factory Kit 3.0",
    targetSize: [1.35, 1.45, 0.5],
    color: [0.16, 0.42, 0.48, 1],
  },
  {
    name: "hazard-marker",
    source: resolve(FACTORY_SOURCE, "button-floor-square.glb"),
    sourcePack: "Kenney Factory Kit 3.0",
    targetSize: [2.35, 0.24, 2.35],
    color: [0.94, 0.38, 0.08, 1],
  },
  {
    name: "extraction-beacon",
    source: resolve(FACTORY_SOURCE, "scanner-high.glb"),
    sourcePack: "Kenney Factory Kit 3.0",
    targetSize: [1.45, 2.75, 1.45],
    color: [0.12, 0.68, 0.58, 1],
  },
  {
    name: "level-exit",
    source: resolve(FACTORY_SOURCE, "indicator-special-arrow.glb"),
    sourcePack: "Kenney Factory Kit 3.0",
    targetSize: [1.4, 2.35, 0.65],
    color: [0.18, 0.78, 0.55, 1],
  },
  {
    name: "status-runner",
    source: resolve(FACTORY_SOURCE, "scanner-low.glb"),
    sourcePack: "Kenney Factory Kit 3.0",
    targetSize: [0.7, 0.7, 0.7],
    color: [0.18, 0.72, 0.76, 1],
  },
  {
    name: "landmark-crane",
    source: resolve(FACTORY_SOURCE, "crane.glb"),
    sourcePack: "Kenney Factory Kit 3.0",
    targetSize: [6, 5, 4],
    color: [0.72, 0.36, 0.09, 1],
  },
  {
    name: "landmark-tank",
    source: resolve(INDUSTRIAL_SOURCE, "detail-tank.glb"),
    sourcePack: "Kenney City Kit Industrial 1.0",
    targetSize: [4, 4, 4],
    color: [0.24, 0.36, 0.4, 1],
  },
];

const ORIGINAL_ASSETS = [
  ["energy-cell", buildEnergyCell],
  ["scatter-shells", buildScatterShells],
  ["med-patch", buildMedPatch],
  ["impact-vest", buildImpactVest],
  ["maintenance-pass", buildMaintenancePass],
  ["arc-pistol", buildArcPistol],
  ["breach-scattergun", buildBreachScattergun],
  ["rivet-carbine", buildRivetCarbine],
  ["muzzle-flash", buildMuzzleFlash],
];

class MeshBuilder {
  constructor(name, materials) {
    this.name = name;
    this.positions = [];
    this.normals = [];
    this.indices = [];
    this.groups = [];
    this.materials = materials.map((material, slot) => ({
      slot,
      name: `${name}-${material.name}`,
      color: material.color,
      texture: null,
    }));
  }

  addTriangle(a, b, c, materialSlot = 0) {
    const normal = faceNormal(a, b, c);
    const base = this.positions.length / 3;
    this.positions.push(...a, ...b, ...c);
    this.normals.push(...normal, ...normal, ...normal);
    this.indices.push(base, base + 1, base + 2);
    this.#extendGroup(materialSlot, 3);
  }

  addQuad(a, b, c, d, materialSlot = 0) {
    this.addTriangle(a, b, c, materialSlot);
    this.addTriangle(a, c, d, materialSlot);
  }

  addBox(center, size, materialSlot = 0) {
    const [cx, cy, cz] = center;
    const [sx, sy, sz] = size.map((value) => value / 2);
    const p = [
      [cx - sx, cy - sy, cz - sz],
      [cx + sx, cy - sy, cz - sz],
      [cx + sx, cy + sy, cz - sz],
      [cx - sx, cy + sy, cz - sz],
      [cx - sx, cy - sy, cz + sz],
      [cx + sx, cy - sy, cz + sz],
      [cx + sx, cy + sy, cz + sz],
      [cx - sx, cy + sy, cz + sz],
    ];
    for (const face of [
      [p[0], p[3], p[2], p[1]],
      [p[4], p[5], p[6], p[7]],
      [p[0], p[1], p[5], p[4]],
      [p[3], p[7], p[6], p[2]],
      [p[0], p[4], p[7], p[3]],
      [p[1], p[2], p[6], p[5]],
    ]) {
      this.addQuad(...face, materialSlot);
    }
  }

  addCylinder(center, radius, length, axis, segments, materialSlot = 0) {
    const half = length / 2;
    const point = (along, first, second) => {
      if (axis === "x")
        return [center[0] + along, center[1] + first, center[2] + second];
      if (axis === "y")
        return [center[0] + first, center[1] + along, center[2] + second];
      return [center[0] + first, center[1] + second, center[2] + along];
    };
    const startCenter = point(-half, 0, 0);
    const endCenter = point(half, 0, 0);
    for (let index = 0; index < segments; index += 1) {
      const angleA = (index / segments) * Math.PI * 2;
      const angleB = ((index + 1) / segments) * Math.PI * 2;
      const a = [Math.cos(angleA) * radius, Math.sin(angleA) * radius];
      const b = [Math.cos(angleB) * radius, Math.sin(angleB) * radius];
      const startA = point(-half, a[0], a[1]);
      const startB = point(-half, b[0], b[1]);
      const endA = point(half, a[0], a[1]);
      const endB = point(half, b[0], b[1]);
      this.addQuad(startA, endA, endB, startB, materialSlot);
      this.addTriangle(startCenter, startB, startA, materialSlot);
      this.addTriangle(endCenter, endA, endB, materialSlot);
    }
  }

  addDiamond(center, size, materialSlot = 0) {
    const [cx, cy, cz] = center;
    const [sx, sy, sz] = size.map((value) => value / 2);
    const top = [cx, cy + sy, cz];
    const bottom = [cx, cy - sy, cz];
    const ring = [
      [cx - sx, cy, cz],
      [cx, cy, cz - sz],
      [cx + sx, cy, cz],
      [cx, cy, cz + sz],
    ];
    for (let index = 0; index < ring.length; index += 1) {
      const next = ring[(index + 1) % ring.length];
      this.addTriangle(top, ring[index], next, materialSlot);
      this.addTriangle(bottom, next, ring[index], materialSlot);
    }
  }

  toDocument() {
    return {
      schemaVersion: 1,
      name: `prop-kit/${this.name}`,
      positions: this.positions.map(roundFloat),
      normals: this.normals.map(roundFloat),
      indices: this.indices,
      materials: this.materials,
      groups: this.groups,
      collision: "visualOnly",
    };
  }

  #extendGroup(materialSlot, count) {
    const previous = this.groups.at(-1);
    if (
      previous !== undefined &&
      previous.materialSlot === materialSlot &&
      previous.start + previous.count === this.indices.length - count
    ) {
      previous.count += count;
      return;
    }
    this.groups.push({
      materialSlot,
      start: this.indices.length - count,
      count,
    });
  }
}

function buildEnergyCell() {
  const mesh = new MeshBuilder("energy-cell", [
    { name: "shell", color: [0.12, 0.24, 0.28, 1] },
    { name: "charge", color: [0.15, 0.86, 0.94, 1] },
  ]);
  for (const x of [-0.22, 0, 0.22]) {
    mesh.addCylinder([x, 0, 0], 0.105, 0.58, "y", 10, 1);
  }
  mesh.addBox([0, -0.33, 0], [0.76, 0.12, 0.32], 0);
  mesh.addBox([0, 0.33, 0], [0.76, 0.12, 0.32], 0);
  return mesh;
}

function buildScatterShells() {
  const mesh = new MeshBuilder("scatter-shells", [
    { name: "rack", color: [0.2, 0.15, 0.1, 1] },
    { name: "shell", color: [0.85, 0.25, 0.08, 1] },
    { name: "brass", color: [0.82, 0.63, 0.2, 1] },
  ]);
  mesh.addBox([0, -0.2, 0], [0.82, 0.1, 0.54], 0);
  for (const x of [-0.27, 0, 0.27]) {
    for (const z of [-0.14, 0.14]) {
      mesh.addCylinder([x, 0.03, z], 0.085, 0.42, "y", 10, 1);
      mesh.addCylinder([x, 0.255, z], 0.089, 0.06, "y", 10, 2);
    }
  }
  return mesh;
}

function buildMedPatch() {
  const mesh = new MeshBuilder("med-patch", [
    { name: "case", color: [0.84, 0.88, 0.84, 1] },
    { name: "cross", color: [0.12, 0.82, 0.35, 1] },
  ]);
  mesh.addBox([0, 0, 0], [0.75, 0.42, 0.58], 0);
  mesh.addBox([0, 0.225, 0], [0.18, 0.05, 0.4], 1);
  mesh.addBox([0, 0.225, 0], [0.46, 0.05, 0.15], 1);
  return mesh;
}

function buildImpactVest() {
  const mesh = new MeshBuilder("impact-vest", [
    { name: "armor", color: [0.16, 0.47, 0.82, 1] },
    { name: "trim", color: [0.07, 0.14, 0.2, 1] },
  ]);
  mesh.addBox([0, 0, 0], [0.62, 0.76, 0.28], 0);
  mesh.addBox([-0.37, 0.18, 0], [0.16, 0.42, 0.32], 0);
  mesh.addBox([0.37, 0.18, 0], [0.16, 0.42, 0.32], 0);
  mesh.addBox([0, -0.05, -0.17], [0.48, 0.5, 0.08], 1);
  mesh.addBox([0, 0.39, 0], [0.18, 0.14, 0.3], 1);
  return mesh;
}

function buildMaintenancePass() {
  const mesh = new MeshBuilder("maintenance-pass", [
    { name: "card", color: [0.72, 0.18, 0.76, 1] },
    { name: "stripe", color: [0.95, 0.74, 0.18, 1] },
    { name: "loop", color: [0.18, 0.22, 0.26, 1] },
  ]);
  mesh.addBox([0, 0, 0], [0.52, 0.72, 0.08], 0);
  mesh.addBox([0, 0.08, -0.052], [0.42, 0.12, 0.03], 1);
  mesh.addCylinder([0, 0.48, 0], 0.17, 0.08, "z", 12, 2);
  return mesh;
}

function buildArcPistol() {
  const mesh = weaponBuilder("arc-pistol", [
    [0.12, 0.28, 0.34, 1],
    [0.16, 0.76, 0.94, 1],
    [0.08, 0.12, 0.16, 1],
  ]);
  mesh.addBox([0, 0, 0], [0.38, 0.28, 0.52], 0);
  mesh.addCylinder([0, 0.04, -0.42], 0.085, 0.55, "z", 10, 1);
  mesh.addBox([0, -0.3, 0.09], [0.22, 0.44, 0.18], 2);
  mesh.addCylinder([0, 0.17, -0.03], 0.12, 0.24, "x", 10, 1);
  return mesh;
}

function buildBreachScattergun() {
  const mesh = weaponBuilder("breach-scattergun", [
    [0.24, 0.19, 0.13, 1],
    [0.62, 0.56, 0.46, 1],
    [0.7, 0.32, 0.1, 1],
  ]);
  mesh.addBox([0, 0, 0.18], [0.48, 0.34, 0.58], 0);
  mesh.addCylinder([-0.1, 0.05, -0.5], 0.07, 0.95, "z", 10, 1);
  mesh.addCylinder([0.1, 0.05, -0.5], 0.07, 0.95, "z", 10, 1);
  mesh.addBox([0, -0.03, -0.38], [0.46, 0.28, 0.38], 2);
  mesh.addBox([0, -0.2, 0.58], [0.32, 0.3, 0.38], 0);
  return mesh;
}

function buildRivetCarbine() {
  const mesh = weaponBuilder("rivet-carbine", [
    [0.1, 0.27, 0.3, 1],
    [0.3, 0.65, 0.67, 1],
    [0.84, 0.39, 0.12, 1],
  ]);
  mesh.addBox([0, 0, 0.1], [0.46, 0.34, 0.7], 0);
  mesh.addCylinder([0, 0.06, -0.52], 0.075, 0.72, "z", 10, 1);
  mesh.addBox([0, 0.02, -0.3], [0.32, 0.24, 0.45], 1);
  mesh.addBox([0, -0.36, 0.05], [0.22, 0.42, 0.24], 2);
  mesh.addBox([0, -0.15, 0.58], [0.32, 0.22, 0.32], 0);
  return mesh;
}

function buildMuzzleFlash() {
  const mesh = new MeshBuilder("muzzle-flash", [
    { name: "flare", color: [1, 0.76, 0.14, 1] },
  ]);
  mesh.addDiamond([0, 0, 0], [0.28, 0.28, 0.4], 0);
  mesh.addDiamond([0, 0, 0], [0.4, 0.16, 0.22], 0);
  return mesh;
}

function weaponBuilder(name, colors) {
  return new MeshBuilder(
    name,
    colors.map((color, index) => ({
      name: ["receiver", "accent", "grip"][index],
      color,
    })),
  );
}

async function readGlbMesh(path, name, targetSize, color) {
  const bytes = await readFile(path);
  if (bytes.readUInt32LE(0) !== 0x46546c67 || bytes.readUInt32LE(4) !== 2) {
    throw new Error(`${path} is not a GLB 2.0 file`);
  }
  const jsonLength = bytes.readUInt32LE(12);
  const jsonType = bytes.readUInt32LE(16);
  if (jsonType !== 0x4e4f534a) {
    throw new Error(`${path} has no leading GLB JSON chunk`);
  }
  const document = JSON.parse(
    bytes.subarray(20, 20 + jsonLength).toString("utf8"),
  );
  const binaryHeader = 20 + paddedLength(jsonLength);
  const binaryLength = bytes.readUInt32LE(binaryHeader);
  const binaryType = bytes.readUInt32LE(binaryHeader + 4);
  if (binaryType !== 0x004e4942) {
    throw new Error(`${path} has no GLB binary chunk`);
  }
  const binary = bytes.subarray(
    binaryHeader + 8,
    binaryHeader + 8 + binaryLength,
  );
  const builder = new MeshBuilder(name, [{ name: "surface", color }]);
  for (const mesh of document.meshes ?? []) {
    for (const primitive of mesh.primitives ?? []) {
      if (primitive.mode !== undefined && primitive.mode !== 4) {
        throw new Error(`${path} contains a non-triangle primitive`);
      }
      const positions = readAccessor(
        document,
        binary,
        primitive.attributes.POSITION,
      );
      const indices =
        primitive.indices === undefined
          ? Array.from({ length: positions.length / 3 }, (_, index) => index)
          : readAccessor(document, binary, primitive.indices);
      for (let offset = 0; offset < indices.length; offset += 3) {
        const a = vectorAt(positions, indices[offset]);
        const b = vectorAt(positions, indices[offset + 1]);
        const c = vectorAt(positions, indices[offset + 2]);
        builder.addTriangle(a, b, c, 0);
      }
    }
  }
  fitMesh(builder, targetSize);
  return { builder, bytes };
}

function readAccessor(document, binary, accessorIndex) {
  const accessor = document.accessors?.[accessorIndex];
  const view = document.bufferViews?.[accessor?.bufferView];
  if (
    accessor === undefined ||
    view === undefined ||
    accessor.sparse !== undefined
  ) {
    throw new Error(`unsupported GLB accessor ${String(accessorIndex)}`);
  }
  const components = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 }[accessor.type];
  const bytesPerComponent = { 5121: 1, 5123: 2, 5125: 4, 5126: 4 }[
    accessor.componentType
  ];
  if (components === undefined || bytesPerComponent === undefined) {
    throw new Error(`unsupported GLB accessor format ${accessor.type}`);
  }
  const stride = view.byteStride ?? components * bytesPerComponent;
  const base = (view.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
  const values = [];
  for (let index = 0; index < accessor.count; index += 1) {
    for (let component = 0; component < components; component += 1) {
      const offset = base + index * stride + component * bytesPerComponent;
      values.push(
        accessor.componentType === 5121
          ? binary.readUInt8(offset)
          : accessor.componentType === 5123
            ? binary.readUInt16LE(offset)
            : accessor.componentType === 5125
              ? binary.readUInt32LE(offset)
              : binary.readFloatLE(offset),
      );
    }
  }
  return values;
}

function fitMesh(builder, targetSize) {
  const bounds = meshBounds(builder.positions);
  const sourceSize = bounds.max.map((value, axis) => value - bounds.min[axis]);
  const scale = sourceSize.map((value, axis) =>
    value <= Number.EPSILON ? 1 : targetSize[axis] / value,
  );
  const center = bounds.min.map(
    (value, axis) => (value + bounds.max[axis]) / 2,
  );
  for (let offset = 0; offset < builder.positions.length; offset += 3) {
    for (let axis = 0; axis < 3; axis += 1) {
      builder.positions[offset + axis] =
        (builder.positions[offset + axis] - center[axis]) * scale[axis];
    }
  }
  builder.normals.length = 0;
  for (let offset = 0; offset < builder.positions.length; offset += 9) {
    const normal = faceNormal(
      builder.positions.slice(offset, offset + 3),
      builder.positions.slice(offset + 3, offset + 6),
      builder.positions.slice(offset + 6, offset + 9),
    );
    builder.normals.push(...normal, ...normal, ...normal);
  }
}

function meshBounds(positions) {
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  for (let offset = 0; offset < positions.length; offset += 3) {
    for (let axis = 0; axis < 3; axis += 1) {
      min[axis] = Math.min(min[axis], positions[offset + axis]);
      max[axis] = Math.max(max[axis], positions[offset + axis]);
    }
  }
  return { min, max };
}

function faceNormal(a, b, c) {
  const ab = b.map((value, axis) => value - a[axis]);
  const ac = c.map((value, axis) => value - a[axis]);
  const normal = [
    ab[1] * ac[2] - ab[2] * ac[1],
    ab[2] * ac[0] - ab[0] * ac[2],
    ab[0] * ac[1] - ab[1] * ac[0],
  ];
  const length = Math.hypot(...normal);
  return length <= Number.EPSILON
    ? [0, 1, 0]
    : normal.map((value) => value / length);
}

function vectorAt(values, index) {
  return values.slice(index * 3, index * 3 + 3);
}

function paddedLength(length) {
  return Math.ceil(length / 4) * 4;
}

function roundFloat(value) {
  return Math.round(value * 1_000_000) / 1_000_000;
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function writeMesh(builder) {
  const document = builder.toDocument();
  const bytes = `${JSON.stringify(document, null, 2)}\n`;
  const path = resolve(OUTPUT, `${builder.name}.mesh.json`);
  await writeFile(path, bytes);
  const bounds = meshBounds(builder.positions);
  return {
    assetId: `mesh/prop-kit/${builder.name}`,
    importSourcePath: relative(ROOT, path),
    contentSha256: sha256(bytes),
    byteCount: Buffer.byteLength(bytes),
    vertexCount: builder.positions.length / 3,
    triangleCount: builder.indices.length / 3,
    materialSlots: builder.materials.length,
    bounds: {
      min: bounds.min.map(roundFloat),
      max: bounds.max.map(roundFloat),
    },
    collisionProxyIntent: "visualOnly; gameplay proxies remain entity-owned",
  };
}

async function main() {
  await mkdir(OUTPUT, { recursive: true });
  const assets = [];
  for (const external of EXTERNAL_ASSETS) {
    const imported = await readGlbMesh(
      external.source,
      external.name,
      external.targetSize,
      external.color,
    );
    assets.push({
      ...(await writeMesh(imported.builder)),
      origin: "external",
      sourcePack: external.sourcePack,
      sourcePath: relative(ROOT, external.source),
      sourceSha256: sha256(imported.bytes),
      modifications:
        "direct GLB geometry extraction; centered, bounded non-uniform scale; project palette material; texture omitted",
    });
  }
  for (const [name, build] of ORIGINAL_ASSETS) {
    const built = build();
    assets.push({
      ...(await writeMesh(built)),
      origin: "original",
      sourcePack: null,
      sourcePath: "scripts/build-loading-bay-prop-kit.mjs",
      sourceSha256: sha256(
        await readFile(resolve(ROOT, "scripts/build-loading-bay-prop-kit.mjs")),
      ),
      modifications: "original deterministic low-poly composition",
    });
  }
  const licenseFiles = [
    resolve(OUTPUT, "KENNEY-FACTORY-KIT-LICENSE.txt"),
    resolve(OUTPUT, "KENNEY-CITY-KIT-INDUSTRIAL-LICENSE.txt"),
  ];
  const manifest = {
    schemaVersion: 1,
    generatedBy: "scripts/build-loading-bay-prop-kit.mjs",
    licenses: await Promise.all(
      licenseFiles.map(async (path) => {
        const bytes = await readFile(path);
        return {
          path: relative(ROOT, path),
          sha256: sha256(bytes),
        };
      }),
    ),
    assets,
  };
  await writeFile(
    resolve(OUTPUT, "source-manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  console.log(
    JSON.stringify({
      assetCount: assets.length,
      vertices: assets.reduce((sum, asset) => sum + asset.vertexCount, 0),
      triangles: assets.reduce((sum, asset) => sum + asset.triangleCount, 0),
    }),
  );
}

await main();
