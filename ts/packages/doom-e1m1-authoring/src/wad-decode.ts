import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

import type {
  DoomE1M1Level,
  DoomWadDirectoryEntry,
  DoomWadInfo,
  E1M1Intermediate,
  E1M1Json,
  WadLinedef,
  WadSector,
  WadSidedef,
  WadThing,
  WadVertex,
} from "./types.js";

// E1M1 is the 0-size label; the 10 following lumps are the mappayload in fixed order.
// This mirrors doom.ts `MapLumpOrder` (Label, Things, LineDefs, SideDefs, Vertexes, Segs, SSectors, Nodes, Sectors, Reject, BlockMap)
// but we only decode the 5 structures needed for the showcase.
const MAP_PAYLOAD = [
  "THINGS",
  "LINEDEFS",
  "SIDEDEFS",
  "VERTEXES",
  "SEGS",
  "SSECTORS",
  "NODES",
  "SECTORS",
  "REJECT",
  "BLOCKMAP",
] as const;

export function decodeWad(buffer: ArrayBuffer, opts?: { computeSha256?: boolean }): DoomWadInfo {
  if (buffer.byteLength < 12) {
    throw new Error(`WAD too short: ${buffer.byteLength} bytes`);
  }
  const view = new DataView(buffer);
  const ident = decodeAscii(buffer.slice(0, 4)).trim();
  if (ident !== "IWAD" && ident !== "PWAD") {
    throw new Error(`WAD identification must be IWAD or PWAD, got ${JSON.stringify(ident)}`);
  }
  const lumpCount = view.getInt32(4, true);
  const tableOffset = view.getInt32(8, true);
  if (lumpCount < 0 || tableOffset < 0 || tableOffset + lumpCount * 16 > buffer.byteLength) {
    throw new Error(`WAD header out of bounds: count=${lumpCount} offset=${tableOffset}`);
  }

  const entries: DoomWadDirectoryEntry[] = [];
  for (let i = 0; i < lumpCount; i += 1) {
    const pos = tableOffset + i * 16;
    const filePosition = view.getInt32(pos, true);
    const size = view.getInt32(pos + 4, true);
    const name = decodeAscii(buffer.slice(pos + 8, pos + 16)).replace(/\0+$/, "").trim();
    if (filePosition < 0 || size < 0 || filePosition + size > buffer.byteLength) {
      throw new Error(`lump ${name} out of bounds: pos=${filePosition} size=${size}`);
    }
    entries.push({ name, filePosition, size });
  }

  let sha256: string | undefined;
  if (opts?.computeSha256) {
    sha256 = createHash("sha256").update(Buffer.from(buffer)).digest("hex");
  }

  return {
    identification: ident,
    entries,
    sha256,
    byteLength: buffer.byteLength,
  };
}

export function findE1M1Label(entries: readonly DoomWadDirectoryEntry[]): number {
  const idx = entries.findIndex((e) => e.name === "E1M1" && e.size === 0);
  if (idx === -1) throw new Error("E1M1 label lump not found (expected name=E1M1 size=0)");
  // ensure payload lumps exist
  for (let offset = 0; offset < MAP_PAYLOAD.length; offset += 1) {
    const expected = MAP_PAYLOAD[offset]!;
    const actual = entries[idx + 1 + offset]?.name;
    if (actual !== expected) {
      throw new Error(`E1M1 payload mismatch at +${1 + offset}: expected ${expected} got ${actual ?? "EOF"}`);
    }
  }
  return idx;
}

export function decodeE1M1Level(buffer: ArrayBuffer, entries: readonly DoomWadDirectoryEntry[], labelIndex: number): DoomE1M1Level {
  const slice = (entry: DoomWadDirectoryEntry): ArrayBuffer => buffer.slice(entry.filePosition, entry.filePosition + entry.size);

  const thingsEntry = entries[labelIndex + 1]!;
  const linedefsEntry = entries[labelIndex + 2]!;
  const sidedefsEntry = entries[labelIndex + 3]!;
  const vertexesEntry = entries[labelIndex + 4]!;
  const sectorsEntry = entries[labelIndex + 8]!; // Sectors is 8th after label per MAP_PAYLOAD order

  return {
    mapName: "E1M1",
    vertices: decodeVertices(slice(vertexesEntry)),
    things: decodeThings(slice(thingsEntry)),
    linedefs: decodeLinedefs(slice(linedefsEntry)),
    sidedefs: decodeSidedefs(slice(sidedefsEntry)),
    sectors: decodeSectors(slice(sectorsEntry)),
    lumpIndex: labelIndex,
  };
}

export function decodeVertices(buffer: ArrayBuffer): WadVertex[] {
  if (buffer.byteLength % 4 !== 0) throw new Error(`VERTEXES length ${buffer.byteLength} not multiple of 4`);
  const view = new DataView(buffer);
  const out: WadVertex[] = [];
  for (let off = 0; off < buffer.byteLength; off += 4) {
    out.push({ x: view.getInt16(off, true), y: view.getInt16(off + 2, true) });
  }
  return out;
}

export function decodeThings(buffer: ArrayBuffer): WadThing[] {
  if (buffer.byteLength % 10 !== 0) throw new Error(`THINGS length ${buffer.byteLength} not multiple of 10`);
  const view = new DataView(buffer);
  const out: WadThing[] = [];
  for (let off = 0; off < buffer.byteLength; off += 10) {
    out.push({
      x: view.getInt16(off, true),
      y: view.getInt16(off + 2, true),
      angle: view.getInt16(off + 4, true),
      type: view.getInt16(off + 6, true),
      options: view.getInt16(off + 8, true),
    });
  }
  return out;
}

export function decodeLinedefs(buffer: ArrayBuffer): WadLinedef[] {
  if (buffer.byteLength % 14 !== 0) throw new Error(`LINEDEFS length ${buffer.byteLength} not multiple of 14`);
  const view = new DataView(buffer);
  const out: WadLinedef[] = [];
  for (let off = 0; off < buffer.byteLength; off += 14) {
    out.push({
      startVertex: view.getUint16(off, true),
      endVertex: view.getUint16(off + 2, true),
      flags: view.getUint16(off + 4, true),
      lineType: view.getUint16(off + 6, true),
      sectorTag: view.getUint16(off + 8, true),
      frontSidedef: view.getUint16(off + 10, true),
      backSidedef: view.getInt16(off + 12, true), // -1 when absent (0xFFFF)
    });
  }
  return out;
}

export function decodeSidedefs(buffer: ArrayBuffer): WadSidedef[] {
  if (buffer.byteLength % 30 !== 0) throw new Error(`SIDEDEFS length ${buffer.byteLength} not multiple of 30`);
  const view = new DataView(buffer);
  const out: WadSidedef[] = [];
  for (let off = 0; off < buffer.byteLength; off += 30) {
    out.push({
      xOffset: view.getInt16(off, true),
      yOffset: view.getInt16(off + 2, true),
      upperTexture: decodeTextureName(buffer.slice(off + 4, off + 12)),
      lowerTexture: decodeTextureName(buffer.slice(off + 12, off + 20)),
      middleTexture: decodeTextureName(buffer.slice(off + 20, off + 28)),
      sector: view.getInt16(off + 28, true),
    });
  }
  return out;
}

export function decodeSectors(buffer: ArrayBuffer): WadSector[] {
  if (buffer.byteLength % 26 !== 0) throw new Error(`SECTORS length ${buffer.byteLength} not multiple of 26`);
  const view = new DataView(buffer);
  const out: WadSector[] = [];
  for (let off = 0; off < buffer.byteLength; off += 26) {
    out.push({
      floorHeight: view.getInt16(off, true),
      ceilingHeight: view.getInt16(off + 2, true),
      floorTexture: decodeTextureName(buffer.slice(off + 4, off + 12)),
      ceilingTexture: decodeTextureName(buffer.slice(off + 12, off + 20)),
      lightLevel: view.getInt16(off + 20, true),
      special: view.getInt16(off + 22, true),
      tag: view.getInt16(off + 24, true),
    });
  }
  return out;
}

export function buildE1M1Intermediate(
  buffer: ArrayBuffer,
  wadPath: string,
  opts?: { computeSha256?: boolean },
): E1M1Intermediate {
  const wad = decodeWad(buffer, { computeSha256: true });
  const sha = wad.sha256 ?? createHash("sha256").update(Buffer.from(buffer)).digest("hex");
  const e1m1Idx = findE1M1Label(wad.entries);
  const level = decodeE1M1Level(buffer, wad.entries, e1m1Idx);

  const wallNames = new Set<string>();
  for (const sd of level.sidedefs) {
    for (const n of [sd.upperTexture, sd.lowerTexture, sd.middleTexture]) {
      if (n.length > 0 && n !== "-") wallNames.add(n);
    }
  }
  const flatNames = new Set<string>();
  for (const s of level.sectors) {
    flatNames.add(s.floorTexture);
    flatNames.add(s.ceilingTexture);
  }

  const xs = level.vertices.map((v) => v.x);
  const ys = level.vertices.map((v) => v.y);
  const minX = xs.length ? Math.min(...xs) : 0;
  const maxX = xs.length ? Math.max(...xs) : 0;
  const minY = ys.length ? Math.min(...ys) : 0;
  const maxY = ys.length ? Math.max(...ys) : 0;

  const floors = level.sectors.map((s) => s.floorHeight);
  const ceilings = level.sectors.map((s) => s.ceilingHeight);

  return {
    source: {
      wadPath,
      wadSha256: sha,
      wadByteLength: buffer.byteLength,
      wadIdentification: wad.identification,
      entryCount: wad.entries.length,
      e1m1LumpIndex: e1m1Idx,
    },
    level,
    diagnostics: {
      vertexBounds: { minX, maxX, minY, maxY },
      sectorFloorCeilingRange: {
        minFloor: floors.length ? Math.min(...floors) : 0,
        maxCeiling: ceilings.length ? Math.max(...ceilings) : 0,
      },
      textureIncidence: {
        wallTextureNames: [...wallNames].sort(),
        flatTextureNames: [...flatNames].sort(),
      },
    },
  };
}

export function serializeIntermediate(intermediate: E1M1Intermediate): string {
  // Deterministic emit: sort keys already stable; use 2-space indent + trailing newline (like project-content).
  const json: E1M1Json = intermediate;
  return `${JSON.stringify(json, null, 2)}\n`;
}

export function decodeWadFromFile(path: string): { buffer: ArrayBuffer; wad: DoomWadInfo } {
  const bytes = readFileSync(path);
  const ab = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
  const wad = decodeWad(ab, { computeSha256: true });
  return { buffer: ab, wad };
}

function decodeAscii(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let out = "";
  for (const b of bytes) {
    if (b === 0) break;
    out += String.fromCharCode(b);
  }
  return out;
}

function decodeTextureName(buf: ArrayBuffer): string {
  // 8-byte null/space-padded name; "-" sentinel means no texture; trim correctly.
  const raw = decodeAscii(buf).trim();
  // Some entries may contain trailing \0 but decodeAscii already stops at 0; still trim spaces.
  // Normalize empty to "" and keep "-" exactly.
  if (raw === "" || raw === "\0") return "";
  return raw;
}
