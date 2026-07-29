import {
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";

export const ENGINE_REPOSITORY = "https://github.com/FuzzySlipper/rusty-engine";
export const COMMIT_PATTERN = /^[0-9a-f]{40}$/u;

export const ENGINE_PACKAGES = new Map([
  ["@rusty-engine/render-contracts", "render/packages/render-contracts"],
  ["@rusty-engine/render-projection", "render/packages/render-projection"],
  ["@rusty-engine/renderer-host", "render/packages/renderer-host"],
  ["@rusty-engine/renderer-three", "render/packages/renderer-three"],
  ["@rusty-engine/studio-adapter-client", "studio/libs/adapter-client"],
  ["@rusty-engine/studio-editor-shell", "studio/libs/editor-shell"],
  ["@rusty-engine/studio-user-settings", "studio/libs/user-settings"],
  ["@rusty-engine/studio-viewport", "studio/libs/viewport"],
  ["@rusty-engine/studio-voxel-editor", "studio/libs/voxel-editor"],
]);

export const BROWSER_ENGINE_PACKAGES = new Set([
  "@rusty-engine/render-contracts",
  "@rusty-engine/render-projection",
  "@rusty-engine/renderer-host",
  "@rusty-engine/renderer-three",
]);

export const ENGINE_CRATES = [
  "asset-catalog",
  "asset-import",
  "authored-scene",
  "content-store",
  "core-assets",
  "core-ids",
  "core-math",
  "core-space",
  "core-time",
  "core-voxel",
  "engine-inspector",
  "engine-spatial",
  "entity-state",
  "environment-authoring",
  "gameplay-mechanics",
  "render-model",
  "render-projection",
  "voxel-annotation",
  "voxel-asset",
  "voxel-convert",
  "voxel-object-runtime",
];

export const ACTIVE_CARRIER_PATHS = [
  "engine-source.json",
  "Cargo.toml",
  "Cargo.lock",
  "package.json",
  "ts/packages/browser-shell/package.json",
  "pnpm-workspace.yaml",
  "pnpm-lock.yaml",
];

const REPAIR_COMMAND = "./scripts/engine-revision update <sha>";
const DECLARED_PACKAGE_MANIFESTS = new Set([
  "package.json",
  "ts/packages/browser-shell/package.json",
]);
const MANIFEST_SCAN_IGNORES = new Set([
  ".git",
  ".nx",
  "dist",
  "node_modules",
  "target",
]);

export function loadEngineSource(repoRoot) {
  const relativePath = "engine-source.json";
  const path = resolve(repoRoot, relativePath);
  let source;
  try {
    source = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new Error(
      `${relativePath}: cannot decode canonical Engine source: ${error.message}`,
    );
  }
  if (source === null || Array.isArray(source) || typeof source !== "object") {
    throw new Error(`${relativePath}: expected one JSON object`);
  }
  const keys = Object.keys(source).sort();
  const expectedKeys = ["commit", "repository", "schemaVersion"];
  if (JSON.stringify(keys) !== JSON.stringify(expectedKeys)) {
    throw new Error(
      `${relativePath}: expected exactly ${expectedKeys.join(", ")}; observed ${keys.join(", ")}`,
    );
  }
  if (source.schemaVersion !== 1) {
    throw new Error(
      `${relativePath}: schemaVersion expected 1; observed ${String(source.schemaVersion)}`,
    );
  }
  if (source.repository !== ENGINE_REPOSITORY) {
    throw new Error(
      `${relativePath}: repository expected ${ENGINE_REPOSITORY}; observed ${String(source.repository)}`,
    );
  }
  assertCommit(source.commit, `${relativePath}: commit`);
  return Object.freeze({
    schemaVersion: 1,
    repository: ENGINE_REPOSITORY,
    commit: source.commit,
  });
}

export function checkEngineRevision(repoRoot) {
  const source = loadEngineSource(repoRoot);
  const violations = [];
  checkCargoManifest(repoRoot, source, violations);
  checkCargoLock(repoRoot, source, violations);
  checkPackageManifest(
    repoRoot,
    "package.json",
    new Set(ENGINE_PACKAGES.keys()),
    source,
    violations,
  );
  checkPackageManifest(
    repoRoot,
    "ts/packages/browser-shell/package.json",
    BROWSER_ENGINE_PACKAGES,
    source,
    violations,
  );
  checkPnpmWorkspace(repoRoot, source, violations);
  checkPnpmLock(repoRoot, source, violations);
  checkAdjacentDependencyManifests(repoRoot, violations);
  if (violations.length > 0) {
    throw new Error(
      `Engine revision check failed:\n${violations
        .map((violation) => `- ${violation}`)
        .join("\n")}\nRepair with: ${REPAIR_COMMAND}`,
    );
  }
  return source;
}

export async function updateEngineRevision({
  repoRoot,
  commit,
  dryRun = false,
  provePublic = provePublicCommit,
  regenerate = regenerateLocks,
  validate = validateCandidate,
}) {
  assertCommit(commit, "update commit");
  const before = loadEngineSource(repoRoot);
  const unexpectedSourceViolations = [];
  checkAdjacentDependencyManifests(repoRoot, unexpectedSourceViolations);
  if (unexpectedSourceViolations.length > 0) {
    throwRevisionViolations(unexpectedSourceViolations);
  }
  await provePublic(before.repository, commit);
  assertCarrierFilesClean(repoRoot);

  const head = git(repoRoot, ["rev-parse", "HEAD"]).trim();
  const temporaryRoot = mkdtempSync(
    resolve(tmpdir(), "rusty-engine-demo-engine-revision-"),
  );
  const candidate = resolve(temporaryRoot, "candidate");
  let worktreeAdded = false;
  try {
    git(repoRoot, ["worktree", "add", "--detach", candidate, head]);
    worktreeAdded = true;
    rewriteActiveCarriers(candidate, before.commit, commit);
    await regenerate(candidate, before.commit, commit);
    await validate(candidate);
    const diff = scopedDiff(candidate);

    if (dryRun) return Object.freeze({ before, commit, diff, dryRun: true });

    if (git(repoRoot, ["rev-parse", "HEAD"]).trim() !== head) {
      throw new Error(
        `caller HEAD changed during update; expected ${head}. No update was applied.`,
      );
    }
    assertCarrierFilesClean(repoRoot);
    if (diff.length > 0) {
      run("git", ["apply", "--whitespace=nowarn", "-"], {
        cwd: repoRoot,
        input: diff,
      });
    }
    checkEngineRevision(repoRoot);
    return Object.freeze({
      before,
      commit,
      diff: scopedDiff(repoRoot),
      dryRun: false,
    });
  } finally {
    if (worktreeAdded) {
      run("git", ["worktree", "remove", "--force", candidate], {
        cwd: repoRoot,
        allowFailure: true,
      });
    }
    rmSync(temporaryRoot, { recursive: true, force: true });
    run("git", ["worktree", "prune"], { cwd: repoRoot, allowFailure: true });
  }
}

export function rewriteActiveCarriers(repoRoot, previousCommit, commit) {
  assertCommit(previousCommit, "previous commit");
  assertCommit(commit, "replacement commit");

  writeFileSync(
    resolve(repoRoot, "engine-source.json"),
    `${JSON.stringify(
      {
        schemaVersion: 1,
        repository: ENGINE_REPOSITORY,
        commit,
      },
      null,
      2,
    )}\n`,
  );

  replaceRequiredCommit(
    resolve(repoRoot, "Cargo.toml"),
    previousCommit,
    commit,
  );
  rewritePackageManifest(
    resolve(repoRoot, "package.json"),
    new Set(ENGINE_PACKAGES.keys()),
    previousCommit,
    commit,
  );
  rewritePackageManifest(
    resolve(repoRoot, "ts/packages/browser-shell/package.json"),
    BROWSER_ENGINE_PACKAGES,
    previousCommit,
    commit,
  );
  rewriteWorkspacePolicy(
    resolve(repoRoot, "pnpm-workspace.yaml"),
    previousCommit,
    commit,
  );
}

export async function provePublicCommit(repository, commit) {
  const temporaryRoot = mkdtempSync(
    resolve(tmpdir(), "rusty-engine-public-commit-"),
  );
  try {
    run("git", ["init", "--bare", "--quiet"], { cwd: temporaryRoot });
    run(
      "git",
      [
        "-c",
        "protocol.version=2",
        "fetch",
        "--quiet",
        "--no-tags",
        "--depth=1",
        `${repository}.git`,
        commit,
      ],
      { cwd: temporaryRoot },
    );
    const fetched = git(temporaryRoot, ["rev-parse", "FETCH_HEAD"]).trim();
    if (fetched !== commit) {
      throw new Error(
        `public fetch resolved ${fetched}; expected exact commit ${commit}`,
      );
    }
  } catch (error) {
    throw new Error(
      `Engine commit ${commit} is not publicly fetchable from ${repository}: ${error.message}`,
    );
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
}

async function regenerateLocks(candidate) {
  const packageManager = JSON.parse(
    readFileSync(resolve(candidate, "package.json"), "utf8"),
  ).packageManager;
  if (packageManager !== "pnpm@11.7.0") {
    throw new Error(
      `package.json: expected repository-pinned packageManager pnpm@11.7.0; observed ${String(packageManager)}`,
    );
  }
  const pnpmVersion = run("pnpm", ["--version"], { cwd: candidate }).trim();
  if (pnpmVersion !== "11.7.0") {
    throw new Error(
      `pnpm version expected 11.7.0 from packageManager; observed ${pnpmVersion}`,
    );
  }
  run("cargo", ["metadata", "--format-version", "1"], {
    cwd: candidate,
  });
  run(
    "pnpm",
    [
      "install",
      "--lockfile-only",
      "--ignore-scripts",
      "--frozen-lockfile=false",
    ],
    { cwd: candidate },
  );
}

async function validateCandidate(candidate) {
  checkEngineRevision(candidate);
  run("node", ["scripts/audit-boundary.mjs"], { cwd: candidate });
  run("cargo", ["metadata", "--format-version", "1", "--locked", "--no-deps"], {
    cwd: candidate,
  });
}

function checkCargoManifest(repoRoot, source, violations) {
  const relativePath = "Cargo.toml";
  const content = readFile(repoRoot, relativePath, violations);
  if (content === null) return;
  const dependencySection =
    content.match(/\[workspace\.dependencies\]([\s\S]*?)(?=\n\[|$)/u)?.[1] ??
    "";
  for (const crate of ENGINE_CRATES) {
    const match = dependencySection.match(
      new RegExp(`^${escapeRegExp(crate)}\\s*=\\s*\\{([^\\n]+)\\}$`, "mu"),
    );
    if (match === null) {
      violations.push(`${relativePath}: missing Engine dependency ${crate}`);
      continue;
    }
    const expected = `git = "${ENGINE_REPOSITORY}.git", rev = "${source.commit}"`;
    if (match[1].trim() !== expected) {
      violations.push(
        `${relativePath}: ${crate} expected { ${expected} }; observed { ${match[1].trim()} }`,
      );
    }
  }
  for (const line of dependencySection.split("\n")) {
    if (
      /rusty-engine|@rusty-engine/iu.test(line) &&
      !ENGINE_CRATES.some((crate) => line.startsWith(`${crate} `))
    ) {
      violations.push(
        `${relativePath}: unexpected Engine dependency carrier ${line.trim()}`,
      );
    }
  }
  const metadata =
    content.match(
      /\[workspace\.metadata\.rusty-engine\]([\s\S]*?)(?=\n\[|$)/u,
    )?.[1] ?? "";
  expectTomlScalar(
    relativePath,
    metadata,
    "repository",
    ENGINE_REPOSITORY,
    violations,
  );
  expectTomlScalar(
    relativePath,
    metadata,
    "revision",
    source.commit,
    violations,
  );
  const hasSiblingPath = [...content.matchAll(/path\s*=\s*"([^"]*)"/gmu)].some(
    (match) => isEngineRepositoryReference(match[1]),
  );
  const hasNonCanonicalEngineGit = [
    ...content.matchAll(/git\s*=\s*"([^"]*)"/gmu),
  ].some(
    (match) =>
      isEngineRepositoryReference(match[1]) &&
      match[1] !== `${ENGINE_REPOSITORY}.git`,
  );
  if (hasSiblingPath || hasNonCanonicalEngineGit) {
    violations.push(
      `${relativePath}: path, sibling, or non-canonical Engine source is forbidden`,
    );
  }
}

function checkCargoLock(repoRoot, source, violations) {
  const relativePath = "Cargo.lock";
  const content = readFile(repoRoot, relativePath, violations);
  if (content === null) return;
  const sources = [
    ...content.matchAll(/^source = "(git\+[^"]*rusty-engine[^"]*)"$/gimu),
  ].map((match) => match[1]);
  if (sources.length === 0) {
    violations.push(`${relativePath}: missing locked Engine sources`);
    return;
  }
  const expected = `git+${ENGINE_REPOSITORY}.git?rev=${source.commit}#${source.commit}`;
  for (const observed of new Set(sources)) {
    if (observed !== expected) {
      violations.push(
        `${relativePath}: Engine source expected ${expected}; observed ${observed}`,
      );
    }
  }
}

function checkPackageManifest(
  repoRoot,
  relativePath,
  expectedNames,
  source,
  violations,
) {
  let manifest;
  try {
    manifest = JSON.parse(
      readFileSync(resolve(repoRoot, relativePath), "utf8"),
    );
  } catch (error) {
    violations.push(`${relativePath}: cannot decode JSON: ${error.message}`);
    return;
  }
  const dependencies = manifest.dependencies ?? {};
  for (const name of expectedNames) {
    const packagePath = ENGINE_PACKAGES.get(name);
    const expected = packageSpecifier(source.commit, packagePath);
    if (dependencies[name] !== expected) {
      violations.push(
        `${relativePath}: ${name} expected ${expected}; observed ${String(dependencies[name])}`,
      );
    }
  }
  for (const sectionName of dependencySectionNames()) {
    for (const [name, observed] of Object.entries(
      manifest[sectionName] ?? {},
    )) {
      if (
        isEnginePackageReference(name, observed) &&
        (sectionName !== "dependencies" || !expectedNames.has(name))
      ) {
        violations.push(
          `${relativePath}: unexpected Engine package ${name} in ${sectionName} (${String(observed)})`,
        );
      }
    }
  }
}

function checkPnpmWorkspace(repoRoot, source, violations) {
  const relativePath = "pnpm-workspace.yaml";
  const content = readFile(repoRoot, relativePath, violations);
  if (content === null) return;
  const observed = [
    ...content.matchAll(
      /^\s+"([^"]*(?:@rusty-engine\/|FuzzySlipper\/rusty-engine)[^"]*)":\s+true$/gimu,
    ),
  ].map((match) => match[1]);
  const expected = [...ENGINE_PACKAGES.entries()].map(
    ([name, path]) => `${name}@${codeloadSpecifier(source.commit, path)}`,
  );
  compareSets(relativePath, expected, observed, violations);
}

function checkPnpmLock(repoRoot, source, violations) {
  const relativePath = "pnpm-lock.yaml";
  const content = readFile(repoRoot, relativePath, violations);
  if (content === null) return;
  const references = [
    ...content.matchAll(
      /(?:github:FuzzySlipper\/rusty-engine#|codeload\.github\.com\/FuzzySlipper\/rusty-engine\/tar\.gz\/)([^&/#\s'")}\]]+)/gimu,
    ),
  ].map((match) => match[1]);
  if (references.length === 0) {
    violations.push(`${relativePath}: missing locked Engine package sources`);
  }
  for (const observed of new Set(references)) {
    if (observed !== source.commit) {
      violations.push(
        `${relativePath}: Engine package commit expected ${source.commit}; observed ${observed}`,
      );
    }
  }
  for (const [name, path] of ENGINE_PACKAGES) {
    if (
      !content.includes(`${name}:`) &&
      !content.includes(`${name}@`) &&
      !content.includes(`'${name}@`)
    ) {
      violations.push(`${relativePath}: missing locked package ${name}`);
    }
    if (!content.includes(`#path:${path}`)) {
      violations.push(`${relativePath}: missing locked Engine path ${path}`);
    }
  }
  const enginePaths = [
    ...content.matchAll(
      /(?:github:FuzzySlipper\/rusty-engine#[^&\s'")}\]]+&path:|codeload\.github\.com\/FuzzySlipper\/rusty-engine\/tar\.gz\/[^#\s'")}\]]+#path:)([A-Za-z0-9_./-]+)/gimu,
    ),
  ].map((match) => match[1]);
  for (const observed of new Set(enginePaths)) {
    if (![...ENGINE_PACKAGES.values()].includes(observed)) {
      violations.push(
        `${relativePath}: unexpected Engine package path ${observed}`,
      );
    }
  }
  const repositorySpellings = [
    ...content.matchAll(
      /(?:github:|codeload\.github\.com\/)([^/\s]+\/rusty-engine)(?=[/#])/gimu,
    ),
  ].map((match) => match[1]);
  for (const observed of new Set(repositorySpellings)) {
    if (observed !== "FuzzySlipper/rusty-engine") {
      violations.push(
        `${relativePath}: Engine repository identity must use canonical spelling FuzzySlipper/rusty-engine; observed ${observed}`,
      );
    }
  }
  if (
    /(@rusty-engine\/[^\s]+)(?:file:|link:)|(?:file:|link:)[^\s]*rusty-engine/iu.test(
      content,
    )
  ) {
    violations.push(
      `${relativePath}: path, link, or sibling Engine package source is forbidden`,
    );
  }
}

function checkAdjacentDependencyManifests(repoRoot, violations) {
  for (const relativePath of discoverManifests(repoRoot, "package.json")) {
    if (DECLARED_PACKAGE_MANIFESTS.has(relativePath)) continue;
    let manifest;
    try {
      manifest = JSON.parse(
        readFileSync(resolve(repoRoot, relativePath), "utf8"),
      );
    } catch (error) {
      violations.push(
        `${relativePath}: cannot decode discovered package manifest: ${error.message}`,
      );
      continue;
    }
    for (const sectionName of dependencySectionNames()) {
      for (const [name, observed] of Object.entries(
        manifest[sectionName] ?? {},
      )) {
        if (isEnginePackageReference(name, observed)) {
          violations.push(
            `${relativePath}: unexpected Engine source ${name} in ${sectionName} (${String(observed)}); Engine packages are allowed only in declared carrier manifests`,
          );
        }
      }
    }
  }

  for (const relativePath of discoverManifests(repoRoot, "Cargo.toml")) {
    if (relativePath === "Cargo.toml") continue;
    const content = readFile(repoRoot, relativePath, violations);
    if (content === null) continue;
    for (const line of content.split("\n")) {
      const trimmed = line.trim();
      const directSource = trimmed.match(/(?:git|path)\s*=\s*"([^"]*)"/u)?.[1];
      if (
        (directSource !== undefined &&
          isEngineRepositoryReference(directSource)) ||
        ENGINE_CRATES.some((crate) => {
          if (trimmed === `${crate}.workspace = true`) return false;
          return (
            trimmed.startsWith(`${crate} =`) ||
            new RegExp(`\\bpackage\\s*=\\s*"${escapeRegExp(crate)}"`, "u").test(
              trimmed,
            )
          );
        })
      ) {
        violations.push(
          `${relativePath}: unexpected direct Engine dependency carrier ${trimmed}; workspace members must inherit declared Engine crates with .workspace = true`,
        );
      }
    }
  }
}

function rewritePackageManifest(path, expectedNames, previousCommit, commit) {
  const manifest = JSON.parse(readFileSync(path, "utf8"));
  for (const name of expectedNames) {
    const packagePath = ENGINE_PACKAGES.get(name);
    const expected = packageSpecifier(previousCommit, packagePath);
    if (manifest.dependencies?.[name] !== expected) {
      throw new Error(
        `${path}: ${name} changed before rewrite; expected ${expected}`,
      );
    }
    manifest.dependencies[name] = packageSpecifier(commit, packagePath);
  }
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
}

function rewriteWorkspacePolicy(path, previousCommit, commit) {
  let content = readFileSync(path, "utf8");
  for (const [name, packagePath] of ENGINE_PACKAGES) {
    const before = `"${name}@${codeloadSpecifier(previousCommit, packagePath)}": true`;
    const after = `"${name}@${codeloadSpecifier(commit, packagePath)}": true`;
    if (!content.includes(before)) {
      throw new Error(`${path}: missing active allowBuilds carrier ${before}`);
    }
    content = content.replace(before, after);
  }
  writeFileSync(path, content);
}

function replaceRequiredCommit(path, previousCommit, commit) {
  const content = readFileSync(path, "utf8");
  const occurrences = content.split(previousCommit).length - 1;
  if (occurrences !== ENGINE_CRATES.length + 1) {
    throw new Error(
      `${path}: expected ${String(ENGINE_CRATES.length + 1)} active commit carriers; observed ${String(occurrences)}`,
    );
  }
  writeFileSync(path, content.replaceAll(previousCommit, commit));
}

function assertCarrierFilesClean(repoRoot) {
  const output = git(repoRoot, [
    "status",
    "--porcelain=v1",
    "--",
    ...ACTIVE_CARRIER_PATHS,
  ]);
  if (output.trim().length > 0) {
    throw new Error(
      `active Engine carrier or lock files are dirty; preserve or commit them before update:\n${output.trim()}`,
    );
  }
}

function scopedDiff(repoRoot) {
  return git(repoRoot, ["diff", "--binary", "--", ...ACTIVE_CARRIER_PATHS]);
}

function readFile(repoRoot, relativePath, violations) {
  try {
    return readFileSync(resolve(repoRoot, relativePath), "utf8");
  } catch (error) {
    violations.push(
      `${relativePath}: missing or unreadable (${error.message})`,
    );
    return null;
  }
}

function expectTomlScalar(path, section, key, expected, violations) {
  const observed = section.match(
    new RegExp(`^${escapeRegExp(key)}\\s*=\\s*"([^"]*)"$`, "mu"),
  )?.[1];
  if (observed !== expected) {
    violations.push(
      `${path}: Engine metadata ${key} expected ${expected}; observed ${String(observed)}`,
    );
  }
}

function compareSets(path, expected, observed, violations) {
  const expectedSet = new Set(expected);
  const observedSet = new Set(observed);
  for (const value of expectedSet) {
    if (!observedSet.has(value)) violations.push(`${path}: missing ${value}`);
  }
  for (const value of observedSet) {
    if (!expectedSet.has(value))
      violations.push(`${path}: unexpected ${value}`);
  }
}

function dependencySectionNames() {
  return [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
  ];
}

function isEnginePackageReference(name, observed) {
  if (name.toLowerCase().startsWith("@rusty-engine/")) return true;
  if (typeof observed !== "string") return false;
  if (observed.toLowerCase().includes("@rusty-engine/")) return true;
  return isEngineRepositoryReference(observed);
}

function isEngineRepositoryReference(value) {
  const normalized = value.toLowerCase();
  return (
    normalized.includes("fuzzyslipper/rusty-engine") ||
    /(?:^|[/])rusty-engine(?:[/#&.]|$)/u.test(normalized)
  );
}

function discoverManifests(repoRoot, fileName) {
  const discovered = [];
  const visit = (directory, relativeDirectory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.isSymbolicLink()) continue;
      const relativePath =
        relativeDirectory.length === 0
          ? entry.name
          : `${relativeDirectory}/${entry.name}`;
      if (entry.isDirectory()) {
        if (!MANIFEST_SCAN_IGNORES.has(entry.name)) {
          visit(resolve(directory, entry.name), relativePath);
        }
      } else if (entry.isFile() && entry.name === fileName) {
        discovered.push(relativePath);
      }
    }
  };
  visit(repoRoot, "");
  return discovered.sort();
}

function packageSpecifier(commit, path) {
  return `github:FuzzySlipper/rusty-engine#${commit}&path:${path}`;
}

function codeloadSpecifier(commit, path) {
  return `https://codeload.github.com/FuzzySlipper/rusty-engine/tar.gz/${commit}#path:${path}`;
}

function assertCommit(commit, label) {
  if (typeof commit !== "string" || !COMMIT_PATTERN.test(commit)) {
    throw new Error(
      `${label} must be one lowercase 40-character hexadecimal commit; observed ${String(commit)}`,
    );
  }
}

function throwRevisionViolations(violations) {
  throw new Error(
    `Engine revision check failed:\n${violations
      .map((violation) => `- ${violation}`)
      .join("\n")}\nRepair with: ${REPAIR_COMMAND}`,
  );
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

function git(cwd, args) {
  return run("git", args, { cwd });
}

function run(command, args, { cwd, input, allowFailure = false } = {}) {
  const result = spawnSync(command, args, {
    cwd,
    input,
    encoding: "utf8",
    env: { ...process.env, NO_COLOR: "1" },
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0 && !allowFailure) {
    throw new Error(
      `${command} ${args.join(" ")} failed (${String(result.status)}):\n${result.stderr || result.stdout}`,
    );
  }
  return result.stdout ?? "";
}
