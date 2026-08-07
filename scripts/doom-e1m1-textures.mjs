#!/usr/bin/env node
// Thin wrapper so `pnpm run` and CI can invoke the doom-e1m1 texture authoring
// without knowing the exact package dist path. Delegates to the TS package CLI
// which owns the deterministic PNG/manifest generation.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const pkgCli = fileURLToPath(new URL("../ts/packages/doom-e1m1-authoring/dist/texture-cli.js", import.meta.url));
const args = process.argv.slice(2);
if (args.length === 0) args.push("--write");
const result = spawnSync(process.execPath, [pkgCli, ...args], { stdio: "inherit" });
process.exit(result.status ?? 0);
