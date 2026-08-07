export interface WadVertex {
  readonly x: number;
  readonly y: number;
}

export interface WadThing {
  readonly x: number;
  readonly y: number;
  readonly angle: number;
  readonly type: number;
  readonly options: number;
}

export interface WadLinedef {
  readonly startVertex: number;
  readonly endVertex: number;
  readonly flags: number;
  readonly lineType: number;
  readonly sectorTag: number;
  readonly frontSidedef: number;
  /** -1 means none (back side absent, signed 16-bit sentinel) */
  readonly backSidedef: number;
}

export interface WadSidedef {
  readonly xOffset: number;
  readonly yOffset: number;
  readonly upperTexture: string;
  readonly lowerTexture: string;
  readonly middleTexture: string;
  readonly sector: number;
}

export interface WadSector {
  readonly floorHeight: number;
  readonly ceilingHeight: number;
  readonly floorTexture: string;
  readonly ceilingTexture: string;
  readonly lightLevel: number;
  readonly special: number;
  readonly tag: number;
}

export interface WadTexturePatchName {
  readonly name: string;
}

export interface DoomWadDirectoryEntry {
  readonly name: string;
  readonly filePosition: number;
  readonly size: number;
}

export interface DoomE1M1Level {
  readonly mapName: string;
  readonly vertices: readonly WadVertex[];
  readonly things: readonly WadThing[];
  readonly linedefs: readonly WadLinedef[];
  readonly sidedefs: readonly WadSidedef[];
  readonly sectors: readonly WadSector[];
  /** Lump-directory index of the E1M1 label (size 0) */
  readonly lumpIndex: number;
}

export interface DoomWadInfo {
  /** `IWAD` or `PWAD` */
  readonly identification: string;
  readonly entries: readonly DoomWadDirectoryEntry[];
  /** SHA-256 hex of the raw bytes, if computed */
  readonly sha256: string | undefined;
  readonly byteLength: number;
}

export interface E1M1Intermediate {
  readonly source: {
    readonly wadPath: string;
    readonly wadSha256: string;
    readonly wadByteLength: number;
    readonly wadIdentification: string;
    readonly entryCount: number;
    readonly e1m1LumpIndex: number;
  };
  readonly level: DoomE1M1Level;
  readonly diagnostics: {
    readonly vertexBounds: { readonly minX: number; readonly maxX: number; readonly minY: number; readonly maxY: number };
    readonly sectorFloorCeilingRange: { readonly minFloor: number; readonly maxCeiling: number };
    readonly textureIncidence: {
      readonly wallTextureNames: readonly string[];
      readonly flatTextureNames: readonly string[];
    };
  };
}

/**
 * Stable JSON-serializable view of the intermediate for `--write` checks.
 * Keys are in definition order; arrays preserve WAD order; no floats beyond
 * the 16-bit integers in this layer.
 */
export type E1M1Json = E1M1Intermediate;
