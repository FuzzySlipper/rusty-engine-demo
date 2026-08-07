#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { buildE1M1Intermediate, serializeIntermediate } from "./wad-decode.js";

const DEFAULT_WAD = "/home/research/doom.ts/public/doom1.wad";
const DEFAULT_STAGING_JSON = resolve(
  fileURLToPath(new URL("../../../../content/doom-e1m1/e1m1.intermediate.json", import.meta.url)),
);

function arg(name: string): string | undefined {
  const pref = `--${name}=`;
  for (const a of process.argv) if (a.startsWith(pref)) return a.slice(pref.length);
  return undefined;
}

const mode = process.argv.includes("--check") ? "check" : process.argv.includes("--write") ? "write" : "--check";
const wadPath = arg("wad") ?? DEFAULT_WAD;
const outPath = arg("out") ?? DEFAULT_STAGING_JSON;

function loadWadBytes(path: string): ArrayBuffer {
  const bytes = readFileSync(path);
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

function generate(intermediateJson: string): string {
  return intermediateJson;
}

function run(): void {
  const buf = loadWadBytes(wadPath);
  const intermediate = buildE1M1Intermediate(buf, wadPath);
  const expected = serializeIntermediate(intermediate);

  if (mode === "write") {
    mkdirSync(resolve(outPath, ".."), { recursive: true });
    writeFileSync(outPath, expected, "utf8");
    console.log(`Wrote ${outPath} (${expected.length} bytes) wad=${intermediate.source.wadSha256.slice(0, 12)} entries=${intermediate.source.entryCount}`);
    return;
  }

  // --check
  let actual: string;
  try {
    actual = readFileSync(outPath, "utf8");
  } catch {
    throw new Error(`${outPath} missing; run \`pnpm --filter @rusty-engine-demo/doom-e1m1-authoring generate\` to create it`);
  }
  if (actual !== expected) {
    const msg = `${outPath} is stale (bytes actual=${actual.length} expected=${expected.length}). Run generate --write.`;
    // emit helpful diff head
    const aLines = actual.split("\n");
    const eLines = expected.split("\n");
    for (let i = 0; i < Math.min(aLines.length, eLines.length); i += 1) {
      if (aLines[i] !== eLines[i]) {
        console.error(`first difference at line ${i + 1}:\n  actual: ${aLines[i]}\n  expect: ${eLines[i]}`);
        break;
      }
    }
    throw new Error(msg);
  }
  console.log(`OK ${outPath} (${expected.length} bytes)`);
}

run();
