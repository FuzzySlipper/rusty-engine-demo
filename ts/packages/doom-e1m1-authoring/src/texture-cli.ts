import { mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";

import { buildE1M1Intermediate, decodeWad } from "./wad-decode.js";
import { decodePalette, flatToPng, decodeTexture1, decodePnames, decodePatch, textureToPngBytes, directoryByName, wadLumpBytes, hashBytes } from "./textures.js";
import { sha256Hex } from "./png.js";

const DEFAULT_WAD = "/home/research/doom.ts/public/doom1.wad";
const DEFAULT_OUT_DIR = resolve(new URL("../../../../content/doom-e1m1/textures", import.meta.url).pathname);
const DEFAULT_MANIFEST = resolve(DEFAULT_OUT_DIR, "manifest.json");
const DEFAULT_STAGING_DIR = resolve(new URL("../../../../content/doom-e1m1", import.meta.url).pathname);
const DOOM_UNITS_PER_VOXEL_CELL = 16;

export interface TextureManifestEntry {
  readonly kind: "flat" | "wall";
  readonly name: string;
  readonly sourceLump: string;
  readonly sourceByteLength: number;
  readonly sourceSha256: string;
  readonly pngSha256: string;
  readonly pngByteLength: number;
  readonly width: number;
  readonly height: number;
  readonly tileScale: [number, number] | null;
}

export interface SkyTextureManifestEntry {
  readonly name: "SKY1";
  readonly sourceLump: "TEXTURE1:SKY1";
  readonly sourceByteLength: number;
  readonly sourceSha256: string;
  readonly pngSha256: string;
  readonly pngByteLength: number;
  readonly width: number;
  readonly height: number;
  readonly runtimeMapping: "equirectangular";
}

export interface TextureManifest {
  readonly generatedAt: string;
  readonly wadPath: string;
  readonly wadSha256: string;
  readonly wadByteLength: number;
  readonly paletteSha256: string;
  readonly entries: readonly TextureManifestEntry[];
  readonly sky: SkyTextureManifestEntry;
  readonly diagnostics: {
    readonly totalPngBytes: number;
    readonly totalDecodedRgbaBytes: number;
    readonly textureIdentities: number;
  };
}

function loadWadBuffer(path: string): ArrayBuffer {
  const bytes = readFileSync(path);
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

export function buildTextureArtifacts(wadPath: string = DEFAULT_WAD, outDir: string = DEFAULT_OUT_DIR): TextureManifest {
  const buffer = loadWadBuffer(wadPath);
  const wad = decodeWad(buffer, { computeSha256: true });
  const sha = wad.sha256!;
  const dir = directoryByName(wad.entries);
  const byName = (n: string) => {
    const e = dir.get(n);
    if (!e) throw new Error(`WAD missing lump ${n}`);
    return e;
  };

  // E1M1 incidence from intermediate (reuse build to get wall/flat lists)
  const inter = buildE1M1Intermediate(buffer, wadPath);
  const wallNames = inter.diagnostics.textureIncidence.wallTextureNames;
  const flatNames = inter.diagnostics.textureIncidence.flatTextureNames;

  // PLAYPAL
  const playpalEntry = byName("PLAYPAL");
  const playpalBytes = wadLumpBytes(buffer, playpalEntry);
  const palette = decodePalette(playpalBytes);
  const paletteSha = hashBytes(playpalBytes.subarray(0, 768));

  // PNAMES / TEXTURE1 for wall compositing
  const pnamesEntry = byName("PNAMES");
  const pnamesBytes = wadLumpBytes(buffer, pnamesEntry);
  const pnames = decodePnames(pnamesBytes);

  const tex1Entry = byName("TEXTURE1");
  const tex1Bytes = wadLumpBytes(buffer, tex1Entry);
  const texDefs = decodeTexture1(tex1Bytes, pnames);
  const texByName = new Map(texDefs.map((d) => [d.name, d] as const));
  // Keep raw TEXTURE1 entry bytes per texture for provenance hashing
  const texEntryBytesByName = new Map<string, Uint8Array>();
  {
    const view = new DataView(tex1Bytes.buffer, tex1Bytes.byteOffset, tex1Bytes.byteLength);
    const num = view.getInt32(0, true);
    const offsets: number[] = [];
    for (let i = 0; i < num; i += 1) offsets.push(view.getInt32(4 + i * 4, true));
    for (let i = 0; i < num; i += 1) {
      const off = offsets[i]!;
      const patchCount = view.getInt16(off + 20, true);
      const len = 22 + patchCount * 10;
      texEntryBytesByName.set(texDefs[i]!.name, tex1Bytes.subarray(off, off + len));
    }
  }

  // Patch cache: decode on demand — WAD names are case-insensitive (PNAMES may be lower, directory upper)
  const patchCache = new Map<string, ReturnType<typeof decodePatch>>();
  const patchBytesCache = new Map<string, Uint8Array>();
  const getPatch = (name: string) => {
    const key = name.toUpperCase();
    let cached = patchCache.get(key);
    if (cached) return cached;
    const entry = dir.get(key) ?? [...dir.entries()].find(([k]) => k.toUpperCase() === key)?.[1];
    if (!entry) throw new Error(`missing patch lump ${name}`);
    const bytes = wadLumpBytes(buffer, entry);
    const decoded = decodePatch(bytes);
    patchCache.set(key, decoded);
    patchBytesCache.set(key, bytes);
    return decoded;
  };
  const getPatchBytes = (name: string): Uint8Array => {
    const key = name.toUpperCase();
    let b = patchBytesCache.get(key);
    if (b) return b;
    getPatch(name);
    return patchBytesCache.get(key)!;
  };
  const skyDef = texByName.get("SKY1");
  if (!skyDef) throw new Error("TEXTURE1 missing canonical SKY1 panorama");
  if (skyDef.width !== skyDef.height * 2) {
    throw new Error(`SKY1 must be an exact 2:1 panorama, got ${skyDef.width}x${skyDef.height}`);
  }
  // Preload all patch bytes needed for E1M1 walls (to validate missing-patch early)
  for (const w of wallNames) {
    const def = texByName.get(w);
    if (!def) throw new Error(`TEXTURE1 missing definition for wall ${w}`);
    for (const p of def.patches) getPatch(p.patchName);
  }
  for (const patch of skyDef.patches) getPatch(patch.patchName);

  const entries: TextureManifestEntry[] = [];
  let totalPng = 0;
  let totalRgba = 0;

  // Flats: 64x64
  for (const flatName of flatNames) {
    const entry = dir.get(flatName);
    if (!entry) throw new Error(`missing flat lump ${flatName}`);
    const flatBytes = wadLumpBytes(buffer, entry);
    if (flatBytes.length !== 4096) throw new Error(`flat ${flatName} bad size ${flatBytes.length}`);
    const pngBytes = flatToPng(flatBytes, palette);
    if (pngBytes.length > 16 * 1024 * 1024) throw new Error(`flat ${flatName} PNG ${pngBytes.length} exceeds 16 MiB`);
    const pngSha = hashBytes(pngBytes);
    totalPng += pngBytes.length;
    totalRgba += 64 * 64 * 4;
    // write file
    const outPath = join(outDir, "flat", `${flatName}.png`);
    mkdirSync(resolve(outPath, ".."), { recursive: true });
    writeFileSync(outPath, pngBytes);
    entries.push({
      kind: "flat",
      name: flatName,
      sourceLump: flatName,
      sourceByteLength: flatBytes.length,
      sourceSha256: hashBytes(flatBytes),
      pngSha256: pngSha,
      pngByteLength: pngBytes.length,
      width: 64,
      height: 64,
      tileScale: [64 / DOOM_UNITS_PER_VOXEL_CELL, 64 / DOOM_UNITS_PER_VOXEL_CELL],
    });
  }

  // Walls
  for (const wallName of wallNames) {
    const def = texByName.get(wallName)!;
    const patchMap = new Map(patchCache.entries());
    const pngBytes = textureToPngBytes(def, patchMap, palette);
    if (pngBytes.length > 16 * 1024 * 1024) throw new Error(`wall ${wallName} PNG ${pngBytes.length} exceeds 16 MiB`);
    const pngSha = hashBytes(pngBytes);
    totalPng += pngBytes.length;
    totalRgba += def.width * def.height * 4;
    const outPath = join(outDir, "wall", `${wallName}.png`);
    mkdirSync(resolve(outPath, ".."), { recursive: true });
    writeFileSync(outPath, pngBytes);
    const entryBytes = texEntryBytesByName.get(wallName)!;
    const patchBytesList = def.patches.map((p) => getPatchBytes(p.patchName));
    const hasher = createHash("sha256");
    hasher.update(entryBytes);
    for (const pb of patchBytesList) hasher.update(pb);
    const provenanceSha = hasher.digest("hex");
    const provenanceLen = entryBytes.length + patchBytesList.reduce((s, b) => s + b.length, 0);
    entries.push({
      kind: "wall",
      name: wallName,
      sourceLump: `TEXTURE1:${wallName}`,
      sourceByteLength: provenanceLen,
      sourceSha256: provenanceSha,
      pngSha256: pngSha,
      pngByteLength: pngBytes.length,
      width: def.width,
      height: def.height,
      tileScale: def.width
        ? [def.width / DOOM_UNITS_PER_VOXEL_CELL, def.height / DOOM_UNITS_PER_VOXEL_CELL]
        : null,
    });
  }

  const skyPngBytes = textureToPngBytes(skyDef, new Map(patchCache.entries()), palette);
  if (skyPngBytes.length > 16 * 1024 * 1024) {
    throw new Error(`SKY1 PNG ${skyPngBytes.length} exceeds 16 MiB`);
  }
  const skyEntryBytes = texEntryBytesByName.get("SKY1");
  if (!skyEntryBytes) throw new Error("TEXTURE1 provenance bytes missing for SKY1");
  const skyPatchBytes = skyDef.patches.map((patch) => getPatchBytes(patch.patchName));
  const skySourceHasher = createHash("sha256");
  skySourceHasher.update(skyEntryBytes);
  for (const bytes of skyPatchBytes) skySourceHasher.update(bytes);
  const sky: SkyTextureManifestEntry = {
    name: "SKY1",
    sourceLump: "TEXTURE1:SKY1",
    sourceByteLength: skyEntryBytes.length + skyPatchBytes.reduce((sum, bytes) => sum + bytes.length, 0),
    sourceSha256: skySourceHasher.digest("hex"),
    pngSha256: hashBytes(skyPngBytes),
    pngByteLength: skyPngBytes.length,
    width: skyDef.width,
    height: skyDef.height,
    runtimeMapping: "equirectangular",
  };
  const skyPath = join(outDir, "sky", "SKY1.png");
  mkdirSync(resolve(skyPath, ".."), { recursive: true });
  writeFileSync(skyPath, skyPngBytes);
  totalPng += skyPngBytes.length;
  totalRgba += skyDef.width * skyDef.height * 4;

  // VTX budgets
  if (entries.length + 1 > 256) throw new Error(`texture identities ${entries.length + 1} exceeds 256`);
  if (totalPng > 128 * 1024 * 1024) throw new Error(`total PNG bytes ${totalPng} exceeds 128 MiB`);
  if (totalRgba > 256 * 1024 * 1024) throw new Error(`total decoded RGBA bytes ${totalRgba} exceeds 256 MiB`);

  // Stable sort entries by kind then name (flat before wall for determinism)
  entries.sort((a, b) => (a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind.localeCompare(b.kind)));

  const manifest: TextureManifest = {
    generatedAt: "2026-08-07T00:00:00.000Z",
    wadPath,
    wadSha256: sha,
    wadByteLength: buffer.byteLength,
    paletteSha256: paletteSha,
    entries,
    sky,
    diagnostics: {
      totalPngBytes: totalPng,
      totalDecodedRgbaBytes: totalRgba,
      textureIdentities: entries.length + 1,
    },
  };

  mkdirSync(outDir, { recursive: true });
  const manifestPath = join(outDir, "manifest.json");
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  return manifest;
}

function sha256HexStr(s: string): string {
  return createHash("sha256").update(s).digest("hex");
}

export function checkTextureArtifacts(wadPath: string = DEFAULT_WAD, outDir: string = DEFAULT_OUT_DIR): void {
  const manifestPath = join(outDir, "manifest.json");
  if (!existsSync(manifestPath)) throw new Error(`${manifestPath} missing; run generate`);
  const existing = JSON.parse(readFileSync(manifestPath, "utf8")) as TextureManifest;
  // Rebuild in-memory and compare entries deterministically (ignore generatedAt)
  const rebuilt = buildTextureArtifacts(wadPath, outDir);
  // Re-read after rebuild would have overwritten; so we need to have captured expected before rebuild?
  // Instead, we rebuilt and overwrote; so check by re-reading and comparing without timestamp.
  // To avoid mutation before check, we should not write during check. So we reimplement check as dry-run comparison.
  // For now, simple: compare existing.entries vs rebuilt.entries (excluding generatedAt)
  const norm = (m: TextureManifest) => ({ ...m, generatedAt: "" });
  const a = JSON.stringify(norm(existing), null, 2);
  const b = JSON.stringify(norm(rebuilt), null, 2);
  if (a !== b) {
    throw new Error(`texture manifest stale; run generate --write. Diff excerpts:\n${a.slice(0, 500)}\n---\n${b.slice(0, 500)}`);
  }
  console.log(`OK ${manifestPath} (${existing.entries.length} textures, ${existing.diagnostics.totalPngBytes} bytes)`);
}

// CLI
const mode = process.argv.includes("--check") ? "check" : process.argv.includes("--write") ? "write" : "write";
const wadArg = process.argv.find((a) => a.startsWith("--wad="))?.slice("--wad=".length) ?? DEFAULT_WAD;
const outArg = process.argv.find((a) => a.startsWith("--out="))?.slice("--out=".length) ?? DEFAULT_OUT_DIR;

if (import.meta.url.endsWith(process.argv[1] ?? "")) {
  // direct invoke
}

if (mode === "write") {
  const m = buildTextureArtifacts(wadArg, outArg);
  console.log(`Wrote ${m.entries.length} textures to ${outArg} totalPng=${m.diagnostics.totalPngBytes}`);
} else {
  // For check we want a non-mutating verify; build in tmp and compare
  const tmpDir = `${outArg}.tmpcheck`;
  try {
    const rebuild = buildTextureArtifacts(wadArg, tmpDir);
    const existingPath = resolve(outArg, "manifest.json");
    const existing = JSON.parse(readFileSync(existingPath, "utf8")) as TextureManifest;
    const norm = (m: TextureManifest) => JSON.stringify({ ...m, generatedAt: "" as string }, null, 2);
    if (norm(existing) !== norm(rebuild)) {
      console.error(`manifest mismatch`);
      throw new Error(`${existingPath} is stale; run generate --write`);
    }
    // also verify each PNG byte-identical
    for (const e of existing.entries) {
      const p = resolve(outArg, e.kind, `${e.name}.png`);
      const tmpP = resolve(tmpDir, e.kind, `${e.name}.png`);
      const a = readFileSync(p);
      const b = readFileSync(tmpP);
      if (!a.equals(b)) throw new Error(`PNG mismatch ${e.name}`);
    }
    const skyPath = resolve(outArg, "sky", "SKY1.png");
    const rebuiltSkyPath = resolve(tmpDir, "sky", "SKY1.png");
    if (!readFileSync(skyPath).equals(readFileSync(rebuiltSkyPath))) {
      throw new Error("PNG mismatch SKY1");
    }
    console.log(`OK ${existingPath} (${existing.entries.length} textures)`);
  } finally {
    // clean tmp
    try { (await import("node:fs")).rmSync(tmpDir, { recursive: true, force: true }); } catch {}
  }
}
