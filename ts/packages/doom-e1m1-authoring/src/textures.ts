import { createHash } from "node:crypto";

import { encodePngRgba8, sha256Hex } from "./png.js";
import type { DoomWadDirectoryEntry } from "./types.js";

// PLAYPAL is 14 palettes of 768 bytes (256*3), but we only use the first 768 for flats/walls.
export type Palette = readonly [number, number, number][]; // 256 entries sRGB

export function decodePalette(playPalBytes: Uint8Array): Palette {
  if (playPalBytes.length < 768) throw new Error(`PLAYPAL too short: ${playPalBytes.length}`);
  const pal: [number, number, number][] = [];
  for (let i = 0; i < 256; i += 1) {
    pal.push([playPalBytes[i * 3]!, playPalBytes[i * 3 + 1]!, playPalBytes[i * 3 + 2]!]);
  }
  return pal;
}

/**
 * Decode a 64x64 flat (4096 bytes indexed) to RGBA8 using palette.
 * All indices become opaque (alpha 255). Use with encodePngRgba8 for sRGB straight-alpha.
 */
export function decodeFlatToRgba(flatBytes: Uint8Array, palette: Palette): Uint8Array {
  if (flatBytes.length !== 4096) throw new Error(`flat must be 4096 bytes, got ${flatBytes.length}`);
  const rgba = new Uint8Array(4096 * 4);
  for (let i = 0; i < 4096; i += 1) {
    const idx = flatBytes[i]!;
    const [r, g, b] = palette[idx] ?? [0, 0, 0];
    rgba[i * 4] = r!;
    rgba[i * 4 + 1] = g!;
    rgba[i * 4 + 2] = b!;
    rgba[i * 4 + 3] = 255;
  }
  return rgba;
}

export function flatToPng(flatBytes: Uint8Array, palette: Palette): Uint8Array {
  return encodePngRgba8(64, 64, decodeFlatToRgba(flatBytes, palette));
}

// --- Patch decode (Doom column/post format) ---

export interface DoomPatch {
  readonly width: number;
  readonly height: number;
  readonly leftOffset: number;
  readonly topOffset: number;
  /** columnData holds the patch bytes from columnOfs[column] to next columnOfs or end; parsed lazily */
  readonly raw: Uint8Array;
  readonly columnOffsets: readonly number[];
}

export function decodePatch(raw: Uint8Array): DoomPatch {
  if (raw.length < 8) throw new Error(`patch too short ${raw.length}`);
  const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
  const width = view.getInt16(0, true);
  const height = view.getInt16(2, true);
  const leftOffset = view.getInt16(4, true);
  const topOffset = view.getInt16(6, true);
  if (width <= 0 || height <= 0) throw new Error(`patch bad dimensions ${width}x${height}`);
  if (raw.length < 8 + width * 4) throw new Error(`patch header truncated ${raw.length} for width ${width}`);
  const columnOffsets: number[] = [];
  for (let c = 0; c < width; c += 1) columnOffsets.push(view.getUint32(8 + c * 4, true));
  // validate offsets within bounds and sorted
  for (let c = 0; c < width; c += 1) {
    const off = columnOffsets[c]!;
    if (off < 8 + width * 4 || off >= raw.length) throw new Error(`patch column ${c} offset ${off} out of bounds`);
  }
  return { width, height, leftOffset, topOffset, raw, columnOffsets };
}

/**
 * Blit one patch's columns into an indexed canvas at origin (originX, originY).
 * Canvas is width*height indexed bytes, initialized with 255 = transparent sentinel (not a valid palette index for opaque? but we keep transparent separately).
 * Uses the post traversal: each column is sequence of posts {topDelta, length, [padding], bytes[length], [padding]}, terminated by 0xFF topDelta.
 * `drawColumnInCache` style clipping: `position = originY + topDelta`, clipped to [0, canvasHeight).
 */
export function blitPatchIndexed(
  canvasIndexed: Uint8Array,
  canvasWidth: number,
  canvasHeight: number,
  patch: DoomPatch,
  originX: number,
  originY: number,
): void {
  for (let col = 0; col < patch.width; col += 1) {
    const destX = originX + col;
    if (destX < 0 || destX >= canvasWidth) continue;
    const colOfs = patch.columnOffsets[col]!;
    let offset = colOfs;
    // Walk posts
    while (offset < patch.raw.length) {
      const topDelta = patch.raw[offset]!;
      if (topDelta === 0xff) break;
      if (offset + 1 >= patch.raw.length) break;
      const length = patch.raw[offset + 1]!;
      // bytes are at offset+3 .. offset+3+length-1, with dummy bytes at offset+2 and offset+3+length
      const srcStart = offset + 3;
      const srcEnd = srcStart + length;
      if (srcEnd > patch.raw.length) break;
      let position = originY + topDelta;
      let srcPos = 0;
      let count = length;
      if (position < 0) {
        srcPos -= position;
        count += position;
        position = 0;
      }
      if (position + count > canvasHeight) count = canvasHeight - position;
      if (count > 0) {
        for (let i = 0; i < count; i += 1) {
          const canvasY = position + i;
          const canvasIdx = canvasY * canvasWidth + destX;
          const srcByte = patch.raw[srcStart + srcPos + i]!;
          canvasIndexed[canvasIdx] = srcByte;
        }
      }
      offset = srcEnd + 1; // skip dummy after bytes
      // Next post may be preceded by a dummy? Actually post padding is 1 byte after bytes, then next post topDelta.
      // The loop handles that as offset already beyond dummy.
      // However some patches have a final 0xFF at offset.
      if (offset >= patch.raw.length) break;
      // If the next byte is not a valid topDelta and we are mid, we continue; the trailing dummy is consumed.
    }
  }
}

// --- PNAMES / TEXTURE1 parsing ---

export function decodePnames(bytes: Uint8Array): string[] {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const num = view.getInt32(0, true);
  if (num < 0 || 4 + num * 8 > bytes.length) throw new Error(`PNAMES bad num ${num}`);
  const out: string[] = [];
  for (let i = 0; i < num; i += 1) {
    out.push(decodeName(bytes.subarray(4 + i * 8, 4 + i * 8 + 8)));
  }
  return out;
}

export interface DoomTextureDef {
  readonly name: string;
  readonly width: number;
  readonly height: number;
  readonly patches: readonly { readonly originX: number; readonly originY: number; readonly patchName: string }[];
}

export function decodeTexture1(bytes: Uint8Array, pnames: readonly string[]): DoomTextureDef[] {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const num = view.getInt32(0, true);
  if (num < 0 || 4 + num * 4 > bytes.length) throw new Error(`TEXTURE1 bad num ${num}`);
  const offsets: number[] = [];
  for (let i = 0; i < num; i += 1) offsets.push(view.getInt32(4 + i * 4, true));
  const out: DoomTextureDef[] = [];
  for (const off of offsets) {
    if (off < 0 || off + 22 > bytes.length) throw new Error(`TEXTURE1 texture offset ${off} oob`);
    const name = decodeName(bytes.subarray(off, off + 8));
    // int32 masked at off+8, we ignore
    const width = view.getInt16(off + 12, true);
    const height = view.getInt16(off + 14, true);
    const patchCount = view.getInt16(off + 20, true);
    if (width <= 0 || height <= 0) throw new Error(`texture ${name} bad dims ${width}x${height}`);
    if (patchCount < 0 || off + 22 + patchCount * 10 > bytes.length) throw new Error(`texture ${name} patchCount ${patchCount} oob`);
    const patches = [] as { originX: number; originY: number; patchName: string }[];
    for (let p = 0; p < patchCount; p += 1) {
      const poff = off + 22 + p * 10;
      const originX = view.getInt16(poff, true);
      const originY = view.getInt16(poff + 2, true);
      const patchIdx = view.getInt16(poff + 4, true);
      // stepDir and colormap ignored
      if (patchIdx < 0 || patchIdx >= pnames.length) throw new Error(`texture ${name} patch idx ${patchIdx} oob pnames ${pnames.length}`);
      patches.push({ originX, originY, patchName: pnames[patchIdx]! });
    }
    out.push({ name, width, height, patches });
  }
  return out;
}

export function compositeTextureToIndexed(
  def: DoomTextureDef,
  patchByName: Map<string, DoomPatch>,
): Uint8Array {
  // Canvas indexed, initialized to 255 = transparent; will later treat 255 as alpha 0 if never overwritten.
  const indexed = new Uint8Array(def.width * def.height);
  indexed.fill(255);
  for (const p of def.patches) {
    const patch = patchByName.get(p.patchName) ?? patchByName.get(p.patchName.toUpperCase());
    if (!patch) throw new Error(`missing patch ${p.patchName} for texture ${def.name}`);
    blitPatchIndexed(indexed, def.width, def.height, patch, p.originX, p.originY);
  }
  return indexed;
}

export function indexedToRgba(indexed: Uint8Array, palette: Palette, transparentIndex: number = 255): Uint8Array {
  const rgba = new Uint8Array(indexed.length * 4);
  for (let i = 0; i < indexed.length; i += 1) {
    const idx = indexed[i]!;
    if (idx === transparentIndex) {
      rgba[i * 4] = 0;
      rgba[i * 4 + 1] = 0;
      rgba[i * 4 + 2] = 0;
      rgba[i * 4 + 3] = 0;
    } else {
      const [r, g, b] = palette[idx] ?? [0, 0, 0];
      rgba[i * 4] = r!;
      rgba[i * 4 + 1] = g!;
      rgba[i * 4 + 2] = b!;
      rgba[i * 4 + 3] = 255;
    }
  }
  return rgba;
}

export function textureToPngBytes(def: DoomTextureDef, patchByName: Map<string, DoomPatch>, palette: Palette): Uint8Array {
  const indexed = compositeTextureToIndexed(def, patchByName);
  const rgba = indexedToRgba(indexed, palette);
  return encodePngRgba8(def.width, def.height, rgba);
}

// utility
function decodeName(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) {
    if (b === 0) break;
    s += String.fromCharCode(b);
  }
  return s.trim();
}

export function wadLumpBytes(buffer: ArrayBuffer, entry: DoomWadDirectoryEntry): Uint8Array {
  return new Uint8Array(buffer, entry.filePosition, entry.size);
}

export function directoryByName(entries: readonly DoomWadDirectoryEntry[]): Map<string, DoomWadDirectoryEntry> {
  const m = new Map<string, DoomWadDirectoryEntry>();
  for (const e of entries) m.set(e.name.toUpperCase(), e);
  return m;
}

export function hashBytes(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}
