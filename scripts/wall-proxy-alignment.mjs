import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const PROJECT = resolve(ROOT, "content/projects/loading-bay.project.json");
const SCENE_ID = "scene/loading-bay";
const COLUMN_INSTANCES = Object.freeze([
  "level-column-arrival-a",
  "level-column-arrival-b",
  "level-column-generator-a",
  "level-column-generator-b",
  "level-column-dock-a",
  "level-column-dock-b",
]);
const CORNER_INSTANCES = Object.freeze([
  "level-corner-nw",
  "level-corner-ne",
  "level-corner-sw",
  "level-corner-se",
]);

function invariant(condition, message) {
  if (!condition) throw new Error(`wall-proxy alignment failed: ${message}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function rounded(value) {
  return Math.round(value * 1_000_000) / 1_000_000;
}

function boundsGap(left, right) {
  return Math.max(
    ...left.min.map((value, index) => Math.abs(value - right.min[index])),
    ...left.max.map((value, index) => Math.abs(value - right.max[index])),
  );
}

function horizontal(bounds) {
  return {
    min: [bounds.min[0], bounds.min[2]],
    max: [bounds.max[0], bounds.max[2]],
  };
}

function proxyCellBounds(x, z) {
  return { min: [x, z], max: [x + 1, z + 1] };
}

function mergeIntervals(intervals) {
  const merged = [];
  for (const interval of intervals.toSorted(
    (left, right) => left[0] - right[0],
  )) {
    const previous = merged.at(-1);
    if (previous === undefined || interval[0] > previous[1] + 0.000_001) {
      merged.push([...interval]);
    } else {
      previous[1] = Math.max(previous[1], interval[1]);
    }
  }
  return merged;
}

function uncoveredIntervals(occupied, collision) {
  const uncovered = [];
  for (const [occupiedMin, occupiedMax] of occupied) {
    let cursor = occupiedMin;
    for (const [collisionMin, collisionMax] of collision) {
      if (collisionMax <= cursor + 0.000_001) continue;
      if (collisionMin >= occupiedMax - 0.000_001) break;
      if (collisionMin > cursor + 0.000_001) {
        uncovered.push([cursor, Math.min(collisionMin, occupiedMax)]);
      }
      cursor = Math.max(cursor, collisionMax);
      if (cursor >= occupiedMax - 0.000_001) break;
    }
    if (cursor < occupiedMax - 0.000_001) {
      uncovered.push([cursor, occupiedMax]);
    }
  }
  return uncovered;
}

function instanceBounds(instance, assets) {
  invariant(
    JSON.stringify(instance.rotation) === JSON.stringify([0, 0, 0, 1]),
    `${instance.instanceId} must retain identity rotation for exact AABB comparison`,
  );
  const object = assets.get(instance.voxelObjectAssetId)?.voxelObject;
  invariant(
    object !== undefined,
    `${instance.instanceId} definition is missing`,
  );
  const { cellSize, pivot } = object.grid;
  return {
    min: object.bounds.min.map(
      (value, axis) =>
        instance.translation[axis] +
        (value - pivot[axis]) * cellSize * instance.scale[axis],
    ),
    max: object.bounds.max.map(
      (value, axis) =>
        instance.translation[axis] +
        (value + 1 - pivot[axis]) * cellSize * instance.scale[axis],
    ),
  };
}

function measurement(kind, instance, expected, assets, note) {
  const rawVisible = horizontal(instanceBounds(instance, assets));
  const visible = {
    min: rawVisible.min.map(rounded),
    max: rawVisible.max.map(rounded),
  };
  const gap = rounded(boundsGap(visible, expected));
  return {
    kind,
    instanceId: instance.instanceId,
    visible,
    gameplayProxy: expected,
    gap,
    note,
  };
}

function doorwayMeasurement(instance, expected, assets, byId, note) {
  const openingStart = expected.min[0];
  const openingEnd = expected.max[0];
  const z = expected.min[1];
  const wallId = (x) =>
    `level-wall-x${String(x).padStart(2, "0")}-z${String(z).padStart(2, "0")}`;
  const leftId = wallId(openingStart - 1);
  const rightId = wallId(openingEnd);
  const left = byId.get(leftId);
  const right = byId.get(rightId);
  invariant(left !== undefined, `${instance.instanceId} is missing ${leftId}`);
  invariant(
    right !== undefined,
    `${instance.instanceId} is missing ${rightId}`,
  );
  const leftBounds = horizontal(instanceBounds(left, assets));
  const rightBounds = horizontal(instanceBounds(right, assets));
  const outer = horizontal(instanceBounds(instance, assets));
  const visible = {
    min: [rounded(leftBounds.max[0]), rounded(outer.min[1])],
    max: [rounded(rightBounds.min[0]), rounded(outer.max[1])],
  };
  return {
    kind: "doorSurround",
    instanceId: instance.instanceId,
    visible,
    gameplayProxy: expected,
    gap: rounded(boundsGap(visible, expected)),
    collisionBackedSideInstances: {
      left: {
        instanceId: leftId,
        range: [rounded(leftBounds.min[0]), rounded(leftBounds.max[0])],
      },
      right: {
        instanceId: rightId,
        range: [rounded(rightBounds.min[0]), rounded(rightBounds.max[0])],
      },
    },
    overheadFrame: {
      min: outer.min.map(rounded),
      max: outer.max.map(rounded),
    },
    note,
  };
}

function doorwayCollisionCoverage(instance, assets, materialVoxels) {
  const object = assets.get(instance.voxelObjectAssetId)?.voxelObject;
  invariant(
    object !== undefined,
    `${instance.instanceId} definition is missing`,
  );
  const z = Math.round(instance.translation[2]);
  const { cellSize, pivot } = object.grid;
  const walkMinY = 1.25;
  const walkMaxY = 1.75;
  const occupiedCells = new Set();
  for (const run of object.defaultFrame.representation.sparseRuns) {
    const worldMinY =
      instance.translation[1] +
      (run.start[1] - pivot[1]) * cellSize * instance.scale[1];
    const worldMaxY =
      instance.translation[1] +
      (run.start[1] + 1 - pivot[1]) * cellSize * instance.scale[1];
    if (worldMaxY <= walkMinY || worldMinY >= walkMaxY) continue;
    for (let offset = 0; offset < run.length; offset += 1) {
      occupiedCells.add(run.start[0] + offset);
    }
  }
  const occupied = mergeIntervals(
    [...occupiedCells].map((cell) => [
      instance.translation[0] +
        (cell - pivot[0]) * cellSize * instance.scale[0],
      instance.translation[0] +
        (cell + 1 - pivot[0]) * cellSize * instance.scale[0],
    ]),
  );
  const collision = mergeIntervals(
    materialVoxels
      .filter(({ address }) => address[1] === 1 && address[2] === z)
      .map(({ address }) => [address[0], address[0] + 1]),
  );
  const uncovered = uncoveredIntervals(occupied, collision);
  invariant(
    uncovered.length === 0,
    `${instance.instanceId} has walk-height occupied intervals without Rust proxy collision: ${JSON.stringify(uncovered.map((interval) => interval.map(rounded)))}`,
  );
  return {
    occupied: occupied.map((interval) => interval.map(rounded)),
    collision: collision.map((interval) => interval.map(rounded)),
    uncovered: [],
    result: "pass",
  };
}

export async function collectWallProxyAlignment() {
  const projectBytes = await readFile(PROJECT);
  const project = JSON.parse(projectBytes);
  const scene = project.scenes.find(({ id }) => id === SCENE_ID);
  invariant(scene !== undefined, "Loading Bay scene is missing");
  invariant(
    scene.voxelEnvironment?.gameplayProxy === true,
    "material voxels must remain the explicit gameplay proxy",
  );
  const assets = new Map(project.assets.map((asset) => [asset.id, asset]));
  const instances = scene.voxelObjectInstances ?? [];
  const byId = new Map(
    instances.map((instance) => [instance.instanceId, instance]),
  );
  const material = new Set(
    scene.voxelEnvironment.materialVoxels.map(({ address }) =>
      address.join(","),
    ),
  );
  const brushAssets = project.assets.filter(({ voxelObject }) =>
    voxelObject?.assetId.startsWith("voxel-object/brush-"),
  );
  const finestCell = Math.min(
    ...brushAssets.map(({ voxelObject }) => voxelObject.grid.cellSize),
  );

  const walls = instances
    .filter(({ instanceId }) => instanceId.startsWith("level-wall-"))
    .map((instance) => {
      const [x, , z] = instance.translation;
      invariant(
        material.has(`${x},1,${z}`),
        `${instance.instanceId} has no proxy cell`,
      );
      return measurement(
        "wall",
        instance,
        proxyCellBounds(x, z),
        assets,
        "walk-facing visual slab matches its material-voxel cell",
      );
    });

  const columns = COLUMN_INSTANCES.map((instanceId) => {
    const instance = byId.get(instanceId);
    invariant(instance !== undefined, `${instanceId} is missing`);
    const [x, , z] = instance.translation;
    for (const y of [1, 2, 3]) {
      invariant(
        material.has(`${x},${y},${z}`),
        `${instanceId} proxy is missing y=${y}`,
      );
    }
    invariant(
      !byId.has(
        `level-wall-x${String(x).padStart(2, "0")}-z${String(z).padStart(2, "0")}`,
      ),
      `${instanceId} must not overlap a second wall presentation instance`,
    );
    return measurement(
      "column",
      instance,
      proxyCellBounds(x, z),
      assets,
      "column visual owns the same one-cell footprint as its three-cell-high proxy",
    );
  });

  const corners = CORNER_INSTANCES.map((instanceId) => {
    const instance = byId.get(instanceId);
    invariant(instance !== undefined, `${instanceId} is missing`);
    const [x, , z] = instance.translation;
    invariant(material.has(`${x},1,${z}`), `${instanceId} has no corner proxy`);
    return measurement(
      "corner",
      instance,
      proxyCellBounds(x, z),
      assets,
      "outer-corner accent uses the exact canonical boundary cell",
    );
  });

  const doorways = scene.entities
    .filter(({ door }) => door !== undefined)
    .sort((left, right) => left.id - right.id)
    .map((door) => {
      const instanceId = `level-doorway-owner-${String(door.id)}`;
      const instance = byId.get(instanceId);
      invariant(instance !== undefined, `${instanceId} is missing`);
      const z = Math.round(door.translation[2]);
      let openingStart = Math.floor(door.translation[0]);
      while (!material.has(`${openingStart - 1},1,${z}`)) openingStart -= 1;
      let openingEnd = Math.floor(door.translation[0]) + 1;
      while (!material.has(`${openingEnd},1,${z}`)) openingEnd += 1;
      const expected = { min: [openingStart, z], max: [openingEnd, z + 1] };
      const measured = doorwayMeasurement(
        instance,
        expected,
        assets,
        byId,
        `collision-backed wall jambs terminate at proxy opening edges ${String(openingStart)} and ${String(openingEnd)}; the decorative header has no walk-height occupancy`,
      );
      measured.globalCollisionCoverage = doorwayCollisionCoverage(
        instance,
        assets,
        scene.voxelEnvironment.materialVoxels,
      );
      return measured;
    });

  const passageLeft = byId.get("level-wall-x16-z22");
  const passageRight = byId.get("level-column-generator-a");
  invariant(
    passageLeft !== undefined && passageRight !== undefined,
    "narrow passage is missing",
  );
  const leftBounds = horizontal(instanceBounds(passageLeft, assets));
  const rightBounds = horizontal(instanceBounds(passageRight, assets));
  const narrowPassage = {
    kind: "narrowPassage",
    z: 22,
    visualOpenRange: [rounded(leftBounds.max[0]), rounded(rightBounds.min[0])],
    gameplayProxyOpenRange: [17, 18],
    width: rounded(rightBounds.min[0] - leftBounds.max[0]),
    gap: rounded(
      Math.max(
        Math.abs(leftBounds.max[0] - 17),
        Math.abs(rightBounds.min[0] - 18),
      ),
    ),
    note: "the one-unit route gap between the west wall and generator column is identical in both authorities",
  };

  const measurements = [
    ...walls,
    ...columns,
    ...corners,
    ...doorways,
    narrowPassage,
  ];
  const maximumGap = Math.max(...measurements.map(({ gap }) => gap));
  invariant(
    maximumGap <= finestCell,
    `maximum gap ${maximumGap} exceeds ${finestCell}`,
  );

  return {
    schemaVersion: 1,
    project: {
      path: "content/projects/loading-bay.project.json",
      schemaVersion: project.schemaVersion,
      hash: sha256(projectBytes),
      bytes: projectBytes.byteLength,
    },
    authority: {
      collisionNavigationOcclusion: "Rust-owned material-voxel gameplayProxy",
      visibleDetail: "Studio-authored repeated voxel-object brushes",
      secondCollisionSource: false,
    },
    threshold: {
      finestAuthoredBrushCell: finestCell,
      maximumAllowedGap: finestCell,
      measuredMaximumGap: maximumGap,
      result: "pass",
    },
    supportedAuthoring: {
      columnProxyVoxelsRequired: 18,
      preexisting: 4,
      addedThroughVoxelEdit: 14,
      current: 18,
      publication:
        "browser-host /api/voxel-edit -> Rust ProjectStore admission",
    },
    counts: {
      walls: walls.length,
      columns: columns.length,
      corners: corners.length,
      doorSurrounds: doorways.length,
      narrowPassages: 1,
      levelPlacements: instances.filter(({ instanceId }) =>
        instanceId.startsWith("level-"),
      ).length,
      totalInstances: instances.length,
    },
    measurements,
    visualEvidence: {
      studio: [
        {
          path: "docs/evidence/wall-proxy-studio-desktop.png",
          viewport: [1600, 900],
        },
        {
          path: "docs/evidence/wall-proxy-studio-narrow.png",
          viewport: [390, 844],
        },
      ],
      gameplay: [
        {
          path: "docs/evidence/wall-proxy-gameplay-desktop.png",
          viewport: [1600, 900],
        },
        {
          path: "docs/evidence/wall-proxy-gameplay-narrow.png",
          viewport: [390, 844],
        },
      ],
    },
  };
}
