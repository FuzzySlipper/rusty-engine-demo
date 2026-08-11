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
  const voxel = JSON.parse(readFileSync(voxelPath, "utf8"));
  const loadingBay = JSON.parse(readFileSync(loadingBayPath, "utf8"));

  const sectorEdges = buildSectorEdges(inter);

  // Build material assets (54 materials + 54 textures =108) to match Rust helper
  const assets: any[] = [];
  // Include required mesh assets from loading-bay (player marker, prop-kit, animated meshes) so
  // Doom renderables resolve. We copy them verbatim to keep the one-way pin and avoid a second catalog.
  for (const asset of loadingBay.assets as any[]) {
    if (typeof asset.id === "string" && asset.id.startsWith("mesh")) {
      assets.push(asset);
    }
  }
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

  // E1M1's single-player item vocabulary. Rust owns the reusable pickup,
  // vitality, and firing state machines; these values are authored calibration.
  const itemDefinitions = [
    { id: "ammo/bullets", maxQuantity: 200, kind: { kind: "ammunition" } },
    { id: "ammo/shells", maxQuantity: 50, kind: { kind: "ammunition" } },
    {
      id: "armor/bonus",
      maxQuantity: 1,
      kind: {
        kind: "armor",
        protection: 1,
        maximumArmor: 200,
        absorptionDivisor: 3,
        grantMode: "add",
        transition: "preserve",
        consumeAtCap: true,
      },
    },
    {
      id: "armor/green",
      maxQuantity: 1,
      kind: {
        kind: "armor",
        protection: 100,
        maximumArmor: 200,
        absorptionDivisor: 3,
        grantMode: "setMinimum",
        transition: "replace",
      },
    },
    {
      id: "armor/blue",
      maxQuantity: 1,
      kind: {
        kind: "armor",
        protection: 200,
        maximumArmor: 200,
        absorptionDivisor: 2,
        grantMode: "setMinimum",
        transition: "replace",
      },
    },
    {
      id: "supply/stimpack",
      maxQuantity: 1,
      kind: {
        kind: "healthSupply",
        restoreHealth: 10,
        maximumHealth: 100,
        automaticUse: true,
      },
    },
    {
      id: "supply/medikit",
      maxQuantity: 1,
      kind: {
        kind: "healthSupply",
        restoreHealth: 25,
        maximumHealth: 100,
        automaticUse: true,
      },
    },
    {
      id: "supply/health-bonus",
      maxQuantity: 1,
      kind: {
        kind: "healthSupply",
        restoreHealth: 1,
        maximumHealth: 200,
        automaticUse: true,
        consumeAtCap: true,
      },
    },
    {
      id: "weapon/fist",
      maxQuantity: 1,
      kind: {
        kind: "weapon",
        // The closed weapon schema retains an ammunition identity for every
        // attack, while zero cost makes this portable melee action resource-free.
        ammunition: "ammo/bullets",
        attackMode: "hitscan",
        repeatWhileHeld: true,
        damage: 2,
        damageRolls: 10,
        maxDistance: 4,
        cooldownTicks: 38,
        ammunitionCost: 0,
        muzzleOffset: [0, 0, 0],
        presentation: "fist",
      },
    },
    {
      id: "weapon/pistol",
      maxQuantity: 1,
      kind: {
        kind: "weapon",
        ammunition: "ammo/bullets",
        attackMode: "hitscan",
        repeatWhileHeld: true,
        damage: 5,
        damageRolls: 3,
        maxDistance: 128,
        cooldownTicks: 24,
        ammunitionCost: 1,
        muzzleOffset: [0, 0, 0],
        presentation: "pistol",
      },
    },
    {
      id: "weapon/shotgun",
      maxQuantity: 1,
      kind: {
        kind: "weapon",
        ammunition: "ammo/shells",
        attackMode: "spread",
        repeatWhileHeld: true,
        pelletCount: 7,
        spreadDegrees: 5.625,
        damage: 5,
        damageRolls: 3,
        maxDistance: 128,
        cooldownTicks: 63,
        ammunitionCost: 1,
        muzzleOffset: [0, 0, 0],
        presentation: "shotgun",
      },
    },
  ];

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

  const enemyTypes = new Set([9, 3001, 3004]);
  const pickupMap: Record<
    number,
    {
      item: string;
      quantity: number;
      mesh: string;
      starterAmmunition?: { item: string; quantity: number };
    }
  > = {
    2001: {
      item: "weapon/shotgun",
      quantity: 1,
      mesh: "mesh/prop-kit/breach-scattergun",
      starterAmmunition: { item: "ammo/shells", quantity: 8 },
    },
    2007: {
      item: "ammo/bullets",
      quantity: 10,
      mesh: "mesh/prop-kit/energy-cell",
    },
    2008: {
      item: "ammo/shells",
      quantity: 4,
      mesh: "mesh/prop-kit/scatter-shells",
    },
    2048: {
      item: "ammo/bullets",
      quantity: 50,
      mesh: "mesh/prop-kit/energy-cell",
    },
    2049: {
      item: "ammo/shells",
      quantity: 20,
      mesh: "mesh/prop-kit/scatter-shells",
    },
    2011: {
      item: "supply/stimpack",
      quantity: 1,
      mesh: "mesh/prop-kit/med-patch",
    },
    2012: {
      item: "supply/medikit",
      quantity: 1,
      mesh: "mesh/prop-kit/med-patch",
    },
    2014: {
      item: "supply/health-bonus",
      quantity: 1,
      mesh: "mesh/prop-kit/med-patch",
    },
    2015: {
      item: "armor/bonus",
      quantity: 1,
      mesh: "mesh/prop-kit/impact-vest",
    },
    2018: {
      item: "armor/green",
      quantity: 1,
      mesh: "mesh/prop-kit/impact-vest",
    },
    2019: {
      item: "armor/blue",
      quantity: 1,
      mesh: "mesh/prop-kit/impact-vest",
    },
  };

  // Enemies
  let enemyIndex = 0;
  for (const thing of inter.level.things) {
    if (!enemyTypes.has(thing.type)) continue;
    enemyIndex += 1;
    const pos = doomToWorldForThing(thing);
    const isRanged = thing.type === 9 || thing.type === 3001;
    const mesh = isRanged
      ? "mesh-animation/arc-warden"
      : "mesh-animation/bay-rusher";
    const visualBinding = isRanged
      ? {
          version: 1,
          states: [
            {
              state: "idle",
              kind: "animation",
              clip: "idle",
              loopMode: "repeat",
              speed: 1,
              fadeSeconds: 0.12,
            },
            {
              state: "moving",
              kind: "animation",
              clip: "run",
              loopMode: "repeat",
              speed: 1,
              fadeSeconds: 0.1,
            },
            {
              state: "alert",
              kind: "animation",
              clip: "idle",
              loopMode: "repeat",
              speed: 1,
              fadeSeconds: 0.08,
            },
            {
              state: "attacking",
              kind: "animation",
              clip: "attack",
              loopMode: "repeat",
              speed: 1,
              fadeSeconds: 0.06,
            },
            {
              state: "hit",
              kind: "animation",
              clip: "hit",
              loopMode: "once",
              speed: 1,
              fadeSeconds: 0.04,
            },
            {
              state: "defeated",
              kind: "animation",
              clip: "death",
              loopMode: "once",
              speed: 1,
              fadeSeconds: 0.08,
            },
          ],
        }
      : {
          version: 1,
          states: [
            {
              state: "idle",
              kind: "animation",
              clip: "idle",
              loopMode: "repeat",
              speed: 1,
              fadeSeconds: 0.12,
            },
            {
              state: "moving",
              kind: "animation",
              clip: "run",
              loopMode: "repeat",
              speed: 1,
              fadeSeconds: 0.1,
            },
            {
              state: "alert",
              kind: "animation",
              clip: "idle",
              loopMode: "repeat",
              speed: 1,
              fadeSeconds: 0.08,
            },
            {
              state: "attacking",
              kind: "animation",
              clip: "attack",
              loopMode: "repeat",
              speed: 1,
              fadeSeconds: 0.06,
            },
            {
              state: "hit",
              kind: "animation",
              clip: "hit",
              loopMode: "once",
              speed: 1,
              fadeSeconds: 0.04,
            },
            {
              state: "defeated",
              kind: "animation",
              clip: "death",
              loopMode: "once",
              speed: 1,
              fadeSeconds: 0.08,
            },
          ],
        };
    const id = nextId++;
    entities.push({
      id,
      name: `doom-enemy-${thing.type}-${enemyIndex}`,
      translation: pos,
      collision: { enabled: true, staticCollider: false },
      renderable: {
        asset: mesh,
        visible: true,
        localTransform: {
          translation: [0, -0.9, 0],
          rotation: [0, 0, 0, 1],
          scale: [1, 1, 1],
        },
        initialClip: "idle",
        visualBinding,
      },
      enemy: true,
      enemyCombat: {
        sightRange: isRanged ? 12 : 9,
        hearingRange: 5,
        attack: {
          kind: isRanged ? "rangedHitscan" : "melee",
          damage: isRanged ? 6 : 10,
          range: isRanged ? 14 : 1.4,
          cooldownTicks: isRanged ? 90 : 110,
          originOffset: [0, 0.25, 0],
          presentation: isRanged ? "sentry-pulse" : "sentry-strike",
        },
      },
      health: { max: isRanged ? 60 : 40, hitboxHalfExtents: [0.35, 0.7, 0.35] },
      kinematic: { halfExtents: [0.3, 0.4, 0.3], velocity: [0, 0, 0] },
      navigation: {
        goal: pos,
        speedUnitsPerSecond: isRanged ? 3.2 : 4.2,
        maxVisited: 64,
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
      bounds: { min: [-0.3, -0.3, -0.3], max: [0.3, 0.3, 0.3] },
      renderable: {
        asset: mapping.mesh,
        visible: true,
        visualBinding: {
          version: 1,
          states: [
            {
              state: "dormant",
              kind: "material",
              textureTint: [0.58, 0.62, 0.68, 1],
              emissionColor: [0.12, 0.14, 0.18],
              emissionIntensity: 0.04,
            },
            {
              state: "available",
              kind: "material",
              textureTint: [0.62, 1, 0.82, 1],
              emissionColor: [0.12, 0.82, 0.52],
              emissionIntensity: 0.35,
            },
            {
              state: "collected",
              kind: "material",
              textureTint: [1, 1, 1, 1],
              emissionColor: [0, 0, 0],
              emissionIntensity: 0,
            },
          ],
        },
      },
      pickup: {
        item: mapping.item,
        quantity: mapping.quantity,
        ...(mapping.starterAmmunition
          ? { starterAmmunition: mapping.starterAmmunition }
          : {}),
      },
    });
  }

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
  writeFileSync(outPath, `${JSON.stringify(project, null, 2)}\n`, "utf8");

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
  buildDoomE1M1Project();
}
