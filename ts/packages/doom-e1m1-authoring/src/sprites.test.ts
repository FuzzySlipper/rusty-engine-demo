import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { inflateSync } from "node:zlib";

import {
  decodePalette,
  directoryByName,
  hashBytes,
  wadLumpBytes,
} from "./textures.js";
import { decodeWad } from "./wad-decode.js";
import { sha256Hex } from "./png.js";
import {
  decodeSpritePatchToRgba,
  renderSpriteArtifacts,
  selectCanonicalSpriteLumps,
} from "./sprite-extract.js";
import { parseSpriteLumpAssignments } from "./sprite-contract.js";

const WAD_PATH = "/home/research/doom.ts/public/doom1.wad";
const WAD_SHA256 =
  "1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771";
const WAD_BYTE_LENGTH = 4196020;

function loadWad(): ArrayBuffer {
  const bytes = readFileSync(WAD_PATH);
  return bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer;
}

function decodeGeneratedPngRgba(bytes: Uint8Array): {
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8Array;
} {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const width = view.getUint32(16);
  const height = view.getUint32(20);
  const idatChunks: Uint8Array[] = [];
  for (let offset = 8; offset < bytes.length; ) {
    const length = view.getUint32(offset);
    const type = new TextDecoder().decode(
      bytes.subarray(offset + 4, offset + 8),
    );
    if (type === "IDAT")
      idatChunks.push(bytes.subarray(offset + 8, offset + 8 + length));
    offset += 12 + length;
  }
  const compressed = new Uint8Array(
    idatChunks.reduce((sum, chunk) => sum + chunk.length, 0),
  );
  let compressedOffset = 0;
  for (const chunk of idatChunks) {
    compressed.set(chunk, compressedOffset);
    compressedOffset += chunk.length;
  }
  const scanlines = inflateSync(compressed);
  const rowBytes = width * 4;
  const rgba = new Uint8Array(width * height * 4);
  for (let row = 0; row < height; row += 1) {
    assert.equal(
      scanlines[row * (rowBytes + 1)],
      0,
      "generated atlas row must use PNG filter none",
    );
    rgba.set(
      scanlines.subarray(row * (rowBytes + 1) + 1, (row + 1) * (rowBytes + 1)),
      row * rowBytes,
    );
  }
  return { width, height, rgba };
}

test("canonical sprite selection is bounded to the six requested Doom families", () => {
  const wad = decodeWad(loadWad(), { computeSha256: true });
  assert.equal(wad.sha256, WAD_SHA256);
  assert.equal(wad.byteLength, WAD_BYTE_LENGTH);
  const selected = selectCanonicalSpriteLumps(wad.entries);
  const counts = new Map<string, number>();
  for (const lump of selected.lumps)
    counts.set(lump.family.prefix, (counts.get(lump.family.prefix) ?? 0) + 1);
  assert.deepEqual(Object.fromEntries(counts), {
    POSS: 49,
    SPOS: 49,
    TROO: 53,
    BAL1: 5,
    BLUD: 3,
    PUFF: 4,
  });
  assert.equal(selected.lumps[0]?.entry.name, "POSSA1");
  assert.equal(selected.lumps.at(-1)?.entry.name, "PUFFD0");
});

test("known canonical lumps retain exact source bytes and patch dimensions", () => {
  const buffer = loadWad();
  const wad = decodeWad(buffer);
  const selected = selectCanonicalSpriteLumps(wad.entries).lumps;
  const byName = new Map(
    selected.map((lump) => [lump.entry.name, lump.entry] as const),
  );
  const expectations = {
    POSSA1: {
      size: 1392,
      sha256:
        "52667cdb0c9b0b6555eba69c60d62861938328945154f29618faa81a3492b34a",
      width: 41,
      height: 55,
      leftOffset: 18,
      topOffset: 50,
    },
    TROOA1: {
      size: 1632,
      sha256:
        "5e44743944db89f3ee0f9bf48233e517a5b43ce4efa9d886c83b1d844b0cfd74",
      width: 41,
      height: 57,
      leftOffset: 19,
      topOffset: 52,
    },
    BAL1A0: {
      size: 320,
      sha256:
        "997f514abeddc35ff75b53c49f2fec27a55b6e9c17222c91dc3411e6bb6d2892",
      width: 15,
      height: 15,
      leftOffset: 8,
      topOffset: 8,
    },
    PUFFA0: {
      size: 76,
      sha256:
        "ba154601d0ea4daedd3c91bede57f44009b5fb7eea7885389f07eb883da27678",
      width: 5,
      height: 5,
      leftOffset: 2,
      topOffset: 3,
    },
  } as const;
  for (const [name, expected] of Object.entries(expectations)) {
    const entry = byName.get(name);
    assert.ok(entry, `${name} must be selected`);
    assert.equal(entry.size, expected.size, `${name} WAD size`);
    assert.equal(
      hashBytes(wadLumpBytes(buffer, entry)),
      expected.sha256,
      `${name} WAD hash`,
    );
    const patch = decodeSpritePatchToRgba(
      wadLumpBytes(buffer, entry),
      decodePalette(
        wadLumpBytes(buffer, directoryByName(wad.entries).get("PLAYPAL")!),
      ),
    );
    assert.deepEqual(
      {
        width: patch.width,
        height: patch.height,
        leftOffset: patch.leftOffset,
        topOffset: patch.topOffset,
      },
      {
        width: expected.width,
        height: expected.height,
        leftOffset: expected.leftOffset,
        topOffset: expected.topOffset,
      },
    );
  }
});

test("Doom lump suffixes preserve directional coverage and mirror flags", () => {
  assert.deepEqual(parseSpriteLumpAssignments("POSS", "POSSA1"), [
    { frame: "A", rotation: 1, mirrored: false },
  ]);
  assert.deepEqual(parseSpriteLumpAssignments("POSS", "POSSA2A8"), [
    { frame: "A", rotation: 2, mirrored: false },
    { frame: "A", rotation: 8, mirrored: true },
  ]);
  assert.deepEqual(parseSpriteLumpAssignments("BAL1", "BAL1A0"), [
    { frame: "A", rotation: 0, mirrored: false },
  ]);
});

test("generated sprite contract keeps clips, timing, directions, dimensions, and pivots exact", () => {
  const manifest = renderSpriteArtifacts(WAD_PATH).manifest;
  assert.equal(manifest.schemaVersion, 2);
  assert.equal(manifest.contract.tickRateHz, 35);
  assert.deepEqual(manifest.contract.scale, {
    mapDoomUnitsPerEngineUnit: 16,
    actorReferenceHeightDoomUnits: 56,
    actorReferenceHeightEngineUnits: 2,
    presentationDoomUnitsPerEngineUnit: 28,
  });

  const family = (prefix: string) =>
    manifest.contract.families.find((candidate) => candidate.prefix === prefix)!;
  const clip = (prefix: string, id: string) =>
    family(prefix).clips.find((candidate) => candidate.id === id)!;

  assert.deepEqual(
    clip("POSS", "attack").steps.map(({ frame, tics }) => [frame, tics]),
    [["E", 10], ["F", 8], ["E", 8]],
  );
  assert.deepEqual(
    clip("SPOS", "pain").steps.map(({ frame, tics }) => [frame, tics]),
    [["G", 3], ["G", 3]],
  );
  assert.deepEqual(
    clip("TROO", "death").steps.map(({ frame, tics }) => [frame, tics]),
    [["I", 8], ["J", 8], ["K", 6], ["L", 6], ["M", -1]],
  );
  assert.deepEqual(
    clip("BLUD", "hit").steps.map(({ frame, tics }) => [frame, tics]),
    [["C", 8], ["B", 8], ["A", 8]],
  );
  assert.deepEqual(family("POSS").dimensionsDoomUnits, { radius: 20, height: 56 });
  assert.deepEqual(family("BAL1").dimensionsDoomUnits, { radius: 6, height: 8 });

  const possA = family("POSS").directionalFrames.find((frame) => frame.frame === "A")!;
  assert.deepEqual(possA.rotations.map(({ rotation }) => rotation), [1, 2, 3, 4, 5, 6, 7, 8]);
  assert.deepEqual(possA.rotations.map(({ mirrored }) => mirrored), [false, false, false, false, false, true, true, true]);
  const possH = family("POSS").directionalFrames.find((frame) => frame.frame === "H")!;
  assert.deepEqual(possH.rotations.map(({ rotation }) => rotation), [0]);

  const sourceFrame = manifest.atlases[0]!.frames.find((frame) => frame.name === "POSSA1")!;
  assert.deepEqual(sourceFrame.pivot, [18 / 41, 5 / 55]);
  assert.deepEqual(sourceFrame.boundsFromOrigin, { left: -18, right: 23, top: 50, bottom: -5 });
  assert.equal(clip("POSS", "idle").steps[0]!.state, "S_POSS_STND");
});

test("sprite posts decode as transparent RGBA without treating palette index 255 as transparent", () => {
  const buffer = loadWad();
  const wad = decodeWad(buffer);
  const directory = directoryByName(wad.entries);
  const palette = decodePalette(
    wadLumpBytes(buffer, directory.get("PLAYPAL")!),
  );
  const patch = decodeSpritePatchToRgba(
    wadLumpBytes(buffer, directory.get("PUFFA0")!),
    palette,
  );

  assert.equal(patch.width, 5);
  assert.equal(patch.height, 5);
  assert.equal(patch.opaquePixelCount, 21);
  assert.deepEqual([...patch.rgba.slice(0, 4)], [0, 0, 0, 0]);
  const center = (2 * patch.width + 2) * 4;
  assert.deepEqual(
    [...patch.rgba.slice(center, center + 4)],
    [255, 255, 115, 255],
  );
});

test("atlas manifest has exact source/output provenance and normalized frame UVs", () => {
  const rendered = renderSpriteArtifacts(WAD_PATH);
  assert.equal(rendered.manifest.wadSha256, WAD_SHA256);
  assert.equal(rendered.manifest.wadByteLength, WAD_BYTE_LENGTH);
  assert.equal(rendered.manifest.sourceLumps.length, 163);
  assert.deepEqual(
    rendered.manifest.atlases.map((atlas) => [atlas.file, atlas.frames.length]),
    [
      ["actors.png", 151],
      ["effects.png", 12],
    ],
  );
  const actors = rendered.manifest.atlases[0]!;
  const frame = actors.frames.find((candidate) => candidate.name === "POSSA1")!;
  assert.equal(frame.sourceByteLength, 1392);
  assert.deepEqual(frame.pixelSize, [41, 55]);
  assert.deepEqual(frame.origin, [18, 50]);
  assert.equal(frame.uv.min[0], frame.atlasRect[0] / actors.width);
  assert.equal(frame.uv.min[1], frame.atlasRect[1] / actors.height);
  assert.equal(
    frame.uv.max[1],
    (frame.atlasRect[1] + frame.atlasRect[3]) / actors.height,
  );
  assert.match(actors.pngSha256, /^[0-9a-f]{64}$/);
  assert.ok(actors.pngByteLength > 0);
  const actorFile = rendered.files.find(
    (file) => file.relativePath === actors.file,
  );
  assert.ok(actorFile);
  assert.equal(sha256Hex(actorFile.bytes), actors.pngSha256);
  assert.equal(actorFile.bytes.length, actors.pngByteLength);
  const header = new DataView(
    actorFile.bytes.buffer,
    actorFile.bytes.byteOffset,
    actorFile.bytes.byteLength,
  );
  assert.equal(header.getUint32(16), actors.width);
  assert.equal(header.getUint32(20), actors.height);
  assert.equal(header.getUint8(25), 6, "atlas PNG must be RGBA8");

  const buffer = loadWad();
  const wad = decodeWad(buffer);
  const directory = directoryByName(wad.entries);
  const palette = decodePalette(
    wadLumpBytes(buffer, directory.get("PLAYPAL")!),
  );
  const generated = decodeGeneratedPngRgba(actorFile.bytes);
  for (const frameName of ["POSSA1", "SPOSA1", "TROOA1"]) {
    const selectedFrame = actors.frames.find(
      (candidate) => candidate.name === frameName,
    )!;
    const source = decodeSpritePatchToRgba(
      wadLumpBytes(buffer, directory.get(frameName)!),
      palette,
    );
    const [atlasX, atlasY, frameWidth, frameHeight] = selectedFrame.atlasRect;
    const atlasFrame = new Uint8Array(source.rgba.length);
    for (let sourceRow = 0; sourceRow < frameHeight; sourceRow += 1) {
      const atlasRow = atlasY + frameHeight - 1 - sourceRow;
      const atlasStart = (atlasRow * generated.width + atlasX) * 4;
      atlasFrame.set(
        generated.rgba.subarray(atlasStart, atlasStart + frameWidth * 4),
        sourceRow * frameWidth * 4,
      );
    }
    assert.deepEqual(
      atlasFrame,
      source.rgba,
      `generated ${frameName} atlas rectangle must contain only its bottom-up source patch`,
    );
  }
});
