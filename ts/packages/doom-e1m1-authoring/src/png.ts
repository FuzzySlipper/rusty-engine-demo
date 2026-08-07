import { deflateSync } from "node:zlib";
import { createHash } from "node:crypto";

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = (c & 1) ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

export function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const b of bytes) crc = CRC_TABLE[(crc ^ b) & 0xff]! ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function u32be(value: number): Uint8Array {
  const out = new Uint8Array(4);
  out[0] = (value >>> 24) & 0xff;
  out[1] = (value >>> 16) & 0xff;
  out[2] = (value >>> 8) & 0xff;
  out[3] = value & 0xff;
  return out;
}

function concatBytes(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((s, c) => s + c.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) {
    out.set(c, off);
    off += c.length;
  }
  return out;
}

function pngChunk(type: string, data: Uint8Array): Uint8Array {
  const typeBytes = new TextEncoder().encode(type);
  const length = u32be(data.length);
  const crcInput = concatBytes([typeBytes, data]);
  const crc = u32be(crc32(crcInput));
  return concatBytes([length, typeBytes, data, crc]);
}

/**
 * Encode RGBA8 straight-alpha, non-interlaced, 8-bit depth, truecolor+alpha (colorType 6).
 * sRGB is the working space; caller provides sRGB bytes directly (Doom PLAYPAL is sRGB-ish).
 */
export function encodePngRgba8(width: number, height: number, rgba: Uint8Array): Uint8Array {
  if (width <= 0 || height <= 0) throw new Error(`PNG dimensions must be positive, got ${width}x${height}`);
  if (rgba.length !== width * height * 4) throw new Error(`RGBA length ${rgba.length} != ${width}*${height}*4`);
  if (width > 4096 || height > 4096) throw new Error(`PNG dimensions exceed VTX budget guard ${width}x${height}`);
  // Build scanlines: filter byte 0 + RGBA per pixel.
  const stride = width * 4 + 1;
  const raw = new Uint8Array(height * stride);
  for (let y = 0; y < height; y += 1) {
    raw[y * stride] = 0; // None filter
    raw.set(rgba.subarray(y * width * 4, (y + 1) * width * 4), y * stride + 1);
  }
  const compressed = deflateSync(raw);
  const ihdr = new Uint8Array(13);
  const view = new DataView(ihdr.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  ihdr[10] = 0; // compression
  ihdr[11] = 0; // filter
  ihdr[12] = 0; // interlace none

  const signature = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const idat = pngChunk("IDAT", compressed);
  return concatBytes([signature, pngChunk("IHDR", ihdr), idat, pngChunk("IEND", new Uint8Array(0))]);
}

export function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}
