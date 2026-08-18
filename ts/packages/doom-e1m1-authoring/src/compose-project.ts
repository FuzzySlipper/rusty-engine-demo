import { readFileSync, writeFileSync, mkdirSync, unlinkSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const SCALE = 16;
const MIN_X = -768;
const MIN_Y = -4864;
const MIN_FLOOR = -136;

function kebab(name: string): string {
  return name
    .toLowerCase()
    .replace(/_/g, "-")
    .replace(/[^a-z0-9-]/g, "-");
}

function wadToWorld(
  x: number,
  y: number,
  floorHeight: number,
): [number, number, number] {
  return [
    (x - MIN_X) / SCALE,
    (floorHeight - MIN_FLOOR) / SCALE + 0.5,
    (y - MIN_Y) / SCALE,
  ];
}

interface Intermediate {
  source: { wadSha256: string; wadByteLength: number };
  level: {
    vertices: { x: number; y: number }[];
    sectors: {
      floorHeight: number;
      ceilingHeight: number;
      floorTexture: string;
      ceilingTexture: string;
      lightLevel: number;
      special: number;
      tag: number;
    }[];
    sidedefs: {
      sector: number;
      lowerTexture: string;
      middleTexture: string;
      upperTexture: string;
    }[];
    linedefs: {
      startVertex: number;
      endVertex: number;
      lineType: number;
      sectorTag: number;
      frontSidedef: number;
      backSidedef: number;
    }[];
    things: {
      x: number;
      y: number;
      angle: number;
      type: number;
      options: number;
    }[];
  };
}

interface ManifestEntry {
  kind: "flat" | "wall";
  name: string;
  pngSha256: string;
  pngByteLength: number;
  width: number;
  height: number;
  tileScale: [number, number] | null;
}

interface Manifest {
  entries: ManifestEntry[];
  wadSha256: string;
  wadByteLength: number;
}

interface SpriteManifestFrame {
  id: number;
  name: string;
  family: string;
  uv: { min: [number, number]; max: [number, number] };
  pixelSize: [number, number];
  origin: [number, number];
  pivot: [number, number];
}

interface SpriteContractClip {
  id: string;
  loopMode: "once" | "repeat";
  steps: { frame: string; tics: number }[];
}

interface SpriteContractFamily {
  prefix: string;
  role: string;
  dimensionsDoomUnits?: { radius: number; height: number };
  directionalFrames: {
    frame: string;
    rotations: {
      rotation: number;
      sourceLump: string;
      mirrored: boolean;
    }[];
  }[];
  clips: SpriteContractClip[];
}

interface SpriteManifestAtlas {
  textureId: string;
  file: string;
  pngSha256: string;
  pngByteLength: number;
  frames: SpriteManifestFrame[];
}

interface SpriteManifest {
  wadSha256: string;
  wadByteLength: number;
  atlases: SpriteManifestAtlas[];
  contract: {
    tickRateHz: number;
    scale: {
      mapDoomUnitsPerEngineUnit: number;
      actorReferenceHeightDoomUnits: number;
      actorReferenceHeightEngineUnits: number;
      presentationDoomUnitsPerEngineUnit: number;
    };
    families: SpriteContractFamily[];
  };
}

interface AuthoredSpriteClip {
  frames: number[];
  ticksPerFrame: number;
  loopMode: "once" | "repeat";
  directionalViews: {
    rotation: number;
    frames: number[];
    mirrored: boolean;
    sourceOriginOffsets: [number, number][];
  }[];
}

function authoredSpriteClip(
  family: SpriteContractFamily,
  clipId: string,
  frameIds: ReadonlyMap<string, number>,
  frameByName: ReadonlyMap<string, SpriteManifestFrame>,
  presentationScale: number,
  tickRateHz: number,
): AuthoredSpriteClip {
  const clip = family.clips.find((candidate) => candidate.id === clipId);
  if (!clip) throw new Error(`missing Doom ${family.prefix} clip ${clipId}`);
  const directionalViews = Array.from({ length: 8 }, (_, index) => ({
    rotation: index + 1,
    frames: [] as number[],
    mirrored: false,
    sourceOriginOffsets: [] as [number, number][],
  }));
  let sourceTics = 0;
  let runtimeTicks = 0;
  for (const step of clip.steps) {
    const duration = (() => {
      if (step.tics < 0) return 1;
      sourceTics += step.tics;
      const nextRuntimeTicks = Math.round((sourceTics * 60) / tickRateHz);
      const ticks = Math.max(1, nextRuntimeTicks - runtimeTicks);
      runtimeTicks = nextRuntimeTicks;
      return ticks;
    })();
    const directions = family.directionalFrames.find(
      (candidate) => candidate.frame === step.frame,
    );
    for (const view of directionalViews) {
      const direction =
        directions?.rotations.find(
          (candidate) => candidate.rotation === view.rotation,
        ) ??
        directions?.rotations.find((candidate) => candidate.rotation === 0);
      if (!direction)
        throw new Error(
          `missing Doom ${family.prefix}${step.frame} rotation ${view.rotation}`,
        );
      const frame = frameIds.get(direction.sourceLump);
      const source = frameByName.get(direction.sourceLump);
      if (frame === undefined || !source)
        throw new Error(`missing authored Doom frame ${direction.sourceLump}`);
      const size: [number, number] = [
        source.pixelSize[0] / presentationScale,
        source.pixelSize[1] / presentationScale,
      ];
      const sourceOriginOffset: [number, number] = [
        (0.5 - source.pivot[0]) * size[0],
        (0.5 - source.pivot[1]) * size[1],
      ];
      for (let tick = 0; tick < duration; tick += 1) {
        view.frames.push(frame);
        view.sourceOriginOffsets.push(sourceOriginOffset);
      }
      if (direction.mirrored) view.mirrored = true;
    }
  }
  return {
    frames: directionalViews[0]!.frames,
    ticksPerFrame: 1,
    loopMode: clip.loopMode,
    directionalViews,
  };
}

function buildSectorEdges(inter: Intermediate) {
  const sectorSidedefs = new Map<number, number[]>();
  inter.level.sidedefs.forEach((sd, idx) => {
    if (sd.sector < 0 || sd.sector >= inter.level.sectors.length) return;
    const arr = sectorSidedefs.get(sd.sector) ?? [];
    arr.push(idx);
    sectorSidedefs.set(sd.sector, arr);
  });
  const sectorEdges = new Map<
    number,
    { x1: number; y1: number; x2: number; y2: number }[]
  >();
  for (let si = 0; si < inter.level.sectors.length; si += 1) {
    const sdi = sectorSidedefs.get(si) ?? [];
    const edges: { x1: number; y1: number; x2: number; y2: number }[] = [];
    for (const idx of sdi) {
      for (const ld of inter.level.linedefs) {
        if (ld.frontSidedef === idx || ld.backSidedef === idx) {
          const v1 = inter.level.vertices[ld.startVertex]!;
          const v2 = inter.level.vertices[ld.endVertex]!;
          edges.push({ x1: v1.x, y1: v1.y, x2: v2.x, y2: v2.y });
        }
      }
    }
    sectorEdges.set(si, edges);
  }
  return sectorEdges;
}

function isInside(
  px: number,
  py: number,
  si: number,
  sectorEdges: Map<
    number,
    { x1: number; y1: number; x2: number; y2: number }[]
  >,
): boolean {
  const edges = sectorEdges.get(si) ?? [];
  if (edges.length === 0) return false;
  let inside = false;
  for (const e of edges) {
    if (e.y1 > py !== e.y2 > py) {
      const xinters = ((e.x2 - e.x1) * (py - e.y1)) / (e.y2 - e.y1) + e.x1;
      if (px < xinters) inside = !inside;
    }
  }
  return inside;
}

function findSectorForPoint(
  x: number,
  y: number,
  inter: Intermediate,
  sectorEdges: Map<number, any>,
): number {
  for (let si = 0; si < inter.level.sectors.length; si += 1) {
    if (isInside(x, y, si, sectorEdges)) return si;
  }
  return -1;
}

export function buildDoomE1M1Project(
  intermediatePath = fileURLToPath(
    new URL(
      "../../../../content/doom-e1m1/e1m1.intermediate.json",
      import.meta.url,
    ),
  ),
  manifestPath = fileURLToPath(
    new URL(
      "../../../../content/doom-e1m1/textures/manifest.json",
      import.meta.url,
    ),
  ),
  spriteManifestPath = fileURLToPath(
    new URL(
      "../../../../content/doom-e1m1/sprites/manifest.json",
      import.meta.url,
    ),
  ),
  voxelPath = fileURLToPath(
    new URL(
      "../../../../content/doom-e1m1/doom-e1m1.voxel.json",
      import.meta.url,
    ),
  ),
  outPath = fileURLToPath(
    new URL(
      "../../../../content/projects/doom-e1m1.project.json",
      import.meta.url,
    ),
  ),
  loadingBayPath = fileURLToPath(
    new URL(
      "../../../../content/projects/loading-bay.project.json",
      import.meta.url,
    ),
  ),
): any {
  const inter: Intermediate = JSON.parse(
    readFileSync(intermediatePath, "utf8"),
  );
  const manifest: Manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const spriteManifest: SpriteManifest = JSON.parse(
    readFileSync(spriteManifestPath, "utf8"),
  );
  const presentationScale =
    spriteManifest.contract.scale.presentationDoomUnitsPerEngineUnit;
  if (presentationScale !== 28)
    throw new Error(`unexpected Doom presentation scale ${presentationScale}`);
  const spriteFrameByName = new Map(
    spriteManifest.atlases.flatMap((atlas) =>
      atlas.frames.map((frame) => [frame.name, frame] as const),
    ),
  );
  const voxel = JSON.parse(readFileSync(voxelPath, "utf8"));
  const loadingBay = JSON.parse(readFileSync(loadingBayPath, "utf8"));

  const sectorEdges = buildSectorEdges(inter);

  // Build material assets (54 materials + 54 textures =108) to match Rust helper
  const assets: any[] = [];
  // Include the required mesh assets and their authored materials from
  // loading-bay so Doom renderables resolve as a complete application-content
  // closure. We copy them verbatim to keep the one-way pin and avoid a second
  // catalog.
  const retainedMeshAssets = new Set([
    "mesh/player-marker",
    "mesh/prop-kit/breach-scattergun",
    "mesh/prop-kit/energy-cell",
    "mesh/prop-kit/hazard-marker",
    "mesh/prop-kit/impact-vest",
    "mesh/prop-kit/level-exit",
    "mesh/prop-kit/med-patch",
    "mesh/prop-kit/scatter-shells",
    "mesh/prop-kit/security-door",
  ]);
  for (const asset of loadingBay.assets as any[]) {
    if (
      typeof asset.id === "string" &&
      (retainedMeshAssets.has(asset.id) || asset.material !== undefined)
    ) {
      assets.push(asset);
    }
  }

  const spriteFrames = new Map<string, Map<string, number>>();
  const addSpriteAsset = (
    id: string,
    atlas: SpriteManifestAtlas,
    family: string,
  ) => {
    const selected = atlas.frames.filter(
      (frame) => frame.family.toLowerCase() === family,
    );
    if (selected.length === 0)
      throw new Error(`missing Doom sprite family ${family}`);
    const frameIds = new Map<string, number>();
    const frames = selected.map((frame, index) => {
      frameIds.set(frame.name, index);
      return {
        frame: index,
        uvMin: frame.uv.min,
        uvMax: frame.uv.max,
        size: [
          frame.pixelSize[0] / presentationScale,
          frame.pixelSize[1] / presentationScale,
        ],
      };
    });
    spriteFrames.set(family, frameIds);
    assets.push({
      id,
      catalog: {
        version: 1,
        hash: `sha256:${atlas.pngSha256}`,
        sourcePath: `content/doom-e1m1/sprites/${atlas.file}`,
        label: `Doom ${family.toUpperCase()} sprites`,
        dependencies: [],
      },
      spriteAtlas: { id, texture: atlas.textureId, frames },
    });
  };
  const actorAtlas = spriteManifest.atlases.find((atlas) =>
    atlas.frames.some((frame) => frame.family.toLowerCase() === "poss"),
  );
  const effectsAtlas = spriteManifest.atlases.find((atlas) =>
    atlas.frames.some((frame) => frame.family.toLowerCase() === "bal1"),
  );
  if (!actorAtlas || !effectsAtlas)
    throw new Error("Doom sprite atlases are incomplete");
  addSpriteAsset("sprite/doom-zombieman", actorAtlas, "poss");
  addSpriteAsset("sprite/doom-shotgun-guy", actorAtlas, "spos");
  addSpriteAsset("sprite/doom-imp", actorAtlas, "troo");
  addSpriteAsset("sprite/doom-imp-fireball", effectsAtlas, "bal1");
  addSpriteAsset("sprite/doom-blood", effectsAtlas, "blud");
  addSpriteAsset("sprite/doom-puff", effectsAtlas, "puff");
  const pickupSpriteSpecs = [
    ["shot", "sprite/doom-pickup-shot"],
    ["clip", "sprite/doom-pickup-clip"],
    ["shel", "sprite/doom-pickup-shel"],
    ["ammo", "sprite/doom-pickup-ammo"],
    ["sbox", "sprite/doom-pickup-sbox"],
    ["stim", "sprite/doom-pickup-stim"],
    ["medi", "sprite/doom-pickup-medi"],
    ["bon1", "sprite/doom-pickup-bon1"],
    ["bon2", "sprite/doom-pickup-bon2"],
    ["arm1", "sprite/doom-pickup-arm1"],
    ["arm2", "sprite/doom-pickup-arm2"],
  ] as const;
  for (const [family, id] of pickupSpriteSpecs) {
    const atlas = spriteManifest.atlases.find((candidate) =>
      candidate.frames.some((frame) => frame.family.toLowerCase() === family),
    );
    if (!atlas)
      throw new Error(`Doom ${family.toUpperCase()} pickup atlas is missing`);
    addSpriteAsset(id, atlas, family);
  }
  const weaponAtlas = spriteManifest.atlases.find((atlas) =>
    atlas.frames.some((frame) => frame.family.toLowerCase() === "pisg"),
  );
  if (!weaponAtlas)
    throw new Error("Doom player weapon sprite atlas is missing");
  addSpriteAsset("sprite/doom-fist-viewmodel", weaponAtlas, "pung");
  addSpriteAsset("sprite/doom-pistol-viewmodel", weaponAtlas, "pisg");
  addSpriteAsset("sprite/doom-pistol-flash-viewmodel", weaponAtlas, "pisf");
  addSpriteAsset("sprite/doom-shotgun-viewmodel", weaponAtlas, "shtg");
  addSpriteAsset("sprite/doom-shotgun-flash-viewmodel", weaponAtlas, "shtf");
  for (const entry of manifest.entries) {
    const k = kebab(entry.name);
    const matId = `material/doom-${entry.kind}-${k}`;
    const texId = `texture/doom-${entry.kind}-${k}`;
    const tileScale: [number, number] = entry.tileScale ?? [
      entry.width / SCALE,
      entry.height / SCALE,
    ];
    // texture asset
    assets.push({
      id: texId,
      catalog: {
        version: 1,
        hash: `sha256:${entry.pngSha256}`,
        sourcePath: `content/doom-e1m1/textures/${entry.kind}/${entry.name}.png`,
        label: entry.name,
        dependencies: [],
      },
    });
    // material asset
    assets.push({
      id: matId,
      catalog: {
        version: 1,
        hash: `sha256:${entry.pngSha256}`,
        sourcePath: `content/doom-e1m1/textures/${entry.kind}/${entry.name}.png`,
        label: entry.name,
        dependencies: [
          {
            id: texId,
            version: { req: "exact", value: 1 },
            hash: `sha256:${entry.pngSha256}`,
          },
        ],
      },
      material: {
        authority: {
          solid: true,
          collidable: true,
          occludes: true,
          structuralClass: "structural",
        },
        style: {
          color: [1, 1, 1, 1],
          texture: null,
          textureTint: [1, 1, 1, 1],
          emissionColor: [0, 0, 0, 1],
          roughness: 1,
          emissive: 0,
          uvStrategy: entry.kind === "flat" ? "flat" : "planar",
          voxelSurface: {
            schemaVersion: 1,
            mapping: {
              kind: "repeat",
              texture: {
                id: texId,
                version: { req: "exact", value: 1 },
                hash: `sha256:${entry.pngSha256}`,
              },
              tile_scale_cells: tileScale,
              tile_origin_cells: [0, 0],
            },
            alphaMode:
              entry.kind === "wall"
                ? { kind: "mask", cutoff: 0.5 }
                : { kind: "opaque" },
          },
        },
      },
    });
  }

  // Voxel volume asset
  assets.push({
    id: voxel.assetId,
    catalog: {
      version: 1,
      hash: voxel.contentHash,
      sourcePath: "content/doom-e1m1/doom-e1m1.voxel.json",
      label: "doom-e1m1",
      dependencies: [],
    },
    voxelVolume: voxel,
  });

  // E1M1's single-player item vocabulary is authored ONCE in
  // gameplay/authoring/src/catalogs/items.ts, materialized into the committed
  // gameplay-rules package artifact (data/gameplay/), and admitted here from
  // that artifact. Rust owns the reusable pickup, vitality, and firing state
  // machines; these values are authored calibration. The project document
  // carries the admitted definitions for entity placement; the package is the
  // authored source of truth, and gameplay-package-check proves parity.
  const gameplayPackagePath = fileURLToPath(
    new URL(
      "../../../../data/gameplay/loading-bay-e1m1-core.package.json",
      import.meta.url,
    ),
  );
  const gameplayPackage: {
    kind?: unknown;
    domain?: unknown;
    package?: unknown;
    payload?: { schemaVersion?: unknown; items?: unknown };
  } = JSON.parse(readFileSync(gameplayPackagePath, "utf8"));
  if (
    gameplayPackage.kind !== "rusty.gameplay-rules.package" ||
    gameplayPackage.domain !== "loading-bay" ||
    gameplayPackage.package !== "e1m1-core" ||
    gameplayPackage.payload?.schemaVersion !== 1 ||
    !Array.isArray(gameplayPackage.payload?.items)
  ) {
    throw new Error(
      `unexpected gameplay package at ${gameplayPackagePath}: run \`pnpm gameplay:build\` to materialize the authored catalog`,
    );
  }
  const itemDefinitions = gameplayPackage.payload.items;

  // Entities
  const entities: any[] = [];
  let nextId = 1;

  const doomToWorldForThing = (thing: { x: number; y: number }) => {
    const si = findSectorForPoint(thing.x, thing.y, inter, sectorEdges);
    const floorHeight = si >= 0 ? inter.level.sectors[si]!.floorHeight : 0;
    return wadToWorld(thing.x, thing.y, floorHeight);
  };

  // Player — first type 1
  const playerThing = inter.level.things.find((t) => t.type === 1)!;
  const playerPos = doomToWorldForThing(playerThing);
  // `wadToWorld` identifies the center of the admitted floor voxel. Place the
  // kinematic body one half-cell higher so its lower face does not begin
  // overlapped with that voxel and axis sweeps remain playable.
  playerPos[1] += 0.5;
  // Doom 0 degrees points +X and increases toward +Y. The Engine camera uses
  // yaw 0 toward -Z, while the forge maps Doom +Y to world +Z.
  const playerYaw = (((270 - playerThing.angle) % 360) + 360) % 360;
  entities.push({
    id: nextId++,
    name: "player",
    translation: playerPos,
    bounds: { min: [-0.25, -0.25, -0.25], max: [0.25, 0.25, 0.25] },
    collision: { enabled: true, staticCollider: false },
    renderable: { asset: "mesh/player-marker", visible: true },
    health: {
      max: 200,
      startingHealth: 100,
      hitboxHalfExtents: [0.25, 0.5, 0.25],
      maxArmor: 200,
      armorAbsorptionPercent: 33,
    },
    kinematic: { halfExtents: [0.25, 0.25, 0.25], velocity: [0, 0, 0] },
    playerController: {
      moveSpeedUnitsPerSecond: 6,
      moveStepSeconds: 0.1,
      lookDegreesPerUnit: 12,
      initialYawDegrees: playerYaw,
      initialPitchDegrees: -6,
      traversal: {
        maxStepHeight: 1.5,
        gravityUnitsPerSecondSquared: 24,
        jumpImpulseUnitsPerSecond: 8,
        groundProbeDistance: 0.3,
        eyeHeight: 2.0625,
        manualJumpEnabled: true,
        maxAirJumps: 0,
      },
      bindings: {
        moveForward: "KeyW",
        moveBackward: "KeyS",
        moveLeft: "KeyA",
        moveRight: "KeyD",
        mouseLook: "pointer",
        primaryFire: "Mouse0",
        jump: "Space",
        selectWeapon: ["Digit1", "Digit2", "Digit3"],
      },
    },
    inventory: {
      capacitySlots: 10,
      startingStacks: [
        { item: "weapon/fist", quantity: 1 },
        { item: "weapon/pistol", quantity: 1 },
        { item: "ammo/bullets", quantity: 50 },
      ],
      initiallyEquippedWeapon: "weapon/pistol",
      weaponSlots: ["weapon/pistol", "weapon/shotgun", "weapon/fist"],
    },
  });

  const enemyArchetypes = {
    9: {
      name: "shotgun-guy",
      spriteFamily: "spos",
      health: 30,
      painDurationTicks: Math.round((6 / 35) * 60),
      mesh: "sprite/doom-shotgun-guy",
      attack: {
        kind: "rangedHitscan",
        // The runtime hitscan is authoritative and cannot reproduce Doom's
        // pellet spread/misses yet. Keep one guaranteed blast dangerous
        // without treating the original average pellet total as unavoidable.
        damage: 12,
        range: 128,
        cooldownTicks: Math.round((30 / 35) * 60),
        originOffset: [0, 0.25, 0],
        presentation: "doom-shotgun-guy-blast",
      },
      drop: {
        item: "weapon/shotgun",
        quantity: 1,
        spriteFamily: "shot",
        asset: "sprite/doom-pickup-shot",
        starterAmmunition: { item: "ammo/shells", quantity: 4 },
      },
    },
    3001: {
      name: "imp",
      spriteFamily: "troo",
      health: 60,
      painDurationTicks: Math.round((4 / 35) * 60),
      mesh: "sprite/doom-imp",
      attack: {
        kind: "projectile",
        damage: 12,
        range: 128,
        cooldownTicks: Math.round((22 / 35) * 60),
        originOffset: [0, 0.25, 0],
        presentation: "doom-imp-fireball",
        projectile: {
          mass: 0.2,
          radius: 0.375,
          impulse: 4.375,
          gravityScale: 0,
          lifetimeTicks: 360,
          restitution: 0,
          visualAsset: "sprite/doom-imp-fireball",
        },
      },
      drop: null,
    },
    3004: {
      name: "zombieman",
      spriteFamily: "poss",
      health: 20,
      painDurationTicks: Math.round((6 / 35) * 60),
      mesh: "sprite/doom-zombieman",
      attack: {
        kind: "rangedHitscan",
        damage: 9,
        range: 128,
        cooldownTicks: Math.round((26 / 35) * 60),
        originOffset: [0, 0.25, 0],
        presentation: "doom-zombieman-shot",
      },
      drop: {
        item: "ammo/bullets",
        quantity: 5,
        spriteFamily: "clip",
        asset: "sprite/doom-pickup-clip",
      },
    },
  } as const;
  const pickupMap: Record<
    number,
    {
      item: string;
      quantity: number;
      spriteFamily: string;
      asset: string;
      starterAmmunition?: { item: string; quantity: number };
    }
  > = {
    2001: {
      item: "weapon/shotgun",
      quantity: 1,
      spriteFamily: "shot",
      asset: "sprite/doom-pickup-shot",
      starterAmmunition: { item: "ammo/shells", quantity: 8 },
    },
    2007: {
      item: "ammo/bullets",
      quantity: 10,
      spriteFamily: "clip",
      asset: "sprite/doom-pickup-clip",
    },
    2008: {
      item: "ammo/shells",
      quantity: 4,
      spriteFamily: "shel",
      asset: "sprite/doom-pickup-shel",
    },
    2048: {
      item: "ammo/bullets",
      quantity: 50,
      spriteFamily: "ammo",
      asset: "sprite/doom-pickup-ammo",
    },
    2049: {
      item: "ammo/shells",
      quantity: 20,
      spriteFamily: "sbox",
      asset: "sprite/doom-pickup-sbox",
    },
    2011: {
      item: "supply/stimpack",
      quantity: 1,
      spriteFamily: "stim",
      asset: "sprite/doom-pickup-stim",
    },
    2012: {
      item: "supply/medikit",
      quantity: 1,
      spriteFamily: "medi",
      asset: "sprite/doom-pickup-medi",
    },
    2014: {
      item: "supply/health-bonus",
      quantity: 1,
      spriteFamily: "bon1",
      asset: "sprite/doom-pickup-bon1",
    },
    2015: {
      item: "armor/bonus",
      quantity: 1,
      spriteFamily: "bon2",
      asset: "sprite/doom-pickup-bon2",
    },
    2018: {
      item: "armor/green",
      quantity: 1,
      spriteFamily: "arm1",
      asset: "sprite/doom-pickup-arm1",
    },
    2019: {
      item: "armor/blue",
      quantity: 1,
      spriteFamily: "arm2",
      asset: "sprite/doom-pickup-arm2",
    },
  };

  const spriteClip = (familyName: string, clipId: string) => {
    const family = spriteManifest.contract.families.find(
      (candidate) => candidate.prefix.toLowerCase() === familyName,
    );
    const frames = spriteFrames.get(familyName);
    if (!family || !frames)
      throw new Error(`missing Doom sprite contract ${familyName}`);
    return authoredSpriteClip(
      family,
      clipId,
      frames,
      spriteFrameByName,
      presentationScale,
      spriteManifest.contract.tickRateHz,
    );
  };
  const spriteState = (state: string, familyName: string, clipId: string) => ({
    state,
    kind: "spriteFrames",
    ...spriteClip(familyName, clipId),
  });
  const actorVisualBinding = (familyName: string) => ({
    version: 2,
    states: [
      spriteState("idle", familyName, "idle"),
      spriteState("moving", familyName, "walk"),
      spriteState("alert", familyName, "idle"),
      spriteState("attacking", familyName, "attack"),
      spriteState("hit", familyName, "pain"),
      spriteState("defeated", familyName, "death"),
    ],
  });
  const pickupVisualBinding = (familyName: string) => {
    const state = (name: string) => spriteState(name, familyName, "available");
    return {
      version: 2,
      states: [state("dormant"), state("available"), state("collected")],
    };
  };
  const pickupPresentation = (mapping: {
    spriteFamily: string;
    asset: string;
  }) => {
    const family = spriteManifest.contract.families.find(
      (candidate) => candidate.prefix.toLowerCase() === mapping.spriteFamily,
    );
    if (family?.role !== "item" || family.dimensionsDoomUnits?.radius !== 20) {
      throw new Error(
        `unexpected ${mapping.spriteFamily.toUpperCase()} pickup contract`,
      );
    }
    const halfExtent = family.dimensionsDoomUnits.radius / presentationScale;
    return {
      bounds: {
        min: [-halfExtent, -0.35, -halfExtent],
        max: [halfExtent, 0.65, halfExtent],
      },
      renderable: {
        asset: mapping.asset,
        visible: true,
        localTransform: {
          translation: [0, 0, 0],
          rotation: [0, 0, 0, 1],
          scale: [1, 1, 1],
        },
      },
    };
  };
  const effectTemplate = (
    name: string,
    spriteFamily: string,
    clipId: string,
    asset: string,
  ) => ({
    id: nextId++,
    name,
    translation: [0, 0, 0],
    renderable: {
      asset,
      visible: false,
      localTransform: {
        translation: [0, 0, 0],
        rotation: [0, 0, 0, 1],
        scale: [1, 1, 1],
      },
      visualBinding: {
        version: 2,
        states: [spriteState("default", spriteFamily, clipId)],
      },
    },
  });
  const visualTemplate = (
    name: string,
    asset: string,
    visualBinding: ReturnType<typeof actorVisualBinding>,
  ) => ({
    id: nextId++,
    name,
    translation: [0, 0, 0],
    renderable: {
      asset,
      visible: false,
      localTransform: {
        translation: [0, 0, 0],
        rotation: [0, 0, 0, 1],
        scale: [1, 1, 1],
      },
      visualBinding,
    },
  });

  // E1M1 UV enemies. The archetype records above are immutable authored
  // calibration; every placement resolves to ordinary reusable Rust owners.
  const encounterMembers = new Map<string, number[]>();
  const encounterPositions = new Map<string, [number, number, number][]>();
  const encounterNameFor = (thing: { x: number; y: number }): string => {
    if (thing.y <= -3900) return "exit-annex";
    if (thing.x < 1000) return "west-court";
    if (thing.x < 2600) return "central-stairs";
    return "east-lift";
  };
  const sourceYaw = (angle: number): [number, number, number, number] => {
    const radians = (-angle * Math.PI) / 180;
    return [0, Math.sin(radians / 2), 0, Math.cos(radians / 2)];
  };
  let enemyIndex = 0;
  for (const thing of inter.level.things) {
    const archetype =
      enemyArchetypes[thing.type as keyof typeof enemyArchetypes];
    if (!archetype || (thing.options & 4) === 0 || (thing.options & 16) !== 0) {
      continue;
    }
    enemyIndex += 1;
    const pos = doomToWorldForThing(thing);
    pos[1] += 1.25;
    const id = nextId++;
    const enemy: any = {
      id,
      name: `doom-${archetype.name}-${enemyIndex}`,
      translation: pos,
      rotation: sourceYaw(thing.angle),
      collision: { enabled: true, staticCollider: false },
      renderable: {
        asset: archetype.mesh,
        visible: true,
        localTransform: {
          translation: [0, -1, 0],
          rotation: [0, 0, 0, 1],
          scale: [1, 1, 1],
        },
      },
      enemy: true,
      enemyCombat: {
        sightRange: 64,
        hearingRange: (thing.options & 8) !== 0 ? 0 : 12,
        painDurationTicks: archetype.painDurationTicks,
        attack: archetype.attack,
      },
      health: {
        max: archetype.health,
        hitboxHalfExtents: [0.45, 1, 0.45],
      },
      kinematic: { halfExtents: [0.45, 1, 0.45], velocity: [0, 0, 0] },
      navigation: {
        goal: pos,
        speedUnitsPerSecond: 3,
        maxVisited: 64,
      },
    };
    if (archetype.drop !== null) {
      const dropId = nextId++;
      enemy.defeatDrop = { pickup: dropId };
      const dropPresentation = pickupPresentation(archetype.drop);
      dropPresentation.renderable.visible = false;
      dropPresentation.renderable.localTransform.translation = [0, -1, 0];
      entities.push(enemy, {
        id: dropId,
        name: `doom-drop-${archetype.name}-${enemyIndex}`,
        translation: pos,
        ...dropPresentation,
        pickup: {
          item: archetype.drop.item,
          quantity: archetype.drop.quantity,
          ...("starterAmmunition" in archetype.drop
            ? { starterAmmunition: archetype.drop.starterAmmunition }
            : {}),
        },
      });
    } else {
      entities.push(enemy);
    }
    const encounterName = encounterNameFor(thing);
    const members = encounterMembers.get(encounterName) ?? [];
    members.push(id);
    encounterMembers.set(encounterName, members);
    const positions = encounterPositions.get(encounterName) ?? [];
    positions.push(pos);
    encounterPositions.set(encounterName, positions);
  }
  for (const [name, members] of encounterMembers) {
    const positions = encounterPositions.get(name)!;
    const translation: [number, number, number] = [0, 0, 0];
    for (const position of positions) {
      translation[0] += position[0] / positions.length;
      translation[1] += position[1] / positions.length;
      translation[2] += position[2] / positions.length;
    }
    entities.push({
      id: nextId++,
      name: `doom-encounter-${name}`,
      translation,
      encounter: {
        members,
        activationRadius: name === "central-stairs" ? 24 : 18,
      },
    });
  }

  // The six E1M1 type-2035 barrels remain ordinary damageable world objects.
  // Rust's named explosive-prop owner supplies their live chain consequences.
  let barrelIndex = 0;
  for (const thing of inter.level.things) {
    if (thing.type !== 2035 || (thing.options & 4) === 0) continue;
    barrelIndex += 1;
    const pos = doomToWorldForThing(thing);
    pos[1] += 0.8125;
    entities.push({
      id: nextId++,
      name: `doom-explosive-barrel-${barrelIndex}`,
      translation: pos,
      rotation: sourceYaw(thing.angle),
      collision: { enabled: true, staticCollider: false },
      renderable: {
        asset: "mesh/prop-kit/hazard-marker",
        visible: true,
      },
      health: { max: 20, hitboxHalfExtents: [0.625, 1.3125, 0.625] },
      kinematic: {
        halfExtents: [0.625, 1.3125, 0.625],
        velocity: [0, 0, 0],
      },
      explosiveProp: {
        damage: 128,
        radius: 8,
      },
    });
  }

  // Pickups
  let pickupIndex = 0;
  for (const thing of inter.level.things) {
    const mapping = pickupMap[thing.type];
    if (!mapping || (thing.options & 16) !== 0) continue;
    pickupIndex += 1;
    const pos = doomToWorldForThing(thing);
    const id = nextId++;
    entities.push({
      id,
      name: `doom-pickup-${thing.type}-${pickupIndex}`,
      translation: pos,
      ...pickupPresentation(mapping),
      pickup: {
        item: mapping.item,
        quantity: mapping.quantity,
        ...(mapping.starterAmmunition
          ? { starterAmmunition: mapping.starterAmmunition }
          : {}),
      },
    });
  }

  // One validated binding per sprite family keeps canonical E1M1 below the
  // project-store admission bound. The Rust application projector resolves
  // these game-specific templates by asset for live actors and pickups.
  entities.push(
    visualTemplate(
      "doom-visual-template-zombieman",
      "sprite/doom-zombieman",
      actorVisualBinding("poss"),
    ),
    visualTemplate(
      "doom-visual-template-shotgun-guy",
      "sprite/doom-shotgun-guy",
      actorVisualBinding("spos"),
    ),
    visualTemplate(
      "doom-visual-template-imp",
      "sprite/doom-imp",
      actorVisualBinding("troo"),
    ),
    ...pickupSpriteSpecs.map(([family, asset]) =>
      visualTemplate(
        `doom-visual-template-pickup-${family}`,
        asset,
        pickupVisualBinding(family),
      ),
    ),
  );

  // Runtime combat effects resolve their exact validated sprite clips from
  // these invisible application-content templates.
  entities.push(
    effectTemplate(
      "doom-fx-template-blood",
      "blud",
      "hit",
      "sprite/doom-blood",
    ),
    effectTemplate(
      "doom-fx-template-bullet-puff",
      "puff",
      "impact",
      "sprite/doom-puff",
    ),
    effectTemplate(
      "doom-fx-template-projectile-flight",
      "bal1",
      "flight",
      "sprite/doom-imp-fireball",
    ),
    effectTemplate(
      "doom-fx-template-projectile-impact",
      "bal1",
      "impact",
      "sprite/doom-imp-fireball",
    ),
  );

  // Special-7 nukage sectors use the reusable bounded hazard owner. The map
  // supplies region geometry and source timing; Rust owns overlap and damage.
  const damagingSectorIndices = inter.level.sectors
    .map((sector, index) => (sector.special === 7 ? index : -1))
    .filter((index) => index >= 0);
  for (const sectorIndex of damagingSectorIndices) {
    const edges = sectorEdges.get(sectorIndex) ?? [];
    if (edges.length === 0) continue;
    const xs = edges.flatMap((edge) => [edge.x1, edge.x2]);
    const ys = edges.flatMap((edge) => [edge.y1, edge.y2]);
    const minX = (Math.min(...xs) - MIN_X) / SCALE;
    const maxX = (Math.max(...xs) - MIN_X) / SCALE;
    const minZ = (Math.min(...ys) - MIN_Y) / SCALE;
    const maxZ = (Math.max(...ys) - MIN_Y) / SCALE;
    const sector = inter.level.sectors[sectorIndex]!;
    const center = wadToWorld(
      (Math.min(...xs) + Math.max(...xs)) / 2,
      (Math.min(...ys) + Math.max(...ys)) / 2,
      sector.floorHeight,
    );
    const halfX = Math.max(0.05, (maxX - minX) / 2 - 0.05);
    const halfZ = Math.max(0.05, (maxZ - minZ) / 2 - 0.05);
    entities.push({
      id: nextId++,
      name: `doom-damaging-sector-${sectorIndex}`,
      translation: center,
      bounds: { min: [-halfX, -0.6, -halfZ], max: [halfX, 0.6, halfZ] },
      renderable: { asset: "mesh/player-marker", visible: false },
      hazard: {
        damage: 5,
        cooldownTicks: 55,
      },
    });
  }

  // Four repeatable manual doors, grouped by their shared back-sidedef sector.
  // The source identities are data, not runtime line-number behavior.
  const manualDoors = [
    { sector: 4, linedefs: [151, 152], texture: "BIGDOOR2" },
    { sector: 68, linedefs: [247, 248], texture: "BROWN96" },
    { sector: 81, linedefs: [324, 325], texture: "EXITDOOR" },
    { sector: 76, linedefs: [340, 341], texture: "BIGDOOR4" },
  ] as const;
  for (const authoredDoor of manualDoors) {
    const edges = sectorEdges.get(authoredDoor.sector) ?? [];
    const xs = edges.flatMap((edge) => [edge.x1, edge.x2]);
    const ys = edges.flatMap((edge) => [edge.y1, edge.y2]);
    const mx = (Math.min(...xs) + Math.max(...xs)) / 2;
    const my = (Math.min(...ys) + Math.max(...ys)) / 2;
    const sector = inter.level.sectors[authoredDoor.sector]!;
    const neighborCeilings = authoredDoor.linedefs.flatMap((linedefIndex) => {
      const linedef = inter.level.linedefs[linedefIndex]!;
      return [linedef.frontSidedef, linedef.backSidedef]
        .filter((sidedefIndex) => sidedefIndex >= 0)
        .map((sidedefIndex) => inter.level.sidedefs[sidedefIndex]!.sector)
        .filter((sectorIndex) => sectorIndex !== authoredDoor.sector)
        .map((sectorIndex) => inter.level.sectors[sectorIndex]!.ceilingHeight);
    });
    const destination = Math.min(...neighborCeilings) - 4;
    const pos = wadToWorld(mx, my, sector.floorHeight);
    const openPos: [number, number, number] = [
      pos[0],
      pos[1] + Math.max(0, destination - sector.ceilingHeight) / SCALE,
      pos[2],
    ];
    const id = nextId++;
    entities.push({
      id,
      name: `doom-manual-door-sector-${authoredDoor.sector}`,
      translation: pos,
      bounds: { min: [-0.8, -1.2, -0.2], max: [0.8, 1.2, 0.2] },
      collision: { enabled: true, staticCollider: false },
      renderable: {
        asset: "mesh/prop-kit/security-door",
        visible: true,
        visualBinding: {
          version: 1,
          states: [
            {
              state: "closed",
              kind: "material",
              textureTint: [1, 0.78, 0.48, 1],
              emissionColor: [0.75, 0.28, 0.05],
              emissionIntensity: 0.12,
            },
            {
              state: "opening",
              kind: "material",
              textureTint: [1, 0.88, 0.58, 1],
              emissionColor: [0.75, 0.42, 0.08],
              emissionIntensity: 0.18,
            },
            {
              state: "open",
              kind: "material",
              textureTint: [0.62, 1, 0.82, 1],
              emissionColor: [0.12, 0.82, 0.52],
              emissionIntensity: 0.35,
            },
            {
              state: "closing",
              kind: "material",
              textureTint: [1, 0.88, 0.58, 1],
              emissionColor: [0.75, 0.42, 0.08],
              emissionIntensity: 0.18,
            },
          ],
        },
      },
      door: {
        openTranslation: openPos,
        // Doom's 150-tic wait converted once to this product's 60 Hz clock.
        autoCloseAfterTicks: Math.round((150 / 35) * 60),
        motionDurationTicks: Math.max(
          1,
          Math.round(((destination - sector.ceilingHeight) / 70) * 60),
        ),
        source: `doom1.wad:E1M1:linedefs:${authoredDoor.linedefs.join(",")}:type:1:sector:${authoredDoor.sector}:texture:${authoredDoor.texture}`,
        openPresentation: `${authoredDoor.texture} door opening`,
        closePresentation: `${authoredDoor.texture} door closing`,
        openSound: "doom:DSDOROPN",
        closeSound: "doom:DSDORCLS",
      },
      switch: {
        controls: [],
        activationRadius: 6,
        prompt: `Open ${authoredDoor.texture} door`,
        unavailablePresentation: `${authoredDoor.texture} door unavailable`,
        effects: [{ kind: "openDoor", door: id }],
      },
      kinematic: { halfExtents: [0.8, 1.2, 0.2], velocity: [0, 0, 0] },
    });
  }

  const sectorPlatform = (
    sectorIndex: number,
    name: string,
    surfaceY: number,
  ): { id: number; raised: [number, number, number] } => {
    const edges = sectorEdges.get(sectorIndex) ?? [];
    const xs = edges.flatMap((edge) => [edge.x1, edge.x2]);
    const ys = edges.flatMap((edge) => [edge.y1, edge.y2]);
    const minX = (Math.min(...xs) - MIN_X) / SCALE;
    const maxX = (Math.max(...xs) - MIN_X) / SCALE;
    const minZ = (Math.min(...ys) - MIN_Y) / SCALE;
    const maxZ = (Math.max(...ys) - MIN_Y) / SCALE;
    const raised: [number, number, number] = [
      (minX + maxX) / 2,
      surfaceY,
      (minZ + maxZ) / 2,
    ];
    const halfX = (maxX - minX) / 2;
    const halfZ = (maxZ - minZ) / 2;
    const id = nextId++;
    entities.push({
      id,
      name,
      translation: raised,
      bounds: { min: [-halfX, -1, -halfZ], max: [halfX, 0, halfZ] },
      collision: { enabled: true, staticCollider: false },
      // The entity translation is the authored floor surface. Keep the dynamic
      // collider on that surface instead of centering a half-unit box above it,
      // which would overlap a rider whose feet are correctly at surfaceY.
      kinematic: { halfExtents: [halfX, 0.001, halfZ], velocity: [0, 0, 0] },
    });
    return { id, raised };
  };

  const floorPlatform = sectorPlatform(59, "doom-floor-platform-sector-59", 15);
  const floorTriggerLine = inter.level.linedefs[308]!;
  const floorTriggerStart = inter.level.vertices[floorTriggerLine.startVertex]!;
  const floorTriggerEnd = inter.level.vertices[floorTriggerLine.endVertex]!;
  entities.push({
    id: nextId++,
    name: "doom-walk-floor-action-linedef-308",
    translation: wadToWorld(
      (floorTriggerStart.x + floorTriggerEnd.x) / 2,
      (floorTriggerStart.y + floorTriggerEnd.y) / 2,
      inter.level.sectors[73]!.floorHeight,
    ),
    bounds: { min: [-6, -1, -0.5], max: [6, 3, 0.5] },
    floorAction: {
      targetPlatform: floorPlatform.id,
      upperTranslation: floorPlatform.raised,
      loweredTranslation: [
        floorPlatform.raised[0],
        6.5,
        floorPlatform.raised[2],
      ],
      motionDurationTicks: 59,
      prompt: "Lower turbo floor",
      presentation: "Sector 59 lowering",
      source: "doom1.wad:E1M1:linedef:308:type:36:tag:1:sector:59:sound:stnmov",
    },
  });

  const liftPlatform = sectorPlatform(70, "doom-lift-platform-sector-70", 15.5);
  const liftTriggerLine = inter.level.linedefs[195]!;
  const liftTriggerStart = inter.level.vertices[liftTriggerLine.startVertex]!;
  const liftTriggerEnd = inter.level.vertices[liftTriggerLine.endVertex]!;
  entities.push({
    id: nextId++,
    name: "doom-repeatable-lift-linedef-195",
    translation: wadToWorld(
      (liftTriggerStart.x + liftTriggerEnd.x) / 2,
      (liftTriggerStart.y + liftTriggerEnd.y) / 2,
      inter.level.sectors[60]!.floorHeight,
    ),
    bounds: { min: [-10, -1, -6], max: [10, 3, 6] },
    lift: {
      targetPlatform: liftPlatform.id,
      raisedTranslation: liftPlatform.raised,
      loweredTranslation: [liftPlatform.raised[0], 6, liftPlatform.raised[2]],
      motionDurationTicks: 65,
      loweredWaitTicks: 180,
      prompt: "Activate secret lift",
      presentation: "Sector 70 lift cycle",
      source:
        "doom1.wad:E1M1:linedef:195:type:88:tag:2:sector:70:sounds:pstart,pstop",
    },
  });

  // Exit — near the exit linedef type 11 midpoint
  const exitLd = inter.level.linedefs.find((ld) => ld.lineType === 11);
  let exitPos: [number, number, number] = [140, 1.5, 120];
  if (exitLd) {
    const v1 = inter.level.vertices[exitLd.startVertex]!;
    const v2 = inter.level.vertices[exitLd.endVertex]!;
    const mx = (v1.x + v2.x) / 2;
    const my = (v1.y + v2.y) / 2;
    const si = findSectorForPoint(mx, my, inter, sectorEdges);
    const fl = si >= 0 ? inter.level.sectors[si]!.floorHeight : 0;
    exitPos = wadToWorld(mx, my, fl);
  }
  entities.push({
    id: nextId++,
    name: "doom-exit",
    translation: exitPos,
    renderable: {
      asset: "mesh/prop-kit/level-exit",
      visible: true,
      visualBinding: {
        version: 1,
        states: [
          {
            state: "available",
            kind: "material",
            textureTint: [1, 0.78, 0.48, 1],
            emissionColor: [0.75, 0.28, 0.05],
            emissionIntensity: 0.12,
          },
          {
            state: "completed",
            kind: "material",
            textureTint: [0.62, 1, 0.82, 1],
            emissionColor: [0.12, 0.82, 0.52],
            emissionIntensity: 0.35,
          },
        ],
      },
    },
    levelExit: {
      activationRadius: 4,
      presentation: "Doom E1M1 complete",
      source: "doom1.wad:E1M1:linedef:330:type:11:texture:SW1STRTN",
    },
  });

  // Every authored E1M1 secret sector is independently discoverable once.
  const secretSectorIndices = inter.level.sectors
    .map((sector, index) => (sector.special === 9 ? index : -1))
    .filter((index) => index >= 0);
  for (const [secretIndex, secretSectorIdx] of secretSectorIndices.entries()) {
    const edges = sectorEdges.get(secretSectorIdx) ?? [];
    if (edges.length > 0) {
      const xs = edges.flatMap((e) => [e.x1, e.x2]);
      const ys = edges.flatMap((e) => [e.y1, e.y2]);
      const cx = (Math.min(...xs) + Math.max(...xs)) / 2;
      const cy = (Math.min(...ys) + Math.max(...ys)) / 2;
      const sec = inter.level.sectors[secretSectorIdx]!;
      const pos = wadToWorld(cx, cy, sec.floorHeight);
      entities.push({
        id: nextId++,
        name: `doom-secret-sector-${secretSectorIdx}`,
        translation: pos,
        bounds: { min: [-1.5, -0.8, -1.5], max: [1.5, 0.8, 1.5] },
        secretRegion: {
          presentation: `Secret ${secretIndex + 1} discovered`,
          source: `doom1.wad:E1M1:sector:${secretSectorIdx}:special:9`,
        },
      });
    }
  }

  // Sort assets deterministic for canonical encoding
  assets.sort((a, b) => a.id.localeCompare(b.id));
  entities.sort((a, b) => a.id - b.id);

  const project = {
    schemaVersion: 25,
    projectId: "doom-e1m1",
    name: "Doom E1M1 — Hangar (VoXel Showcase)",
    entryScene: "scene/doom-e1m1",
    assets,
    itemDefinitions,
    scenes: [
      {
        id: "scene/doom-e1m1",
        name: "Hangar",
        voxelEnvironment: {
          kind: "material",
          voxelSize: 1,
          chunkSize: 16,
          materialVoxels: [],
          voxelAssets: ["voxel-volume/doom-e1m1"],
        },
        voxelInstances: [
          {
            instanceId: "doom-e1m1-volume",
            voxelAssetId: "voxel-volume/doom-e1m1",
            translation: [0, 0, 0],
            rotation: [0, 0, 0, 1],
            scale: [1, 1, 1],
          },
        ],
        voxelObjectInstances: [],
        entities,
      },
    ],
  };

  mkdirSync(resolve(outPath, ".."), { recursive: true });
  // Keep the pre-canonical admission payload compact. Directional sprite
  // contracts are intentionally data-rich and the Rust loader enforces an
  // 8 MiB input safety bound before it rewrites canonical bytes.
  writeFileSync(outPath, `${JSON.stringify(project)}\n`, "utf8");

  // Canonicalize via Rust project-store to guarantee byte equality
  const canonicalOut = `${outPath}.canon`;
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
      outPath,
      "--output",
      canonicalOut,
    ],
    { encoding: "utf8" },
  );
  if (result.status !== 0) {
    console.error(result.stderr, result.stdout);
    throw new Error(`project-store canonicalize failed for ${outPath}`);
  }
  const canonBytes = readFileSync(canonicalOut, "utf8");
  writeFileSync(outPath, canonBytes, "utf8");
  try {
    unlinkSync(canonicalOut);
  } catch {}
  console.log(
    `Wrote ${outPath} entities=${entities.length} assets=${assets.length} bytes=${canonBytes.length}`,
  );
  return project;
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  if (process.argv.includes("--check")) {
    const canonicalPath = fileURLToPath(
      new URL(
        "../../../../content/projects/doom-e1m1.project.json",
        import.meta.url,
      ),
    );
    const candidatePath = `${canonicalPath}.check`;
    try {
      buildDoomE1M1Project(
        undefined,
        undefined,
        undefined,
        undefined,
        candidatePath,
      );
      const expected = readFileSync(canonicalPath);
      const candidate = readFileSync(candidatePath);
      if (!expected.equals(candidate)) {
        throw new Error(
          "canonical Doom E1M1 project is stale; run the authoring generate command",
        );
      }
      console.log("Canonical Doom E1M1 composition is current");
    } finally {
      try {
        unlinkSync(candidatePath);
      } catch {}
      try {
        unlinkSync(`${candidatePath}.canon`);
      } catch {}
    }
  } else {
    buildDoomE1M1Project();
  }
}
