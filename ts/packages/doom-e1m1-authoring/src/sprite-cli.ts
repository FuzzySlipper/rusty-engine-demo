#!/usr/bin/env node
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  buildSpriteArtifacts,
  checkSpriteArtifacts,
  DEFAULT_SPRITE_OUT_DIR,
  DEFAULT_SPRITE_WAD,
} from "./sprite-extract.js";

function arg(name: string): string | undefined {
  const prefix = `--${name}=`;
  return process.argv
    .find((value) => value.startsWith(prefix))
    ?.slice(prefix.length);
}

function isDirectInvocation(): boolean {
  const entry = process.argv[1];
  return (
    entry !== undefined && fileURLToPath(import.meta.url) === resolve(entry)
  );
}

function run(): void {
  const mode = process.argv.includes("--write")
    ? "write"
    : process.argv.includes("--check")
      ? "check"
      : "check";
  const wadPath = arg("wad") ?? DEFAULT_SPRITE_WAD;
  const outDir = arg("out") ?? DEFAULT_SPRITE_OUT_DIR;
  if (mode === "write") {
    const manifest = buildSpriteArtifacts(wadPath, outDir);
    console.log(
      `Wrote ${outDir} (${manifest.diagnostics.frameCount} frames, ${manifest.diagnostics.totalPngBytes} PNG bytes) wad=${manifest.wadSha256.slice(0, 12)}`,
    );
    return;
  }
  checkSpriteArtifacts(wadPath, outDir);
}

if (isDirectInvocation()) run();
