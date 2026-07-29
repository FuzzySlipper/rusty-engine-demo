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
import { spawnSync } from "node:child_process";
import { afterEach, test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  ACTIVE_CARRIER_PATHS,
  checkEngineRevision,
  updateEngineRevision,
} from "./engine-revision-lib.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CURRENT = JSON.parse(
  readFileSync(resolve(repoRoot, "engine-source.json"), "utf8"),
).commit;
const NEXT = "1111111111111111111111111111111111111111";
const HISTORICAL = "2222222222222222222222222222222222222222";
const temporaryRoots = [];

afterEach(() => {
  for (const path of temporaryRoots.splice(0)) {
    rmSync(path, { recursive: true, force: true });
  }
});

test("check accepts the complete current carrier set", () => {
  const fixture = copyCarrierFixture();
  assert.equal(checkEngineRevision(fixture).commit, CURRENT);
});

test("check rejects malformed source identity and unexpected fields", () => {
  const fixture = copyCarrierFixture();
  writeJson(fixture, "engine-source.json", {
    schemaVersion: 1,
    repository: "https://example.invalid/private-engine",
    commit: CURRENT.toUpperCase(),
    branch: "main",
  });
  assert.throws(
    () => checkEngineRevision(fixture),
    /expected exactly commit, repository, schemaVersion/u,
  );
});

test("check rejects missing renamed mixed and path Rust carriers", () => {
  for (const mutate of [
    (content) => content.replace("asset-catalog =", "renamed-asset-catalog ="),
    (content) =>
      content.replace(
        `asset-catalog = { git = "https://github.com/FuzzySlipper/rusty-engine.git", rev = "${CURRENT}" }`,
        "",
      ),
    (content) => content.replace(CURRENT, NEXT),
    (content) =>
      content.replace(
        `git = "https://github.com/FuzzySlipper/rusty-engine.git", rev = "${CURRENT}"`,
        `path = "${["..", "rusty-engine", "crates", "asset-catalog"].join("/")}"`,
      ),
  ]) {
    const fixture = copyCarrierFixture();
    mutateFile(fixture, "Cargo.toml", mutate);
    assert.throws(() => checkEngineRevision(fixture), /Cargo\.toml/u);
  }
});

test("check rejects renderer Studio and allow-build drift", () => {
  const packageFixture = copyCarrierFixture();
  mutateFile(packageFixture, "package.json", (content) =>
    content.replace(CURRENT, NEXT),
  );
  assert.throws(
    () => checkEngineRevision(packageFixture),
    /package\.json: @rusty-engine\/render-contracts/u,
  );

  const browserFixture = copyCarrierFixture();
  mutateFile(
    browserFixture,
    "ts/packages/browser-shell/package.json",
    (content) => content.replace("&path:", "-floating&path:"),
  );
  assert.throws(
    () => checkEngineRevision(browserFixture),
    /browser-shell\/package\.json/u,
  );

  const workspaceFixture = copyCarrierFixture();
  mutateFile(workspaceFixture, "pnpm-workspace.yaml", (content) =>
    content.replace(
      '"@rusty-engine/studio-viewport@',
      '"@rusty-engine/studio-renamed@',
    ),
  );
  assert.throws(
    () => checkEngineRevision(workspaceFixture),
    /pnpm-workspace\.yaml/u,
  );

  const unexpectedFixture = copyCarrierFixture();
  const manifest = JSON.parse(
    readFileSync(resolve(unexpectedFixture, "package.json"), "utf8"),
  );
  manifest.devDependencies = {
    ...manifest.devDependencies,
    "@rusty-engine/unexpected": `github:FuzzySlipper/rusty-engine#${CURRENT}&path:tools/unexpected`,
  };
  writeJson(unexpectedFixture, "package.json", manifest);
  assert.throws(
    () => checkEngineRevision(unexpectedFixture),
    /unexpected Engine package @rusty-engine\/unexpected in devDependencies/u,
  );
});

test("check rejects Engine sources in adjacent dependency manifests", () => {
  const packageFixture = copyCarrierFixture();
  mkdirSync(resolve(packageFixture, "ts/packages/project-content"), {
    recursive: true,
  });
  writeJson(packageFixture, "ts/packages/project-content/package.json", {
    name: "@rusty-engine-demo/project-content",
    private: true,
    dependencies: {
      "@rusty-engine/unexpected": `github:FuzzySlipper/rusty-engine#${NEXT}&path:tools/unexpected`,
    },
  });
  assert.throws(
    () => checkEngineRevision(packageFixture),
    /ts\/packages\/project-content\/package\.json: unexpected Engine source @rusty-engine\/unexpected/u,
  );

  const cargoFixture = copyCarrierFixture();
  mkdirSync(resolve(cargoFixture, "rust/crates/adjacent"), {
    recursive: true,
  });
  writeFileSync(
    resolve(cargoFixture, "rust/crates/adjacent/Cargo.toml"),
    `[package]
name = "adjacent"
version = "0.1.0"

[dependencies]
asset-catalog = { git = "https://github.com/FuzzySlipper/rusty-engine.git", rev = "${NEXT}" }
`,
  );
  assert.throws(
    () => checkEngineRevision(cargoFixture),
    /rust\/crates\/adjacent\/Cargo\.toml: unexpected direct Engine dependency carrier/u,
  );
});

test("check rejects stale Cargo and pnpm locks and path fallback", () => {
  const cargoFixture = copyCarrierFixture();
  mutateFile(cargoFixture, "Cargo.lock", (content) =>
    content.replace(CURRENT, NEXT),
  );
  assert.throws(() => checkEngineRevision(cargoFixture), /Cargo\.lock/u);

  const pnpmFixture = copyCarrierFixture();
  mutateFile(pnpmFixture, "pnpm-lock.yaml", (content) =>
    content.replace(CURRENT, NEXT),
  );
  assert.throws(() => checkEngineRevision(pnpmFixture), /pnpm-lock\.yaml/u);

  const pathFixture = copyCarrierFixture();
  mutateFile(pathFixture, "pnpm-lock.yaml", (content) =>
    content.replace(
      `github:FuzzySlipper/rusty-engine#${CURRENT}`,
      `file:${["..", "rusty-engine"].join("/")}`,
    ),
  );
  assert.throws(() => checkEngineRevision(pathFixture), /pnpm-lock\.yaml/u);
});

test("update validates shape and public reachability before creating a worktree", async () => {
  const fixture = gitFixture();
  await assert.rejects(
    updateEngineRevision({ repoRoot: fixture, commit: "main" }),
    /lowercase 40-character/u,
  );
  await assert.rejects(
    updateEngineRevision({
      repoRoot: fixture,
      commit: NEXT,
      provePublic: async () => {
        throw new Error("not public");
      },
    }),
    /not public/u,
  );
  assert.equal(worktreeCount(fixture), 1);
});

test("dry-run is non-mutating, scoped, and cleans its worktree", async () => {
  const fixture = gitFixture();
  const before = carrierSnapshot(fixture);
  const result = await updateEngineRevision({
    repoRoot: fixture,
    commit: NEXT,
    dryRun: true,
    provePublic: async () => {},
    regenerate: fakeRegenerate,
    validate: async (candidate) => checkEngineRevision(candidate),
  });
  assert.match(result.diff, /engine-source\.json/u);
  assert.match(result.diff, new RegExp(NEXT, "u"));
  assert.deepEqual(carrierSnapshot(fixture), before);
  assert.equal(
    readFileSync(resolve(fixture, "docs/history.txt"), "utf8"),
    `${HISTORICAL}\n`,
  );
  assert.equal(worktreeCount(fixture), 1);
});

test("ordinary update preserves unrelated dirty files and historical values", async () => {
  const fixture = gitFixture();
  writeFileSync(resolve(fixture, "unrelated.txt"), "user change\n");
  const result = await updateEngineRevision({
    repoRoot: fixture,
    commit: NEXT,
    provePublic: async () => {},
    regenerate: fakeRegenerate,
    validate: async (candidate) => checkEngineRevision(candidate),
  });
  assert.equal(checkEngineRevision(fixture).commit, NEXT);
  assert.match(result.diff, new RegExp(NEXT, "u"));
  assert.equal(
    readFileSync(resolve(fixture, "docs/history.txt"), "utf8"),
    `${HISTORICAL}\n`,
  );
  assert.equal(
    readFileSync(resolve(fixture, "unrelated.txt"), "utf8"),
    "user change\n",
  );
  assert.equal(worktreeCount(fixture), 1);
});

test("update rejects dirty carriers and cleans up after candidate failure", async () => {
  const dirtyFixture = gitFixture();
  mutateFile(dirtyFixture, "package.json", (content) => `${content}\n`);
  await assert.rejects(
    updateEngineRevision({
      repoRoot: dirtyFixture,
      commit: NEXT,
      provePublic: async () => {},
    }),
    /carrier or lock files are dirty/u,
  );
  assert.equal(worktreeCount(dirtyFixture), 1);

  const failedFixture = gitFixture();
  const before = carrierSnapshot(failedFixture);
  await assert.rejects(
    updateEngineRevision({
      repoRoot: failedFixture,
      commit: NEXT,
      provePublic: async () => {},
      regenerate: async () => {
        throw new Error("synthetic regeneration failure");
      },
    }),
    /synthetic regeneration failure/u,
  );
  assert.deepEqual(carrierSnapshot(failedFixture), before);
  assert.equal(worktreeCount(failedFixture), 1);
});

function copyCarrierFixture() {
  const root = temporaryRoot();
  for (const relativePath of ACTIVE_CARRIER_PATHS) {
    const destination = resolve(root, relativePath);
    mkdirSync(dirname(destination), { recursive: true });
    cpSync(resolve(repoRoot, relativePath), destination);
  }
  return root;
}

function gitFixture() {
  const root = copyCarrierFixture();
  mkdirSync(resolve(root, "docs"), { recursive: true });
  writeFileSync(resolve(root, "docs/history.txt"), `${HISTORICAL}\n`);
  git(root, ["init", "--quiet"]);
  git(root, ["config", "user.email", "engine-revision-test@example.invalid"]);
  git(root, ["config", "user.name", "Engine Revision Test"]);
  git(root, ["add", "."]);
  git(root, ["commit", "--quiet", "-m", "fixture"]);
  return root;
}

async function fakeRegenerate(candidate, previousCommit, commit) {
  for (const relativePath of ["Cargo.lock", "pnpm-lock.yaml"]) {
    mutateFile(candidate, relativePath, (content) =>
      content.replaceAll(previousCommit, commit),
    );
  }
}

function carrierSnapshot(root) {
  return Object.fromEntries(
    ACTIVE_CARRIER_PATHS.map((relativePath) => [
      relativePath,
      readFileSync(resolve(root, relativePath), "utf8"),
    ]),
  );
}

function writeJson(root, relativePath, value) {
  writeFileSync(
    resolve(root, relativePath),
    `${JSON.stringify(value, null, 2)}\n`,
  );
}

function mutateFile(root, relativePath, mutate) {
  const path = resolve(root, relativePath);
  writeFileSync(path, mutate(readFileSync(path, "utf8")));
}

function worktreeCount(root) {
  return git(root, ["worktree", "list", "--porcelain"])
    .split("\n")
    .filter((line) => line.startsWith("worktree ")).length;
}

function git(cwd, args) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout);
  }
  return result.stdout;
}

function temporaryRoot() {
  const root = mkdtempSync(resolve(tmpdir(), "engine-revision-test-"));
  temporaryRoots.push(root);
  return root;
}
