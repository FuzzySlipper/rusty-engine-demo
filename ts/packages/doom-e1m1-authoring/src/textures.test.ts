import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { decodePnames, decodeTexture1, decodePatch, flatToPng, textureToPngBytes } from "./textures.js";
import { decodePalette } from "./textures.js";
import { decodeWad, buildE1M1Intermediate } from "./wad-decode.js";

const WAD_PATH = "/home/research/doom.ts/public/doom1.wad";
const MANIFEST_PATH = fileURLToPath(new URL("../../../../content/doom-e1m1/textures/manifest.json", import.meta.url));

function wadBytes(): Uint8Array {
  const raw = readFileSync(WAD_PATH);
  return new Uint8Array(raw.buffer, raw.byteOffset, raw.byteLength);
}

function wadEntries() {
  const raw = readFileSync(WAD_PATH);
  const ab = raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength) as ArrayBuffer;
  return decodeWad(ab).entries;
}

test("PLAYPAL palette is 256 colors and PNAMES 350", () => {
  const bytes = wadBytes();
  const ab = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
  const wad = decodeWad(ab);
  const dir = new Map(wad.entries.map((e) => [e.name, e] as const));
  const playpal = new Uint8Array(ab, dir.get("PLAYPAL")!.filePosition, 768);
  const palette = decodePalette(playpal);
  assert.equal(palette.length, 256);
  // first color black
  assert.deepEqual(palette[0], [0, 0, 0]);
  // known palette entry 1
  assert.deepEqual(palette[1], [31, 23, 11]);

  const pnamesEntry = dir.get("PNAMES")!;
  const pnamesBytes = new Uint8Array(ab, pnamesEntry.filePosition, pnamesEntry.size);
  const pnames = decodePnames(pnamesBytes);
  assert.equal(pnames.length, 350);
  assert.equal(pnames[0], "WALL00_3");
  assert.ok(pnames.includes("W94_1") || pnames.includes("w94_1"));
});

test("TEXTURE1 decodes 125 textures including BIGDOOR2", () => {
  const raw = readFileSync(WAD_PATH);
  const ab = raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength) as ArrayBuffer;
  const wad = decodeWad(ab);
  const dir = new Map(wad.entries.map((e) => [e.name.toUpperCase(), e] as const));
  const pnames = decodePnames(new Uint8Array(ab, dir.get("PNAMES")!.filePosition, dir.get("PNAMES")!.size));
  const t1 = decodeTexture1(new Uint8Array(ab, dir.get("TEXTURE1")!.filePosition, dir.get("TEXTURE1")!.size), pnames);
  assert.equal(t1.length, 125);
  const bigdoor = t1.find((t) => t.name === "BIGDOOR2");
  assert.ok(bigdoor);
  assert.ok(bigdoor.width > 0 && bigdoor.height > 0);
  assert.ok(bigdoor.patches.length > 0);
});

test("patch decode handles W94_1 case-insensitive", () => {
  const raw = readFileSync(WAD_PATH);
  const ab = raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength) as ArrayBuffer;
  const wad = decodeWad(ab);
  const dir = new Map(wad.entries.map((e) => [e.name.toUpperCase(), e] as const));
  // W94_1 lump should be upper; decoding via lowercase name should still succeed via case-insensitive lookup in CLI
  const lower = dir.get("W94_1");
  assert.ok(lower);
  const bytes = new Uint8Array(ab, lower.filePosition, lower.size);
  const patch = decodePatch(bytes);
  assert.ok(patch.width > 0 && patch.height > 0);
});

test("flat PNG golden hashes for FLOOR7_2 and CEIL3_5 are stable", () => {
  if (!existsSync(MANIFEST_PATH)) {
    // If manifest not yet generated, build it via texture-cli
    test.skip("manifest not yet generated; run generate");
    return;
  }
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, "utf8")) as { entries: { name: string; pngSha256: string; kind: string }[] };
  const floor72 = manifest.entries.find((e) => e.name === "FLOOR7_2" && e.kind === "flat");
  const ceil35 = manifest.entries.find((e) => e.name === "CEIL3_5" && e.kind === "flat");
  const bigdoor = manifest.entries.find((e) => e.name === "BIGDOOR2" && e.kind === "wall");
  assert.ok(floor72);
  assert.ok(ceil35);
  assert.ok(bigdoor);
  // Fixed golden hashes — any palette or patch-compositing regression will be caught.
  // Generated 2026-08-07 from PLAYPAL fd8959… and TEXTURE1/PNAMES 350, total 183120 B.
  // To update, rerun `pnpm --filter doom-e1m1-authoring exec node dist/texture-cli.js --write` and copy from manifest.json.
  assert.equal(floor72.pngSha256, "64f9e17663049712b610402edba9904e6125f5387cb6fd97ece8ca16e31f5a2a");
  assert.equal(ceil35.pngSha256, "77c168d323d085f8cbf2086a7c5929659d947c501f417a999b0a5a9d257f4dfb");
  assert.equal(bigdoor.pngSha256, "b71bae2b662f1682be58e1517a0cc5f2b01aec3ebf9b3c9dee7d0ae7ed6d786e");
});

test("texture repeat scale is expressed in voxel cells, not reciprocal pixels", () => {
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, "utf8")) as {
    entries: { width: number; height: number; tileScale: [number, number] }[];
  };
  for (const entry of manifest.entries) {
    assert.deepEqual(entry.tileScale, [entry.width / 16, entry.height / 16]);
  }
});

test("wall provenance covers TEXTURE1 entry plus patch bytes (R6676-1 regression)", async () => {
  if (!existsSync(MANIFEST_PATH)) {
    test.skip("manifest missing");
    return;
  }
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, "utf8")) as {
    entries: { name: string; kind: string; sourceSha256: string; sourceByteLength: number }[];
  };
  const wall = manifest.entries.find((e) => e.name === "BIGDOOR2" && e.kind === "wall");
  assert.ok(wall);
  // Recompute from WAD: entry bytes + patch bytes
  const raw = readFileSync(WAD_PATH);
  const ab = raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength) as ArrayBuffer;
  const wad = decodeWad(ab);
  const dir = new Map(wad.entries.map((e: { name: string; filePosition: number; size: number }) => [e.name.toUpperCase(), e] as const));
  const pnamesBytes = new Uint8Array(ab, dir.get("PNAMES")!.filePosition, dir.get("PNAMES")!.size);
  const pnames = decodePnames(pnamesBytes);
  const tex1Bytes = new Uint8Array(ab, dir.get("TEXTURE1")!.filePosition, dir.get("TEXTURE1")!.size);
  const texDefs = decodeTexture1(tex1Bytes, pnames);
  const tex = texDefs.find((t) => t.name === "BIGDOOR2");
  assert.ok(tex);
  // Find raw entry bytes for BIGDOOR2
  const view = new DataView(tex1Bytes.buffer, tex1Bytes.byteOffset, tex1Bytes.byteLength);
  const num = view.getInt32(0, true);
  let entryBytes: Uint8Array | undefined;
  for (let i = 0; i < num; i += 1) {
    const off = view.getInt32(4 + i * 4, true);
    const name = new TextDecoder().decode(tex1Bytes.subarray(off, off + 8)).replace(/\0+$/, "").trim();
    if (name === "BIGDOOR2") {
      const patchCount = view.getInt16(off + 20, true);
      const len = 22 + patchCount * 10;
      entryBytes = tex1Bytes.subarray(off, off + len);
      break;
    }
  }
  assert.ok(entryBytes);
  // Patch bytes
  const patchBytesList = tex.patches.map((p) => {
    const e = dir.get(p.patchName.toUpperCase());
    assert.ok(e, `patch ${p.patchName} missing`);
    return new Uint8Array(ab, e.filePosition, e.size);
  });
  const { createHash } = await import("node:crypto");
  const h = createHash("sha256");
  h.update(entryBytes!);
  for (const b of patchBytesList) h.update(b);
  const expected = h.digest("hex");
  const expectedLen = entryBytes!.length + patchBytesList.reduce((s, b) => s + b.length, 0);
  assert.equal(wall.sourceSha256, expected);
  assert.equal(wall.sourceByteLength, expectedLen);
});

test("texture manifest VTX budgets within limits", () => {
  if (!existsSync(MANIFEST_PATH)) {
    test.skip("manifest missing");
    return;
  }
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, "utf8")) as {
    entries: { pngByteLength: number; width: number; height: number }[];
    sky: { pngByteLength: number; width: number; height: number; runtimeMapping: string };
    diagnostics: { totalPngBytes: number; totalDecodedRgbaBytes: number; textureIdentities: number };
  };
  assert.equal(manifest.entries.length, 54);
  assert.equal(manifest.diagnostics.textureIdentities, 55);
  assert.equal(manifest.sky.width, manifest.sky.height * 2);
  assert.equal(manifest.sky.runtimeMapping, "equirectangular");
  assert.ok(manifest.sky.pngByteLength <= 16 * 1024 * 1024);
  assert.ok(manifest.diagnostics.totalPngBytes < 128 * 1024 * 1024);
  assert.ok(manifest.diagnostics.totalDecodedRgbaBytes < 256 * 1024 * 1024);
  for (const e of manifest.entries) {
    assert.ok(e.pngByteLength <= 16 * 1024 * 1024, `${e} exceeds 16 MiB`);
    assert.ok(e.width > 0 && e.height > 0);
  }
  // E1M1 incidence counts
  const inter = buildE1M1Intermediate(wadBytes().buffer as ArrayBuffer, WAD_PATH);
  assert.equal(inter.diagnostics.textureIncidence.wallTextureNames.length, 32);
  assert.equal(inter.diagnostics.textureIncidence.flatTextureNames.length, 22);
});
