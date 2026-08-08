import {
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { relative, resolve } from "node:path";
import { spawnSync } from "node:child_process";

export const ENGINE_REPOSITORY = "https://github.com/FuzzySlipper/rusty-engine";
export const COMMIT_PATTERN = /^[0-9a-f]{40}$/u;
export const DEVELOPMENT_REF = "refs/heads/main";
export const DEVELOPMENT_MANIFEST = "engine-development.json";
export const DEVELOPMENT_REPORT = ".engine-development/resolution.json";

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
export const BROWSER_ENGINE_PACKAGES = new Set();
export const ENGINE_CRATES = ["rusty-engine"];
export const ACTIVE_CARRIER_PATHS = [
  "engine-source.json",
  "Cargo.toml",
  "Cargo.lock",
  "package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
];

const STUDIO_PACKAGES = new Set(
  [...ENGINE_PACKAGES.keys()].filter((name) => name.includes("/studio-")),
);
const STUDIO_RENDERER_RESOLVERS = new Set(
  [...ENGINE_PACKAGES.keys()].filter((name) => !name.includes("/studio-")),
);

export function loadEngineSource(repoRoot) {
  const source = readJson(resolve(repoRoot, "engine-source.json"));
  const fields = Object.keys(source).sort();
  if (
    fields.join(",") !== "commit,repository,schemaVersion" ||
    source.schemaVersion !== 1 ||
    source.repository !== ENGINE_REPOSITORY ||
    typeof source.commit !== "string" ||
    !COMMIT_PATTERN.test(source.commit)
  ) {
    throw new Error(
      "engine-source.json: expected exactly commit, repository, schemaVersion with canonical public identity",
    );
  }
  return source;
}

export function loadEngineDevelopment(repoRoot) {
  const intent = readJson(resolve(repoRoot, DEVELOPMENT_MANIFEST));
  if (
    intent.schemaVersion !== 1 ||
    intent.repository !== ENGINE_REPOSITORY ||
    intent.ref !== DEVELOPMENT_REF ||
    Object.keys(intent).sort().join(",") !== "ref,repository,schemaVersion"
  ) {
    throw new Error(
      `${DEVELOPMENT_MANIFEST}: development intent may select only ${DEVELOPMENT_REF}`,
    );
  }
  return intent;
}

export function readDevelopmentResolution(repoRoot) {
  const report = readJson(resolve(repoRoot, DEVELOPMENT_REPORT));
  if (
    report.schemaVersion !== 1 ||
    report.mode !== "development" ||
    report.repository !== ENGINE_REPOSITORY ||
    report.requestedRef !== DEVELOPMENT_REF ||
    typeof report.resolvedCommit !== "string" ||
    !COMMIT_PATTERN.test(report.resolvedCommit) ||
    report.certification !== false
  ) {
    throw new Error(`${DEVELOPMENT_REPORT}: invalid development resolution`);
  }
  return report;
}

export function checkDevelopmentResolution(repoRoot) {
  const intent = loadEngineDevelopment(repoRoot);
  const report = readDevelopmentResolution(repoRoot);
  if (report.applied === true) {
    const source = checkEngineRevision(repoRoot);
    if (source.commit !== report.resolvedCommit) {
      throw new Error(
        `${DEVELOPMENT_REPORT}: applied report does not match active Engine revision`,
      );
    }
  }
  return { intent, report };
}

export async function resolvePublicDevelopmentRef(
  repository = ENGINE_REPOSITORY,
  ref = DEVELOPMENT_REF,
) {
  const result = spawnSync("git", ["ls-remote", repository, ref], {
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || `could not resolve ${repository} ${ref}`);
  }
  const [commit, observedRef] = result.stdout.trim().split(/\s+/u);
  if (!COMMIT_PATTERN.test(commit ?? "") || observedRef !== ref) {
    throw new Error(`could not resolve an exact public SHA for ${ref}`);
  }
  return commit;
}

export async function syncDevelopmentRevision({
  repoRoot,
  worktree,
  reportOnly = false,
}) {
  const intent = loadEngineDevelopment(repoRoot);
  let resolvedCommit;
  let source = "public";
  let sourcePath = null;
  let dirty = false;
  if (worktree !== undefined) {
    source = "worktree";
    sourcePath = resolve(worktree);
    resolvedCommit = runGit(sourcePath, ["rev-parse", "HEAD"]);
    dirty = runGit(sourcePath, ["status", "--porcelain"]).length > 0;
  } else {
    resolvedCommit = await resolvePublicDevelopmentRef(
      intent.repository,
      intent.ref,
    );
  }
  const report = {
    schemaVersion: 1,
    mode: "development",
    repository: intent.repository,
    requestedRef: intent.ref,
    resolvedCommit,
    source,
    sourcePath,
    dirty,
    certification: false,
    applied: !reportOnly,
  };
  if (!reportOnly) {
    rewriteActiveCarriers(
      repoRoot,
      loadEngineSource(repoRoot).commit,
      resolvedCommit,
    );
    refreshLocks(repoRoot);
    writeJson(resolve(repoRoot, DEVELOPMENT_REPORT), report);
    checkEngineRevision(repoRoot);
  }
  return { report };
}

export function checkEngineRevision(repoRoot) {
  const source = loadEngineSource(repoRoot);
  const commit = source.commit;
  const cargo = readFileSync(resolve(repoRoot, "Cargo.toml"), "utf8");
  const canonical = [
    ...cargo.matchAll(
      /^([a-z0-9-]+)\s*=\s*\{[^\n]*git\s*=\s*"https:\/\/github\.com\/FuzzySlipper\/rusty-engine"[^\n]*\}$/gmu,
    ),
  ];
  if (
    canonical.length !== 1 ||
    canonical[0][1] !== "rusty-engine" ||
    !canonical[0][0].includes('branch = "main"') ||
    /\brev\s*=|\btag\s*=|\.git"/u.test(canonical[0][0])
  ) {
    throw new Error(
      "Cargo.toml: expected exactly one rolling rusty-engine facade dependency on branch main",
    );
  }
  if (!cargo.includes(`revision = "${commit}"`)) {
    throw new Error(`Cargo.toml: Engine metadata revision must be ${commit}`);
  }
  for (const manifest of collectNamed(repoRoot, "Cargo.toml")) {
    if (manifest === resolve(repoRoot, "Cargo.toml")) continue;
    const content = readFileSync(manifest, "utf8");
    if (/FuzzySlipper\/rusty-engine/iu.test(content)) {
      throw new Error(
        `${relative(repoRoot, manifest)}: unexpected direct Engine dependency carrier`,
      );
    }
  }
  const cargoLock = readFileSync(resolve(repoRoot, "Cargo.lock"), "utf8");
  if (
    !cargoLock.includes('name = "rusty-engine"') ||
    !cargoLock.includes(
      `source = "git+${ENGINE_REPOSITORY}?branch=main#${commit}"`,
    )
  ) {
    throw new Error(
      `Cargo.lock: rusty-engine facade is not resolved at ${commit}`,
    );
  }

  const rootPackage = readJson(resolve(repoRoot, "package.json"));
  validatePackageSection(
    rootPackage.dependencies,
    STUDIO_PACKAGES,
    commit,
    "dependencies",
  );
  validatePackageSection(
    rootPackage.devDependencies,
    STUDIO_RENDERER_RESOLVERS,
    commit,
    "devDependencies",
  );
  const browserPackage = readJson(
    resolve(repoRoot, "ts/packages/browser-shell/package.json"),
  );
  for (const section of ["dependencies", "devDependencies"]) {
    for (const name of Object.keys(browserPackage[section] ?? {})) {
      if (name.startsWith("@rusty-engine/")) {
        throw new Error(
          `ts/packages/browser-shell/package.json: unexpected Engine package ${name}`,
        );
      }
    }
  }

  const pnpmLock = readFileSync(resolve(repoRoot, "pnpm-lock.yaml"), "utf8");
  const observed = [
    ...pnpmLock.matchAll(
      /(?:rusty-engine#|rusty-engine\/tar\.gz\/)([0-9a-f]{40})/gu,
    ),
  ].map((match) => match[1]);
  if (observed.length === 0 || observed.some((value) => value !== commit)) {
    throw new Error(
      `pnpm-lock.yaml: Engine packages are not coherent at ${commit}`,
    );
  }
  const workspace = readFileSync(
    resolve(repoRoot, "pnpm-workspace.yaml"),
    "utf8",
  );
  const workspaceCommits = [
    ...workspace.matchAll(/rusty-engine\/tar\.gz\/([0-9a-f]{40})/gu),
  ].map((match) => match[1]);
  if (
    workspaceCommits.length !== ENGINE_PACKAGES.size ||
    workspaceCommits.some((value) => value !== commit)
  ) {
    throw new Error(
      `pnpm-workspace.yaml: Engine allowBuilds must cover the exact ${commit} package closure`,
    );
  }
  return source;
}

export async function updateEngineRevision({
  repoRoot,
  commit,
  dryRun = false,
  provePublic = provePublicCommit,
}) {
  if (!COMMIT_PATTERN.test(commit ?? "")) {
    throw new Error("Engine revision must be a lowercase 40-character SHA");
  }
  await provePublic(ENGINE_REPOSITORY, commit);
  const before = loadEngineSource(repoRoot);
  if (dryRun) {
    return {
      before,
      commit,
      dryRun: true,
      diff: `${before.commit} -> ${commit}\n`,
    };
  }
  rewriteActiveCarriers(repoRoot, before.commit, commit);
  refreshLocks(repoRoot);
  const source = checkEngineRevision(repoRoot);
  return {
    before,
    commit: source.commit,
    dryRun: false,
    diff: `${before.commit} -> ${commit}\n`,
  };
}

export function rewriteActiveCarriers(repoRoot, previousCommit, commit) {
  for (const relativePath of ACTIVE_CARRIER_PATHS) {
    const path = resolve(repoRoot, relativePath);
    const content = readFileSync(path, "utf8");
    writeFileSync(path, content.replaceAll(previousCommit, commit));
  }
}

export async function provePublicCommit(repository, commit) {
  const probe = mkdtempSync(resolve(tmpdir(), "rusty-engine-public-commit-"));
  try {
    run(probe, "git", ["init", "--quiet"]);
    run(probe, "git", ["fetch", "--quiet", "--depth=1", repository, commit]);
    const fetched = runGit(probe, ["rev-parse", "FETCH_HEAD"]);
    if (fetched !== commit) {
      throw new Error(`fetched ${fetched} instead of ${commit}`);
    }
  } catch (error) {
    throw new Error(
      `Engine revision ${commit} is not public at ${repository}: ${error instanceof Error ? error.message : String(error)}`,
    );
  } finally {
    rmSync(probe, { recursive: true, force: true });
  }
}

function validatePackageSection(section, expectedNames, commit, label) {
  const observedNames = new Set(
    Object.keys(section ?? {}).filter((name) =>
      name.startsWith("@rusty-engine/"),
    ),
  );
  for (const name of observedNames) {
    if (!expectedNames.has(name)) {
      throw new Error(
        `package.json: unexpected Engine package ${name} in ${label}`,
      );
    }
  }
  for (const name of expectedNames) {
    const path = ENGINE_PACKAGES.get(name);
    const expected = `github:FuzzySlipper/rusty-engine#${commit}&path:${path}`;
    if (section?.[name] !== expected) {
      throw new Error(
        `package.json: ${name} must resolve exact Engine ${commit} in ${label}`,
      );
    }
  }
}

function refreshLocks(repoRoot) {
  run(repoRoot, "cargo", ["update", "-p", "rusty-engine"]);
  run(repoRoot, "pnpm", ["install", "--lockfile-only"]);
}

function run(cwd, command, args) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || `${command} failed`);
  }
  return result.stdout.trim();
}

function runGit(cwd, args) {
  return run(cwd, "git", args);
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function collectNamed(root, name) {
  const ignored = new Set([".git", "node_modules", "target", "dist"]);
  const results = [];
  function visit(path) {
    const stat = statSync(path);
    if (!stat.isDirectory()) {
      if (path.endsWith(`/${name}`)) results.push(path);
      return;
    }
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      if (entry.isDirectory() && ignored.has(entry.name)) continue;
      visit(resolve(path, entry.name));
    }
  }
  visit(root);
  return results;
}
