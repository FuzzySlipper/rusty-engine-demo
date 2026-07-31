import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = join(repoRoot, "src-tauri");
const browserDist = join(repoRoot, "dist/apps/loading-bay/browser");
const contentRoot = join(repoRoot, "content");
const targetTriple =
  process.env.TAURI_ENV_TARGET_TRIPLE ??
  execFileSync("rustc", ["--print", "host-tuple"], {
    cwd: repoRoot,
    encoding: "utf8",
  }).trim();
const targetDirectory = resolve(
  repoRoot,
  process.env.CARGO_TARGET_DIR ?? "target",
);
const releaseDirectory = join(targetDirectory, targetTriple, "release");
const cargoReleaseDirectory = releaseDirectory;
const sourceSidecar = join(cargoReleaseDirectory, "browser-host");
const bundledSidecar = join(
  tauriRoot,
  "binaries",
  `loading-bay-browser-host-${targetTriple}`,
);
const directSidecar = join(cargoReleaseDirectory, "loading-bay-browser-host");
const manifestPath = join(
  tauriRoot,
  "resources",
  "desktop-package-manifest.json",
);
const directResourceRoot = join(targetDirectory, "lib", "loading-bay-desktop");

function run(command, args) {
  execFileSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
    env: process.env,
  });
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function walk(root) {
  const files = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      files.push(...walk(path));
    } else if (entry.isFile()) {
      files.push(path);
    }
  }
  return files.sort();
}

function packageEntry(path, logicalPath, kind = "resource") {
  return {
    path: logicalPath.split(sep).join("/"),
    byteLen: statSync(path).size,
    sha256: sha256(path),
    kind,
  };
}

function assertInputs() {
  if (!existsSync(join(browserDist, "index.html"))) {
    throw new Error(`browser production output is missing: ${browserDist}`);
  }
  if (!existsSync(join(contentRoot, "projects/loading-bay.project.json"))) {
    throw new Error("canonical Loading Bay project is missing");
  }
}

export function preparePackage({ buildSidecar = true } = {}) {
  assertInputs();
  if (buildSidecar) {
    run("cargo", [
      "build",
      "--locked",
      "--release",
      "--target",
      targetTriple,
      "-p",
      "loading-bay-game",
      "--bin",
      "browser-host",
    ]);
  }
  if (!existsSync(sourceSidecar)) {
    throw new Error(`release browser-host is missing: ${sourceSidecar}`);
  }

  mkdirSync(dirname(bundledSidecar), { recursive: true });
  copyFileSync(sourceSidecar, bundledSidecar);
  copyFileSync(sourceSidecar, directSidecar);
  chmodSync(bundledSidecar, 0o755);
  chmodSync(directSidecar, 0o755);

  const sourceRevision = execFileSync("git", ["rev-parse", "HEAD"], {
    cwd: repoRoot,
    encoding: "utf8",
  }).trim();
  if (!/^[0-9a-f]{40}$/.test(sourceRevision)) {
    throw new Error(
      `git did not return an exact source revision: ${sourceRevision}`,
    );
  }

  const files = [
    ...walk(browserDist).map((path) =>
      packageEntry(path, join("web", relative(browserDist, path))),
    ),
    ...walk(contentRoot).map((path) =>
      packageEntry(path, join("content", relative(contentRoot, path))),
    ),
    packageEntry(sourceSidecar, "loading-bay-browser-host", "sidecar"),
  ].sort((left, right) =>
    left.path < right.path ? -1 : left.path > right.path ? 1 : 0,
  );
  const manifest = {
    schemaVersion: 1,
    sourceRevision,
    appVersion: "0.1.0",
    targetTriple,
    files,
  };

  mkdirSync(dirname(manifestPath), { recursive: true });
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

  rmSync(directResourceRoot, { recursive: true, force: true });
  mkdirSync(directResourceRoot, { recursive: true });
  cpSync(browserDist, join(directResourceRoot, "web"), { recursive: true });
  cpSync(contentRoot, join(directResourceRoot, "content"), { recursive: true });
  copyFileSync(manifestPath, join(directResourceRoot, basename(manifestPath)));

  return {
    manifest,
    manifestPath,
    bundledSidecar,
    directSidecar,
    directResourceRoot,
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const result = preparePackage();
  process.stdout.write(
    `prepared Tauri package target=${targetTriple} files=${result.manifest.files.length} revision=${result.manifest.sourceRevision}\n`,
  );
}
