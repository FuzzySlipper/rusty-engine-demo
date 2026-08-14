import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

import { decodeWad } from "./wad-decode.js";
import {
  decodePalette,
  decodePatch,
  directoryByName,
  type Palette,
  wadLumpBytes,
} from "./textures.js";
import { encodePngRgba8, sha256Hex } from "./png.js";
import {
  DOOM_SPRITE_CONTRACT_SOURCES,
  DOOM_SPRITE_FAMILY_DEFINITIONS,
  DOOM_SPRITE_TICK_RATE_HZ,
  parseSpriteLumpAssignments,
  type DoomSpriteClipDefinition,
} from "./sprite-contract.js";
import type { DoomWadDirectoryEntry, DoomWadInfo } from "./types.js";

export const DEFAULT_SPRITE_WAD = "/home/research/doom.ts/public/doom1.wad";
export const DEFAULT_SPRITE_OUT_DIR = fileURLToPath(
  new URL("../../../../content/doom-e1m1/sprites/", import.meta.url),
);

export const SPRITE_FAMILIES = [
  { prefix: "POSS", kind: "actor", atlas: "actors" },
  { prefix: "SPOS", kind: "actor", atlas: "actors" },
  { prefix: "TROO", kind: "actor", atlas: "actors" },
  { prefix: "BAL1", kind: "effect", atlas: "effects" },
  { prefix: "BLUD", kind: "effect", atlas: "effects" },
  { prefix: "PUFF", kind: "effect", atlas: "effects" },
  { prefix: "PUNG", kind: "weapon", atlas: "weapons" },
  { prefix: "PISG", kind: "weapon", atlas: "weapons" },
  { prefix: "PISF", kind: "weapon", atlas: "weapons" },
  { prefix: "SHTG", kind: "weapon", atlas: "weapons" },
  { prefix: "SHTF", kind: "weapon", atlas: "weapons" },
  { prefix: "SHOT", kind: "item", atlas: "effects" },
  { prefix: "CLIP", kind: "item", atlas: "effects" },
  { prefix: "SHEL", kind: "item", atlas: "effects" },
  { prefix: "AMMO", kind: "item", atlas: "effects" },
  { prefix: "SBOX", kind: "item", atlas: "effects" },
  { prefix: "STIM", kind: "item", atlas: "effects" },
  { prefix: "MEDI", kind: "item", atlas: "effects" },
  { prefix: "BON1", kind: "item", atlas: "effects" },
  { prefix: "BON2", kind: "item", atlas: "effects" },
  { prefix: "ARM1", kind: "item", atlas: "effects" },
  { prefix: "ARM2", kind: "item", atlas: "effects" },
] as const;

const ATLAS_CONFIGS = [
  {
    key: "actors",
    id: "sprite/doom-e1m1-actors",
    textureId: "texture/doom-e1m1-actors",
    file: "actors.png",
    width: 512,
  },
  {
    key: "effects",
    id: "sprite/doom-e1m1-effects",
    textureId: "texture/doom-e1m1-effects",
    file: "effects.png",
    width: 128,
  },
  {
    key: "weapons",
    id: "sprite/doom-e1m1-weapons",
    textureId: "texture/doom-e1m1-weapons",
    file: "weapons.png",
    width: 512,
  },
] as const;

const ATLAS_PADDING = 1;

type SpriteFamily = (typeof SPRITE_FAMILIES)[number];

export interface SelectedSpriteLump {
  readonly family: SpriteFamily;
  readonly directoryIndex: number;
  readonly entry: DoomWadDirectoryEntry;
}
export interface DecodedSprite {
  readonly width: number;
  readonly height: number;
  readonly leftOffset: number;
  readonly topOffset: number;
  readonly rgba: Uint8Array;
  readonly opaquePixelCount: number;
}

export interface SpriteSourceLumpManifest {
  readonly name: string;
  readonly family: string;
  readonly directoryIndex: number;
  readonly size: number;
  readonly sha256: string;
}

export interface SpriteFrameManifest {
  readonly id: number;
  readonly key: string;
  readonly name: string;
  readonly family: string;
  readonly atlas: string;
  readonly sourceLump: string;
  readonly sourceByteLength: number;
  readonly sourceSha256: string;
  readonly atlasRect: readonly [number, number, number, number];
  readonly uv: {
    readonly min: readonly [number, number];
    readonly max: readonly [number, number];
  };
  readonly pixelSize: readonly [number, number];
  readonly origin: readonly [number, number];
  readonly pivot: readonly [number, number];
  readonly boundsFromOrigin: {
    readonly left: number;
    readonly right: number;
    readonly top: number;
    readonly bottom: number;
  };
  readonly opaquePixelCount: number;
}

export interface SpriteDirectionalFrameManifest {
  readonly frame: string;
  readonly rotations: readonly {
    readonly rotation: number;
    readonly sourceLump: string;
    readonly atlasFrame: number;
    readonly mirrored: boolean;
  }[];
}

export interface SpriteFamilyContractManifest {
  readonly prefix: string;
  readonly role: "actor" | "projectile" | "effect" | "weapon" | "item";
  readonly thingType: number | null;
  readonly dimensionsDoomUnits: {
    readonly radius: number;
    readonly height: number;
  } | null;
  readonly directionalFrames: readonly SpriteDirectionalFrameManifest[];
  readonly clips: readonly DoomSpriteClipDefinition[];
}

export interface SpriteAtlasManifest {
  readonly id: string;
  readonly textureId: string;
  readonly file: string;
  readonly width: number;
  readonly height: number;
  readonly padding: number;
  readonly pngSha256: string;
  readonly pngByteLength: number;
  readonly frames: readonly SpriteFrameManifest[];
}

export interface SpriteManifest {
  readonly schemaVersion: 2;
  readonly wadPath: string;
  readonly wadIdentification: string;
  readonly wadSha256: string;
  readonly wadByteLength: number;
  readonly spriteRange: {
    readonly startLump: string;
    readonly startIndex: number;
    readonly endLump: string;
    readonly endIndex: number;
  };
  readonly paletteLump: {
    readonly name: string;
    readonly size: number;
    readonly sha256: string;
  };
  readonly sourceLumps: readonly SpriteSourceLumpManifest[];
  readonly atlases: readonly SpriteAtlasManifest[];
  readonly contract: {
    readonly tickRateHz: number;
    readonly sources: typeof DOOM_SPRITE_CONTRACT_SOURCES;
    readonly scale: {
      readonly mapDoomUnitsPerEngineUnit: 16;
      readonly actorReferenceHeightDoomUnits: 56;
      readonly actorReferenceHeightEngineUnits: 2;
      readonly presentationDoomUnitsPerEngineUnit: 28;
    };
    readonly families: readonly SpriteFamilyContractManifest[];
  };
  readonly diagnostics: {
    readonly sourceLumpCount: number;
    readonly frameCount: number;
    readonly atlasCount: number;
    readonly totalPngBytes: number;
    readonly totalDecodedRgbaBytes: number;
    readonly totalOpaquePixels: number;
  };
}

export interface RenderedSpriteArtifacts {
  readonly manifest: SpriteManifest;
  readonly manifestJson: string;
  readonly files: readonly {
    readonly relativePath: string;
    readonly bytes: Uint8Array;
  }[];
}

interface DecodedSelectedSprite extends SelectedSpriteLump {
  readonly decoded: DecodedSprite;
  readonly sourceSha256: string;
}

interface AtlasPlacement {
  readonly selected: DecodedSelectedSprite;
  readonly x: number;
  readonly y: number;
}

/**
 * Decode one Doom patch into straight-alpha RGBA.
 *
 * Doom transparency is represented by absent column posts. Palette index 255
 * remains a real opaque palette value here; it is never used as a transparent
 * sentinel.
 */
export function decodeSpritePatchToRgba(
  raw: Uint8Array,
  palette: Palette,
): DecodedSprite {
  const patch = decodePatch(raw);
  const rgba = new Uint8Array(patch.width * patch.height * 4);
  const occupied = new Uint8Array(patch.width * patch.height);
  let opaquePixelCount = 0;

  for (let column = 0; column < patch.width; column += 1) {
    let offset = patch.columnOffsets[column]!;
    while (true) {
      if (offset >= patch.raw.length) {
        throw new Error(`sprite column ${column} has no terminator`);
      }
      const topDelta = patch.raw[offset]!;
      if (topDelta === 0xff) break;
      if (offset + 2 >= patch.raw.length) {
        throw new Error(`sprite column ${column} post header is truncated`);
      }
      const length = patch.raw[offset + 1]!;
      const sourceStart = offset + 3;
      const sourceEnd = sourceStart + length;
      if (sourceEnd >= patch.raw.length) {
        throw new Error(`sprite column ${column} post data is truncated`);
      }

      for (let row = 0; row < length; row += 1) {
        const y = topDelta + row;
        if (y < 0 || y >= patch.height) continue;
        const pixel = y * patch.width + column;
        const paletteIndex = patch.raw[sourceStart + row]!;
        const [red, green, blue] = palette[paletteIndex] ?? [0, 0, 0];
        const rgbaOffset = pixel * 4;
        rgba[rgbaOffset] = red!;
        rgba[rgbaOffset + 1] = green!;
        rgba[rgbaOffset + 2] = blue!;
        rgba[rgbaOffset + 3] = 255;
        if (occupied[pixel] === 0) {
          occupied[pixel] = 1;
          opaquePixelCount += 1;
        }
      }
      // A post has one unused byte after its pixel data.
      offset = sourceEnd + 1;
    }
  }

  return {
    width: patch.width,
    height: patch.height,
    leftOffset: patch.leftOffset,
    topOffset: patch.topOffset,
    rgba,
    opaquePixelCount,
  };
}

export function selectCanonicalSpriteLumps(
  entries: readonly DoomWadDirectoryEntry[],
): {
  readonly startIndex: number;
  readonly endIndex: number;
  readonly lumps: readonly SelectedSpriteLump[];
} {
  const startIndex = entries.findIndex(
    (entry) => entry.name === "S_START" && entry.size === 0,
  );
  if (startIndex < 0) throw new Error("sprite start marker S_START not found");
  const endIndex = entries.findIndex(
    (entry, index) =>
      index > startIndex && entry.name === "S_END" && entry.size === 0,
  );
  if (endIndex < 0)
    throw new Error("sprite end marker S_END not found after S_START");

  const lumps: SelectedSpriteLump[] = [];
  const seen = new Set<string>();
  for (const family of SPRITE_FAMILIES) {
    const familyLumps = entries
      .map((entry, directoryIndex) => ({ entry, directoryIndex }))
      .filter(
        ({ entry, directoryIndex }) =>
          directoryIndex > startIndex &&
          directoryIndex < endIndex &&
          entry.name.startsWith(family.prefix),
      );
    if (familyLumps.length === 0) {
      throw new Error(
        `no ${family.prefix} sprite lumps found between S_START and S_END`,
      );
    }
    for (const selected of familyLumps) {
      if (!seen.add(selected.entry.name)) {
        throw new Error(
          `sprite lump selected more than once: ${selected.entry.name}`,
        );
      }
      lumps.push({ ...selected, family });
    }
  }

  return { startIndex, endIndex, lumps };
}

function loadWad(path: string): {
  readonly buffer: ArrayBuffer;
  readonly wad: DoomWadInfo;
} {
  const bytes = readFileSync(path);
  const buffer = bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer;
  return { buffer, wad: decodeWad(buffer, { computeSha256: true }) };
}

function packAtlas(
  selected: readonly DecodedSelectedSprite[],
  width: number,
): {
  readonly placements: readonly AtlasPlacement[];
  readonly height: number;
  readonly rgba: Uint8Array;
} {
  const placements: AtlasPlacement[] = [];
  let x = ATLAS_PADDING;
  let y = ATLAS_PADDING;
  let rowHeight = 0;

  for (const sprite of selected) {
    if (sprite.decoded.width + ATLAS_PADDING * 2 > width) {
      throw new Error(
        `sprite ${sprite.entry.name} does not fit atlas width ${width}`,
      );
    }
    if (x > ATLAS_PADDING && x + sprite.decoded.width + ATLAS_PADDING > width) {
      x = ATLAS_PADDING;
      y += rowHeight + ATLAS_PADDING;
      rowHeight = 0;
    }
    placements.push({ selected: sprite, x, y });
    x += sprite.decoded.width + ATLAS_PADDING;
    rowHeight = Math.max(rowHeight, sprite.decoded.height);
  }

  const height = y + rowHeight + ATLAS_PADDING;
  const rgba = new Uint8Array(width * height * 4);
  for (const placement of placements) {
    const { decoded } = placement.selected;
    for (let row = 0; row < decoded.height; row += 1) {
      const sourceStart = row * decoded.width * 4;
      const sourceEnd = sourceStart + decoded.width * 4;
      // Doom patch rows and PNG image rows both use a top-left origin. Preserve
      // that ordinary image orientation; the Engine sprite UV contract maps
      // the decoded PNG top directly to the sprite-plane top.
      const targetRow = placement.y + row;
      const targetStart = (targetRow * width + placement.x) * 4;
      rgba.set(decoded.rgba.subarray(sourceStart, sourceEnd), targetStart);
    }
  }
  return { placements, height, rgba };
}

function frameKey(family: SpriteFamily, name: string): string {
  return `${family.prefix.toLowerCase()}:${name.slice(family.prefix.length).toLowerCase()}`;
}

function buildFrameManifest(
  placement: AtlasPlacement,
  frameId: number,
  atlas: (typeof ATLAS_CONFIGS)[number],
  atlasHeight: number,
): SpriteFrameManifest {
  const { family, entry, decoded } = placement.selected;
  const uvMin: readonly [number, number] = [
    placement.x / atlas.width,
    placement.y / atlasHeight,
  ];
  const uvMax: readonly [number, number] = [
    (placement.x + decoded.width) / atlas.width,
    (placement.y + decoded.height) / atlasHeight,
  ];
  return {
    id: frameId,
    key: frameKey(family, entry.name),
    name: entry.name,
    family: family.prefix,
    atlas: atlas.key,
    sourceLump: entry.name,
    sourceByteLength: entry.size,
    sourceSha256: placement.selected.sourceSha256,
    atlasRect: [placement.x, placement.y, decoded.width, decoded.height],
    uv: { min: uvMin, max: uvMax },
    pixelSize: [decoded.width, decoded.height],
    origin: [decoded.leftOffset, decoded.topOffset],
    pivot: [
      decoded.leftOffset / decoded.width,
      (decoded.height - decoded.topOffset) / decoded.height,
    ],
    boundsFromOrigin: {
      left: -decoded.leftOffset,
      right: decoded.width - decoded.leftOffset,
      top: decoded.topOffset,
      bottom: decoded.topOffset - decoded.height,
    },
    opaquePixelCount: decoded.opaquePixelCount,
  };
}

function buildFamilyContracts(
  atlases: readonly SpriteAtlasManifest[],
): readonly SpriteFamilyContractManifest[] {
  const frames = atlases.flatMap((atlas) => atlas.frames);
  return DOOM_SPRITE_FAMILY_DEFINITIONS.map((definition) => {
    const byFrame = new Map<
      string,
      Array<{ rotation: number; sourceLump: string; atlasFrame: number; mirrored: boolean }>
    >();
    for (const frame of frames.filter((candidate) => candidate.family === definition.prefix)) {
      for (const assignment of parseSpriteLumpAssignments(definition.prefix, frame.sourceLump)) {
        const rotations = byFrame.get(assignment.frame) ?? [];
        rotations.push({
          rotation: assignment.rotation,
          sourceLump: frame.sourceLump,
          atlasFrame: frame.id,
          mirrored: assignment.mirrored,
        });
        byFrame.set(assignment.frame, rotations);
      }
    }
    const directionalFrames = [...byFrame.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([frame, rotations]) => {
        rotations.sort((left, right) => left.rotation - right.rotation);
        const coverage = rotations.map((rotation) => rotation.rotation);
        const expected = coverage[0] === 0 ? [0] : [1, 2, 3, 4, 5, 6, 7, 8];
        if (coverage.length !== expected.length || coverage.some((value, index) => value !== expected[index])) {
          throw new Error(`${definition.prefix}${frame} has incomplete rotation coverage: ${coverage.join(",")}`);
        }
        return { frame, rotations };
      });
    const availableFrames = new Set(directionalFrames.map((frame) => frame.frame));
    for (const clip of definition.clips) {
      for (const clipStep of clip.steps) {
        if (!availableFrames.has(clipStep.frame)) {
          throw new Error(`${definition.prefix} clip ${clip.id} references missing frame ${clipStep.frame}`);
        }
      }
    }
    return { ...definition, directionalFrames };
  });
}

export function serializeSpriteManifest(manifest: SpriteManifest): string {
  return `${JSON.stringify(manifest, null, 2)}\n`;
}

export function renderSpriteArtifacts(
  wadPath: string = DEFAULT_SPRITE_WAD,
): RenderedSpriteArtifacts {
  const { buffer, wad } = loadWad(wadPath);
  const selected = selectCanonicalSpriteLumps(wad.entries);
  const byName = directoryByName(wad.entries);
  const playpal = byName.get("PLAYPAL");
  if (!playpal) throw new Error("WAD missing PLAYPAL lump");
  const playpalBytes = wadLumpBytes(buffer, playpal);
  const palette = decodePalette(playpalBytes);

  const decoded: DecodedSelectedSprite[] = selected.lumps.map((lump) => {
    const bytes = wadLumpBytes(buffer, lump.entry);
    return {
      ...lump,
      decoded: decodeSpritePatchToRgba(bytes, palette),
      sourceSha256: sha256Hex(bytes),
    };
  });

  const atlases: SpriteAtlasManifest[] = [];
  const files: { relativePath: string; bytes: Uint8Array }[] = [];
  let totalOpaquePixels = 0;
  let totalDecodedRgbaBytes = 0;
  for (const atlas of ATLAS_CONFIGS) {
    const atlasSprites = decoded.filter(
      (sprite) => sprite.family.atlas === atlas.key,
    );
    const packed = packAtlas(atlasSprites, atlas.width);
    const pngBytes = encodePngRgba8(atlas.width, packed.height, packed.rgba);
    const frames = packed.placements.map((placement, frameId) =>
      buildFrameManifest(placement, frameId, atlas, packed.height),
    );
    const manifestAtlas: SpriteAtlasManifest = {
      id: atlas.id,
      textureId: atlas.textureId,
      file: atlas.file,
      width: atlas.width,
      height: packed.height,
      padding: ATLAS_PADDING,
      pngSha256: sha256Hex(pngBytes),
      pngByteLength: pngBytes.length,
      frames,
    };
    atlases.push(manifestAtlas);
    files.push({ relativePath: atlas.file, bytes: pngBytes });
    totalOpaquePixels += atlasSprites.reduce(
      (sum, sprite) => sum + sprite.decoded.opaquePixelCount,
      0,
    );
    totalDecodedRgbaBytes += atlasSprites.reduce(
      (sum, sprite) => sum + sprite.decoded.rgba.length,
      0,
    );
  }

  const sourceLumps: SpriteSourceLumpManifest[] = decoded.map(
    ({ family, entry, directoryIndex, sourceSha256 }) => ({
      name: entry.name,
      family: family.prefix,
      directoryIndex,
      size: entry.size,
      sha256: sourceSha256,
    }),
  );
  const manifest: SpriteManifest = {
    schemaVersion: 2,
    wadPath,
    wadIdentification: wad.identification,
    wadSha256: wad.sha256!,
    wadByteLength: wad.byteLength,
    spriteRange: {
      startLump: "S_START",
      startIndex: selected.startIndex,
      endLump: "S_END",
      endIndex: selected.endIndex,
    },
    paletteLump: {
      name: playpal.name,
      size: playpal.size,
      sha256: sha256Hex(playpalBytes),
    },
    sourceLumps,
    atlases,
    contract: {
      tickRateHz: DOOM_SPRITE_TICK_RATE_HZ,
      sources: DOOM_SPRITE_CONTRACT_SOURCES,
      scale: {
        mapDoomUnitsPerEngineUnit: 16,
        actorReferenceHeightDoomUnits: 56,
        actorReferenceHeightEngineUnits: 2,
        presentationDoomUnitsPerEngineUnit: 28,
      },
      families: buildFamilyContracts(atlases),
    },
    diagnostics: {
      sourceLumpCount: sourceLumps.length,
      frameCount: sourceLumps.length,
      atlasCount: atlases.length,
      totalPngBytes: atlases.reduce(
        (sum, atlas) => sum + atlas.pngByteLength,
        0,
      ),
      totalDecodedRgbaBytes,
      totalOpaquePixels,
    },
  };
  const manifestJson = serializeSpriteManifest(manifest);
  files.push({
    relativePath: "manifest.json",
    bytes: new TextEncoder().encode(manifestJson),
  });
  return { manifest, manifestJson, files };
}

export function buildSpriteArtifacts(
  wadPath: string = DEFAULT_SPRITE_WAD,
  outDir: string = DEFAULT_SPRITE_OUT_DIR,
): SpriteManifest {
  const rendered = renderSpriteArtifacts(wadPath);
  mkdirSync(outDir, { recursive: true });
  for (const file of rendered.files)
    writeFileSync(join(outDir, file.relativePath), file.bytes);
  return rendered.manifest;
}

export function checkSpriteArtifacts(
  wadPath: string = DEFAULT_SPRITE_WAD,
  outDir: string = DEFAULT_SPRITE_OUT_DIR,
): void {
  const manifestPath = join(outDir, "manifest.json");
  if (!existsSync(manifestPath))
    throw new Error(
      `${manifestPath} missing; run sprite generation with --write`,
    );
  const actualManifest = readFileSync(manifestPath, "utf8");
  const rendered = renderSpriteArtifacts(wadPath);
  if (actualManifest !== rendered.manifestJson) {
    throw new Error(
      `${manifestPath} is stale; run sprite generation with --write`,
    );
  }
  for (const file of rendered.files) {
    const path = join(outDir, file.relativePath);
    if (file.relativePath === "manifest.json") continue;
    if (!existsSync(path))
      throw new Error(`${path} missing; run sprite generation with --write`);
    const actual = readFileSync(path);
    if (!actual.equals(Buffer.from(file.bytes)))
      throw new Error(`${path} is stale; run sprite generation with --write`);
  }
  const frameCount = rendered.manifest.diagnostics.frameCount;
  console.log(
    `OK ${manifestPath} (${frameCount} frames, ${rendered.manifest.diagnostics.totalPngBytes} PNG bytes)`,
  );
}
