import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  buildE1M1Intermediate,
  decodeLinedefs,
  decodeSectors,
  decodeSidedefs,
  decodeThings,
  decodeVertices,
  decodeWad,
  findE1M1Label,
} from "./wad-decode.js";

const WAD_PATH = "/home/research/doom.ts/public/doom1.wad";
const EXPECTED_SHA = "1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771";
const EXPECTED_WALL_TEXTURES = [
  "BIGDOOR2",
  "BIGDOOR4",
  "BRNBIGC",
  "BRNBIGL",
  "BRNBIGR",
  "BROWN1",
  "BROWN144",
  "BROWN96",
  "BROWNGRN",
  "COMPSPAN",
  "COMPTALL",
  "COMPTILE",
  "COMPUTE2",
  "DOOR3",
  "DOORSTOP",
  "DOORTRAK",
  "EXITDOOR",
  "EXITSIGN",
  "LITE3",
  "NUKE24",
  "PLANET1",
  "SLADWALL",
  "STARG3",
  "STARGR1",
  "STARTAN1",
  "STARTAN3",
  "STEP1",
  "STEP6",
  "SUPPORT2",
  "SW1STRTN",
  "TEKWALL1",
  "TEKWALL4",
] as const;
const EXPECTED_FLATS = [
  "CEIL3_5",
  "CEIL5_1",
  "CEIL5_2",
  "FLAT14",
  "FLAT18",
  "FLAT20",
  "FLAT23",
  "FLAT5_5",
  "FLOOR1_1",
  "FLOOR4_8",
  "FLOOR5_1",
  "FLOOR5_2",
  "FLOOR6_2",
  "FLOOR7_1",
  "FLOOR7_2",
  "F_SKY1",
  "NUKAGE3",
  "STEP2",
  "TLITE6_1",
  "TLITE6_4",
  "TLITE6_5",
  "TLITE6_6",
] as const;

function loadWadBuffer(): ArrayBuffer {
  const bytes = readFileSync(WAD_PATH);
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

test("WAD header decodes as IWAD 1264 lumps and expected SHA", () => {
  const buf = loadWadBuffer();
  const wad = decodeWad(buf, { computeSha256: true });
  assert.equal(wad.identification, "IWAD");
  assert.equal(wad.entries.length, 1264);
  assert.equal(wad.sha256, EXPECTED_SHA);
  assert.equal(wad.byteLength, 4196020);
});

test("E1M1 payload counts match plan §3 invariants", () => {
  const buf = loadWadBuffer();
  const inter = buildE1M1Intermediate(buf, WAD_PATH);
  assert.equal(inter.source.wadSha256, EXPECTED_SHA);
  assert.equal(inter.source.wadIdentification, "IWAD");
  assert.equal(inter.level.mapName, "E1M1");
  assert.equal(inter.level.vertices.length, 467);
  assert.equal(inter.level.things.length, 138);
  assert.equal(inter.level.linedefs.length, 475);
  assert.equal(inter.level.sidedefs.length, 648);
  assert.equal(inter.level.sectors.length, 85);
  assert.equal(inter.source.e1m1LumpIndex, 6);
});

test("vertex bounds match decoded doom1.wad (-768..3808 / -4864..-2048)", () => {
  const inter = buildE1M1Intermediate(loadWadBuffer(), WAD_PATH);
  assert.deepEqual(inter.diagnostics.vertexBounds, { minX: -768, maxX: 3808, minY: -4864, maxY: -2048 });
});

test("sector height range matches WAD (-136..264)", () => {
  const inter = buildE1M1Intermediate(loadWadBuffer(), WAD_PATH);
  assert.deepEqual(inter.diagnostics.sectorFloorCeilingRange, { minFloor: -136, maxCeiling: 264 });
});

test("texture incidence matches extracted sets (32 walls, 22 flats)", () => {
  const inter = buildE1M1Intermediate(loadWadBuffer(), WAD_PATH);
  assert.equal(inter.diagnostics.textureIncidence.wallTextureNames.length, 32);
  assert.equal(inter.diagnostics.textureIncidence.flatTextureNames.length, 22);
  assert.deepEqual(inter.diagnostics.textureIncidence.wallTextureNames, [...EXPECTED_WALL_TEXTURES]);
  assert.deepEqual(inter.diagnostics.textureIncidence.flatTextureNames, [...EXPECTED_FLATS]);
});

test("sidedef sentinel and sector light invariants", () => {
  const inter = buildE1M1Intermediate(loadWadBuffer(), WAD_PATH);
  // At least one linedef should be single-sided (backSidedef === -1)
  assert.ok(inter.level.linedefs.some((ld) => ld.backSidedef === -1));
  // All sector light levels within 0..255
  for (const s of inter.level.sectors) {
    assert.ok(s.lightLevel >= 0 && s.lightLevel <= 255);
  }
  // Sidedef sector indices within 0..84
  for (const sd of inter.level.sidedefs) {
    assert.ok(sd.sector >= 0 && sd.sector < 85);
  }
});

test("decode rejects malformed WAD headers", () => {
  assert.throws(() => decodeWad(new ArrayBuffer(0)), /WAD too short/);
  const bad = new ArrayBuffer(12);
  new Uint8Array(bad).set([0x42, 0x41, 0x44, 0x21]); // BAD!
  assert.throws(() => decodeWad(bad), /IWAD or PWAD/);
});

test("decode helpers reject misaligned buffers", () => {
  assert.throws(() => decodeVertices(new ArrayBuffer(3)), /multiple of 4/);
  assert.throws(() => decodeThings(new ArrayBuffer(9)), /multiple of 10/);
  assert.throws(() => decodeLinedefs(new ArrayBuffer(13)), /multiple of 14/);
  assert.throws(() => decodeSidedefs(new ArrayBuffer(29)), /multiple of 30/);
  assert.throws(() => decodeSectors(new ArrayBuffer(25)), /multiple of 26/);
});

test("findE1M1Label requires exact payload order", () => {
  const wad = decodeWad(loadWadBuffer());
  assert.equal(findE1M1Label(wad.entries), 6);
  // truncated order — still an E1M1 label but missing payload lumps → payload mismatch
  assert.throws(() => findE1M1Label(wad.entries.slice(0, 10) as never), /payload mismatch/);
});

test("deterministic serialize produces stable JSON", async () => {
  const { serializeIntermediate } = await import("./wad-decode.js");
  const inter = buildE1M1Intermediate(loadWadBuffer(), WAD_PATH);
  const a = serializeIntermediate(inter);
  const b = serializeIntermediate(inter);
  assert.equal(a, b);
  assert.ok(a.endsWith("\n"));
  // round-trip JSON parse retains counts
  const parsed = JSON.parse(a) as ReturnType<typeof buildE1M1Intermediate>;
  assert.equal(parsed.level.vertices.length, 467);
});
