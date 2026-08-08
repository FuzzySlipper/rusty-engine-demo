import assert from "node:assert/strict";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { afterEach, test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  ACTIVE_CARRIER_PATHS,
  checkDevelopmentResolution,
  checkEngineRevision,
  updateEngineRevision,
} from "./engine-revision-lib.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const current = JSON.parse(
  readFileSync(resolve(repoRoot, "engine-source.json"), "utf8"),
).commit;
const next = "1111111111111111111111111111111111111111";
const temporaryRoots = [];

afterEach(() => {
  for (const path of temporaryRoots.splice(0)) {
    rmSync(path, { recursive: true, force: true });
  }
});

test("check accepts the rolling facade and private Studio resolver closure", () => {
  const fixture = copyFixture();
  assert.equal(checkEngineRevision(fixture).commit, current);
  assert.equal(
    checkDevelopmentResolution(fixture).report.resolvedCommit,
    current,
  );
});

test("check rejects a second direct Rust owner dependency", () => {
  const fixture = copyFixture();
  mutate(fixture, "Cargo.toml", (content) =>
    content.replace(
      "rusty-engine = {",
      `core-ids = { git = "https://github.com/FuzzySlipper/rusty-engine", branch = "main" }\nrusty-engine = {`,
    ),
  );
  assert.throws(() => checkEngineRevision(fixture), /exactly one rolling/u);
});

test("check rejects an Engine renderer dependency in the browser package", () => {
  const fixture = copyFixture();
  const path = resolve(fixture, "ts/packages/browser-shell/package.json");
  const manifest = JSON.parse(readFileSync(path, "utf8"));
  manifest.dependencies["@rusty-engine/renderer-host"] =
    `github:FuzzySlipper/rusty-engine#${current}&path:render/packages/renderer-host`;
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
  assert.throws(() => checkEngineRevision(fixture), /browser-shell/u);
});

test("check rejects stale Cargo and pnpm locks", () => {
  const cargoFixture = copyFixture();
  mutate(cargoFixture, "Cargo.lock", (content) =>
    content.replaceAll(current, next),
  );
  assert.throws(() => checkEngineRevision(cargoFixture), /Cargo\.lock/u);

  const pnpmFixture = copyFixture();
  mutate(pnpmFixture, "pnpm-lock.yaml", (content) =>
    content.replace(current, next),
  );
  assert.throws(() => checkEngineRevision(pnpmFixture), /pnpm-lock\.yaml/u);
});

test("check rejects stale Studio allow-build carriers", () => {
  const fixture = copyFixture();
  mutate(fixture, "pnpm-workspace.yaml", (content) =>
    content.replace(current, next),
  );
  assert.throws(() => checkEngineRevision(fixture), /allowBuilds/u);
});

test("exact update dry-run validates public reachability without mutation", async () => {
  const fixture = copyFixture();
  const before = carrierSnapshot(fixture);
  const result = await updateEngineRevision({
    repoRoot: fixture,
    commit: next,
    dryRun: true,
    provePublic: async () => {},
  });
  assert.equal(result.commit, next);
  assert.match(result.diff, new RegExp(next, "u"));
  assert.deepEqual(carrierSnapshot(fixture), before);
});

function copyFixture() {
  const fixture = mkdtempSync(resolve(tmpdir(), "rusty-demo-engine-revision-"));
  temporaryRoots.push(fixture);
  for (const relativePath of ACTIVE_CARRIER_PATHS) {
    const target = resolve(fixture, relativePath);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(resolve(repoRoot, relativePath), target);
  }
  for (const relativePath of [
    "engine-development.json",
    "ts/packages/browser-shell/package.json",
  ]) {
    const target = resolve(fixture, relativePath);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(resolve(repoRoot, relativePath), target);
  }
  const resolutionPath = resolve(
    fixture,
    ".engine-development/resolution.json",
  );
  mkdirSync(dirname(resolutionPath), { recursive: true });
  writeFileSync(
    resolutionPath,
    `${JSON.stringify(
      {
        schemaVersion: 1,
        mode: "development",
        repository: "https://github.com/FuzzySlipper/rusty-engine",
        requestedRef: "refs/heads/main",
        resolvedCommit: current,
        source: "public",
        sourcePath: null,
        dirty: false,
        certification: false,
        applied: true,
      },
      null,
      2,
    )}\n`,
  );
  return fixture;
}

function mutate(root, relativePath, transform) {
  const path = resolve(root, relativePath);
  writeFileSync(path, transform(readFileSync(path, "utf8")));
}

function carrierSnapshot(root) {
  return Object.fromEntries(
    ACTIVE_CARRIER_PATHS.map((relativePath) => [
      relativePath,
      readFileSync(resolve(root, relativePath), "utf8"),
    ]),
  );
}
