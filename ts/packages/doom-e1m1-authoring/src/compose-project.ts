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

function wadToWorld(x: number, y: number, floorHeight: number): [number, number, number] {
  return [(x - MIN_X) / SCALE, (floorHeight - MIN_FLOOR) / SCALE + 0.5, (y - MIN_Y) / SCALE];
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
  const sectorEdges = new Map<number, { x1: number; y1: number; x2: number; y2: number }[]>();
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

function isInside(px: number, py: number, si: number, sectorEdges: Map<number, { x1: number; y1: number; x2: number; y2: number }[]>): boolean {
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

function findSectorForPoint(x: number, y: number, inter: Intermediate, sectorEdges: Map<number, any>): number {
  for (let si = 0; si < inter.level.sectors.length; si += 1) {
    if (isInside(x, y, si, sectorEdges)) return si;
  }
  return -1;
}

export function buildDoomE1M1Project(intermediatePath = fileURLToPath(new URL("../../../../content/doom-e1m1/e1m1.intermediate.json", import.meta.url)), manifestPath = fileURLToPath(new URL("../../../../content/doom-e1m1/textures/manifest.json", import.meta.url)), voxelPath = fileURLToPath(new URL("../../../../content/doom-e1m1/doom-e1m1.voxel.json", import.meta.url)), outPath = fileURLToPath(new URL("../../../../content/projects/doom-e1m1.project.json", import.meta.url)), loadingBayPath = fileURLToPath(new URL("../../../../content/projects/loading-bay.project.json", import.meta.url))): any {
  const inter: Intermediate = JSON.parse(readFileSync(intermediatePath, "utf8"));
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
    const tileScale: [number, number] = entry.tileScale ?? [entry.width / SCALE, entry.height / SCALE];
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
            alphaMode: entry.kind === "wall" ? { kind: "mask", cutoff: 0.5 } : { kind: "opaque" },
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

  // Item definitions — copy from loading-bay
  const itemDefinitions = [
    { id: "ammo/energy-cell", maxQuantity: 200, kind: { kind: "ammunition" } },
    { id: "ammo/kinetic-slug", maxQuantity: 32, kind: { kind: "ammunition" } },
    { id: "ammo/scatter-shell", maxQuantity: 50, kind: { kind: "ammunition" } },
    {
      id: "armor/impact-vest",
      maxQuantity: 1,
      kind: { kind: "armor", protection: 100 },
    },
    {
      id: "key/inert-inspection-tag",
      maxQuantity: 1,
      kind: { kind: "accessKey" },
    },
    { id: "key/maintenance-pass", maxQuantity: 1, kind: { kind: "accessKey" } },
    {
      id: "supply/med-patch",
      maxQuantity: 5,
      kind: { kind: "healthSupply", restoreHealth: 25 },
    },
    {
      id: "weapon/arc-pistol",
      maxQuantity: 1,
      kind: {
        kind: "weapon",
        ammunition: "ammo/energy-cell",
        attackMode: "hitscan",
        damage: 60,
        maxDistance: 20,
        cooldownTicks: 2,
        ammunitionCost: 1,
        muzzleOffset: [0, 0, 0],
        presentation: "arc-pistol",
      },
    },
    {
      id: "weapon/breach-scattergun",
      maxQuantity: 1,
      kind: {
        kind: "weapon",
        ammunition: "ammo/scatter-shell",
        attackMode: "spread",
        pelletCount: 7,
        spreadDegrees: 7,
        damage: 14,
        maxDistance: 12,
        cooldownTicks: 36,
        ammunitionCost: 1,
        muzzleOffset: [0, 0, 0],
        presentation: "breach-scattergun",
      },
    },
    {
      id: "weapon/kinetic-launcher",
      maxQuantity: 1,
      kind: {
        kind: "weapon",
        ammunition: "ammo/kinetic-slug",
        attackMode: "projectile",
        damage: 45,
        maxDistance: 60,
        cooldownTicks: 18,
        ammunitionCost: 1,
        muzzleOffset: [0, 0, -0.35],
        presentation: "kinetic-launcher",
        projectileMass: 0.25,
        projectileRadius: 0.12,
        projectileImpulse: 18,
        projectileGravityScale: 0.8,
        projectileLifetimeTicks: 180,
        projectileRestitution: 0.1,
      },
    },
    {
      id: "weapon/rivet-carbine",
      maxQuantity: 1,
      kind: {
        kind: "weapon",
        ammunition: "ammo/energy-cell",
        attackMode: "automatic",
        damage: 18,
        maxDistance: 25,
        cooldownTicks: 4,
        ammunitionCost: 1,
        muzzleOffset: [0, 0, 0],
        presentation: "rivet-carbine",
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
      max: 100,
      hitboxHalfExtents: [0.25, 0.5, 0.25],
      maxArmor: 100,
      armorAbsorptionPercent: 50,
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
        selectWeapon: ["Digit1", "Digit2", "Digit3", "Digit4"],
      },
    },
    inventory: {
      capacitySlots: 10,
      startingStacks: [
        { item: "weapon/arc-pistol", quantity: 1 },
        { item: "ammo/energy-cell", quantity: 30 },
        { item: "supply/med-patch", quantity: 1 },
      ],
      initiallyEquippedWeapon: "weapon/arc-pistol",
      weaponSlots: ["weapon/arc-pistol", "weapon/breach-scattergun", "weapon/rivet-carbine", "weapon/kinetic-launcher"],
    },
  });

  const enemyTypes = new Set([9, 3001, 3004]);
  const pickupMap: Record<number, { item: string; quantity: number; mesh: string }> = {
    2001: {
      item: "weapon/breach-scattergun",
      quantity: 1,
      mesh: "mesh/prop-kit/breach-scattergun",
    },
    2002: {
      item: "weapon/rivet-carbine",
      quantity: 1,
      mesh: "mesh/prop-kit/rivet-carbine",
    },
    2003: {
      item: "weapon/kinetic-launcher",
      quantity: 1,
      mesh: "mesh/prop-kit/security-door",
    },
    2007: {
      item: "ammo/energy-cell",
      quantity: 12,
      mesh: "mesh/prop-kit/energy-cell",
    },
    2008: {
      item: "ammo/scatter-shell",
      quantity: 8,
      mesh: "mesh/prop-kit/scatter-shells",
    },
    2011: {
      item: "supply/med-patch",
      quantity: 1,
      mesh: "mesh/prop-kit/med-patch",
    },
    2012: {
      item: "supply/med-patch",
      quantity: 2,
      mesh: "mesh/prop-kit/med-patch",
    },
    2014: {
      item: "supply/med-patch",
      quantity: 1,
      mesh: "mesh/prop-kit/med-patch",
    },
    2015: {
      item: "armor/impact-vest",
      quantity: 1,
      mesh: "mesh/prop-kit/impact-vest",
    },
    2018: {
      item: "armor/impact-vest",
      quantity: 1,
      mesh: "mesh/prop-kit/impact-vest",
    },
    2019: {
      item: "armor/impact-vest",
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
    const mesh = isRanged ? "mesh-animation/arc-warden" : "mesh-animation/bay-rusher";
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
    if (!mapping) continue;
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
      pickup: { item: mapping.item, quantity: mapping.quantity },
    });
  }

  // Doors — first up to 5 door linedefs (lineType 1)
  const doorLines = inter.level.linedefs.filter((ld) => ld.lineType === 1).slice(0, 5);
  for (let di = 0; di < doorLines.length; di += 1) {
    const ld = doorLines[di]!;
    const v1 = inter.level.vertices[ld.startVertex]!;
    const v2 = inter.level.vertices[ld.endVertex]!;
    const mx = (v1.x + v2.x) / 2;
    const my = (v1.y + v2.y) / 2;
    const si = findSectorForPoint(mx, my, inter, sectorEdges);
    const floor = si >= 0 ? inter.level.sectors[si]!.floorHeight : 0;
    const ceil = si >= 0 ? inter.level.sectors[si]!.ceilingHeight : 72;
    const pos = wadToWorld(mx, my, floor);
    const admittedCeiling = (ceil - MIN_FLOOR) / SCALE + 0.5;
    const openPos: [number, number, number] = [pos[0], Math.max(admittedCeiling, pos[1] + 4), pos[2]];
    const id = nextId++;
    entities.push({
      id,
      name: `doom-door-${di + 1}`,
      translation: pos,
      bounds: { min: [-0.8, -1.2, -0.2], max: [0.8, 1.2, 0.2] },
      collision: { enabled: true, staticCollider: true },
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
              state: "open",
              kind: "material",
              textureTint: [0.62, 1, 0.82, 1],
              emissionColor: [0.12, 0.82, 0.52],
              emissionIntensity: 0.35,
            },
          ],
        },
      },
      door: { openTranslation: openPos, autoCloseAfterTicks: null },
      kinematic: { halfExtents: [0.8, 1.2, 0.2], velocity: [0, 0, 0] },
    });
  }

  // One switch that controls first door
  const doorIds = entities.filter((e) => e.door).map((e) => e.id);
  if (doorIds.length >= 2) {
    const swPos = wadToWorld(1088, -3600, 0);
    const id = nextId++;
    entities.push({
      id,
      name: "doom-switch-1",
      translation: swPos,
      renderable: {
        asset: "mesh/prop-kit/control-panel",
        visible: true,
        localTransform: {
          translation: [0, -0.775, 0],
          rotation: [0, 0, 0, 1],
          scale: [1, 1, 1],
        },
        visualBinding: {
          version: 1,
          states: [
            {
              state: "inactive",
              kind: "material",
              textureTint: [1, 0.78, 0.48, 1],
              emissionColor: [0.75, 0.28, 0.05],
              emissionIntensity: 0.12,
            },
            {
              state: "active",
              kind: "material",
              textureTint: [0.62, 1, 0.82, 1],
              emissionColor: [0.12, 0.82, 0.52],
              emissionIntensity: 0.35,
            },
          ],
        },
      },
      switch: {
        controls: doorIds,
        loadingBayInterlock: {
          closeDoor: doorIds[1]!,
          openDoor: doorIds[0]!,
        },
      },
    });
  }

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
    levelExit: { activationRadius: 4, presentation: "Doom E1M1 complete" },
  });

  // Secret — first secret sector (special 9) center
  const secretSectorIdx = inter.level.sectors.findIndex((s) => s.special === 9);
  if (secretSectorIdx >= 0) {
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
        name: "doom-secret-1",
        translation: pos,
        bounds: { min: [-1.5, -0.8, -1.5], max: [1.5, 0.8, 1.5] },
        secretRegion: { presentation: "Hangar secret discovered" },
      });
    }
  }

  // Sort assets deterministic for canonical encoding
  assets.sort((a, b) => a.id.localeCompare(b.id));
  entities.sort((a, b) => a.id - b.id);

  const project = {
    schemaVersion: 24,
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
  const result = spawnSync("cargo", ["run", "--quiet", "--locked", "-p", "loading-bay-game", "--bin", "project-store", "--", "--input", outPath, "--output", canonicalOut], { encoding: "utf8" });
  if (result.status !== 0) {
    console.error(result.stderr, result.stdout);
    throw new Error(`project-store canonicalize failed for ${outPath}`);
  }
  const canonBytes = readFileSync(canonicalOut, "utf8");
  writeFileSync(outPath, canonBytes, "utf8");
  try {
    unlinkSync(canonicalOut);
  } catch {}
  console.log(`Wrote ${outPath} entities=${entities.length} assets=${assets.length} bytes=${canonBytes.length}`);
  return project;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  buildDoomE1M1Project();
}
