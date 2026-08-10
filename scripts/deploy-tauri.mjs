import { createHash, randomBytes } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  renameSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve, sep } from "node:path";
import process from "node:process";

const APP_IDENTIFIER = "dev.fuzzyslipper.rusty-engine-demo.loading-bay";
const APP_RESOURCE_NAME = "Loading Bay";
const APP_BINARY = "loading-bay-desktop";
const SIDECAR_BINARY = "loading-bay-browser-host";
const STATE_SCHEMA_VERSION = 1;

function fail(message) {
  throw new Error(message);
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function exactSha(value, label) {
  if (!/^[0-9a-f]{40}$/.test(value ?? "")) {
    fail(`${label} must be an exact lowercase 40-character Git SHA`);
  }
  return value;
}

function exactDigest(value, label) {
  if (!/^[0-9a-f]{64}$/.test(value ?? "")) {
    fail(`${label} must be an exact lowercase SHA-256 digest`);
  }
  return value;
}

function parseArguments(argv) {
  const [command, ...rest] = argv;
  const options = {};
  for (let index = 0; index < rest.length; index += 1) {
    const token = rest[index];
    if (!token.startsWith("--")) fail(`unexpected argument ${token}`);
    const key = token.slice(2);
    if (key === "purge-data") {
      options.purgeData = true;
      continue;
    }
    const value = rest[index + 1];
    if (!value || value.startsWith("--")) fail(`missing value for ${token}`);
    options[key] = value;
    index += 1;
  }
  return { command, options };
}

function environmentPaths(env = process.env) {
  const home = resolve(env.HOME ?? fail("HOME is required"));
  const dataHome = resolve(env.XDG_DATA_HOME ?? join(home, ".local/share"));
  const cacheHome = resolve(env.XDG_CACHE_HOME ?? join(home, ".cache"));
  const binHome = resolve(env.XDG_BIN_HOME ?? join(home, ".local/bin"));
  const installRoot = resolve(
    env.LOADING_BAY_INSTALL_ROOT ??
      join(dataHome, "rusty-engine-demo", "desktop"),
  );
  return {
    home,
    dataHome,
    cacheHome,
    binHome,
    installRoot,
    releasesRoot: join(installRoot, "releases"),
    statePath: join(installRoot, "deployment.json"),
    currentLink: join(installRoot, "current"),
    previousLink: join(installRoot, "previous"),
    launcherPath: join(binHome, "loading-bay"),
    desktopEntryPath: join(dataHome, "applications", "loading-bay.desktop"),
    iconRoot: join(dataHome, "icons", "hicolor"),
    appDataRoot: join(dataHome, APP_IDENTIFIER),
    saveRoot: join(dataHome, APP_IDENTIFIER, "saves"),
    cacheRoot: join(cacheHome, APP_IDENTIFIER),
    logRoot: join(dataHome, APP_IDENTIFIER, "logs"),
  };
}

function atomicWrite(path, contents, mode) {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = join(
    dirname(path),
    `.${basename(path)}.${process.pid}.${randomBytes(6).toString("hex")}.tmp`,
  );
  writeFileSync(temporary, contents, { mode });
  if (mode !== undefined) chmodSync(temporary, mode);
  renameSync(temporary, path);
}

function atomicLink(path, target) {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.${process.pid}.${randomBytes(6).toString("hex")}.tmp`;
  symlinkSync(target, temporary);
  renameSync(temporary, path);
}

function readLinkRelease(path) {
  if (!existsSync(path) && !lstatExists(path)) return null;
  const link = readlinkSync(path);
  const normalized = link.split(sep).join("/");
  if (!/^releases\/[0-9a-f]{40}-[0-9a-f]{12}$/.test(normalized)) {
    fail(`deployment link ${path} has unsafe target ${link}`);
  }
  return basename(normalized);
}

function lstatExists(path) {
  try {
    lstatSync(path);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

function readJson(path, label) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    fail(`${label} ${path} is invalid: ${error.message}`);
  }
}

function validateRelativePath(path) {
  if (
    typeof path !== "string" ||
    path.length === 0 ||
    path.startsWith("/") ||
    path.split(/[\\/]/).some((part) => part === ".." || part === "")
  ) {
    fail(`package manifest contains unsafe path ${JSON.stringify(path)}`);
  }
}

function verifyFile(path, expected, label) {
  if (!existsSync(path) || !statSync(path).isFile()) {
    fail(`${label} is missing: ${path}`);
  }
  const metadata = statSync(path);
  if (metadata.size !== expected.byteLen) {
    fail(`${label} has ${metadata.size} bytes; expected ${expected.byteLen}`);
  }
  const actual = sha256(path);
  if (actual !== exactDigest(expected.sha256, `${label} digest`)) {
    fail(`${label} hash mismatch: got ${actual}, expected ${expected.sha256}`);
  }
}

function verifyRelease(releaseRoot, evidence, expectedSourceRevision) {
  const application = join(releaseRoot, "usr/bin", APP_BINARY);
  const sidecar = join(releaseRoot, "usr/bin", SIDECAR_BINARY);
  const resourceRoot = join(releaseRoot, "usr/lib", APP_RESOURCE_NAME);
  const manifestPath = join(resourceRoot, "desktop-package-manifest.json");
  for (const [path, label] of [
    [application, "desktop application"],
    [sidecar, "desktop sidecar"],
  ]) {
    if (!existsSync(path) || !statSync(path).isFile()) {
      fail(`${label} is missing: ${path}`);
    }
  }
  verifyFile(manifestPath, evidence.files.manifest, "desktop package manifest");
  const manifest = readJson(manifestPath, "desktop package manifest");
  if (manifest.schemaVersion !== 1)
    fail("unsupported desktop package manifest");
  if (manifest.sourceRevision !== expectedSourceRevision) {
    fail(
      `manifest source ${manifest.sourceRevision} does not match ${expectedSourceRevision}`,
    );
  }
  let sidecarCount = 0;
  for (const file of manifest.files ?? []) {
    validateRelativePath(file.path);
    const path =
      file.kind === "sidecar"
        ? sidecar
        : join(resourceRoot, ...file.path.split("/"));
    if (file.kind === "sidecar") sidecarCount += 1;
    verifyFile(path, file, `manifest file ${file.path}`);
  }
  if (sidecarCount !== 1) {
    fail(`desktop package manifest has ${sidecarCount} sidecars; expected one`);
  }
  for (const size of ["32x32", "128x128", "256x256@2", "512x512"]) {
    const icon = join(
      releaseRoot,
      "usr/share/icons/hicolor",
      size,
      "apps/loading-bay-desktop.png",
    );
    if (!existsSync(icon) || !statSync(icon).isFile()) {
      fail(`packaged desktop icon is missing: ${icon}`);
    }
  }
  return {
    application,
    sidecar,
    resourceRoot,
    manifestPath,
    manifest,
    installedApplication: {
      byteLen: statSync(application).size,
      sha256: sha256(application),
    },
    installedSidecar: {
      byteLen: statSync(sidecar).size,
      sha256: sha256(sidecar),
    },
  };
}

function quoteShell(value) {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function quoteDesktopExec(value) {
  return `"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"').replaceAll("$", "\\$").replaceAll("`", "\\`")}"`;
}

function installEntryPoints(paths, releaseRoot) {
  const executable = join(paths.installRoot, "current/usr/bin", APP_BINARY);
  atomicWrite(
    paths.launcherPath,
    `#!/usr/bin/env bash\nset -euo pipefail\nexec ${quoteShell(executable)} "$@"\n`,
    0o755,
  );
  atomicWrite(
    paths.desktopEntryPath,
    `[Desktop Entry]\nCategories=Game;\nComment=An original industrial first-person exploration game built with Rusty Engine.\nExec=${quoteDesktopExec(paths.launcherPath)}\nStartupWMClass=loading-bay-desktop\nIcon=loading-bay-desktop\nName=Loading Bay\nTerminal=false\nType=Application\nX-Rusty-Engine-Demo-Managed=true\n`,
    0o644,
  );
  const iconMappings = [
    ["32x32", "32x32"],
    ["128x128", "128x128"],
    ["256x256@2", "256x256@2"],
    ["512x512", "512x512"],
  ];
  for (const [sourceSize, destinationSize] of iconMappings) {
    const source = join(
      releaseRoot,
      "usr/share/icons/hicolor",
      sourceSize,
      "apps/loading-bay-desktop.png",
    );
    const destination = join(
      paths.iconRoot,
      destinationSize,
      "apps/loading-bay-desktop.png",
    );
    if (!existsSync(source))
      fail(`packaged desktop icon is missing: ${source}`);
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(source, destination);
    chmodSync(destination, 0o644);
  }
  return executable;
}

function readState(paths) {
  if (!existsSync(paths.statePath)) return null;
  const state = readJson(paths.statePath, "deployment state");
  if (state.schemaVersion !== STATE_SCHEMA_VERSION) {
    fail(`unsupported deployment state schema ${state.schemaVersion}`);
  }
  return state;
}

function releaseRecord(
  releaseId,
  releaseRoot,
  evidence,
  artifact,
  artifactSha256,
  verification,
) {
  return {
    releaseId,
    sourceRevision: evidence.sourceRevision,
    appVersion: readJson(
      join(
        releaseRoot,
        "usr/lib",
        APP_RESOURCE_NAME,
        "desktop-package-manifest.json",
      ),
      "desktop package manifest",
    ).appVersion,
    artifact: {
      fileName: basename(artifact),
      byteLen: statSync(artifact).size,
      sha256: artifactSha256,
    },
    installedApplication: verification.installedApplication,
    installedSidecar: verification.installedSidecar,
    directBuildEvidence: {
      application: evidence.files.application,
      sidecar: evidence.files.sidecar,
    },
  };
}

function deploymentReceipt(paths, state, action) {
  return {
    schemaVersion: STATE_SCHEMA_VERSION,
    action,
    active: state?.active ?? null,
    previous: state?.previous ?? null,
    installRoot: paths.installRoot,
    launcherPath: paths.launcherPath,
    desktopEntryPath: paths.desktopEntryPath,
    appDataRoot: paths.appDataRoot,
    saveRoot: paths.saveRoot,
    cacheRoot: paths.cacheRoot,
    logRoot: paths.logRoot,
  };
}

function install(options, paths) {
  const artifact = resolve(options.artifact ?? fail("--artifact is required"));
  const evidencePath = resolve(
    options.evidence ?? fail("--evidence is required"),
  );
  const expectedArtifactSha = exactDigest(
    options["artifact-sha256"],
    "--artifact-sha256",
  );
  const expectedSourceRevision = exactSha(
    options["source-revision"],
    "--source-revision",
  );
  if (!existsSync(artifact) || !statSync(artifact).isFile()) {
    fail(`deployment artifact is missing: ${artifact}`);
  }
  const artifactSha = sha256(artifact);
  if (artifactSha !== expectedArtifactSha) {
    fail(
      `artifact hash mismatch: got ${artifactSha}, expected ${expectedArtifactSha}`,
    );
  }
  const evidence = readJson(evidencePath, "Tauri package evidence");
  if (evidence.schemaVersion !== 1) fail("unsupported Tauri package evidence");
  if (evidence.sourceRevision !== expectedSourceRevision) {
    fail(
      `evidence source ${evidence.sourceRevision} does not match ${expectedSourceRevision}`,
    );
  }

  requireManagedOrAbsent(paths.launcherPath, paths.installRoot);
  requireManagedOrAbsent(
    paths.desktopEntryPath,
    "X-Rusty-Engine-Demo-Managed=true",
  );
  const oldState = readState(paths);
  const oldActive = readLinkRelease(paths.currentLink);
  const oldPrevious = readLinkRelease(paths.previousLink);
  if ((oldState === null) !== (oldActive === null)) {
    fail("deployment state and active release link are inconsistent");
  }
  if (
    oldState !== null &&
    (oldState.active !== oldActive || oldState.previous !== oldPrevious)
  ) {
    fail("deployment state does not match active/previous release links");
  }

  mkdirSync(paths.releasesRoot, { recursive: true });
  const releaseId = `${expectedSourceRevision}-${artifactSha.slice(0, 12)}`;
  const releaseRoot = join(paths.releasesRoot, releaseId);
  const staging = join(
    paths.releasesRoot,
    `.incoming-${process.pid}-${randomBytes(6).toString("hex")}`,
  );
  let preparedRoot = releaseRoot;
  let verification;
  try {
    if (!existsSync(releaseRoot)) {
      mkdirSync(staging, { recursive: true });
      execFileSync("dpkg-deb", ["-x", artifact, staging], { stdio: "pipe" });
      verifyRelease(staging, evidence, expectedSourceRevision);
      renameSync(staging, releaseRoot);
      verification = verifyRelease(
        releaseRoot,
        evidence,
        expectedSourceRevision,
      );
    } else {
      verification = verifyRelease(
        releaseRoot,
        evidence,
        expectedSourceRevision,
      );
    }
    preparedRoot = releaseRoot;
  } finally {
    if (existsSync(staging)) rmSync(staging, { recursive: true, force: true });
  }

  installEntryPoints(paths, preparedRoot);
  if (oldActive && oldActive !== releaseId) {
    atomicLink(paths.previousLink, join("releases", oldActive));
  }
  atomicLink(paths.currentLink, join("releases", releaseId));
  const previous = readLinkRelease(paths.previousLink);
  const releases = {};
  if (previous !== null && oldState?.releases?.[previous] !== undefined) {
    releases[previous] = oldState.releases[previous];
  }
  releases[releaseId] = releaseRecord(
    releaseId,
    preparedRoot,
    evidence,
    artifact,
    artifactSha,
    verification,
  );
  const state = {
    schemaVersion: STATE_SCHEMA_VERSION,
    active: releaseId,
    previous,
    releases,
    paths: deploymentReceipt(paths, null, "paths"),
    history: [
      ...(oldState?.history ?? []),
      {
        action: oldActive && oldActive !== releaseId ? "upgrade" : "install",
        releaseId,
        recordedAt: new Date().toISOString(),
      },
    ].slice(-32),
  };
  atomicWrite(paths.statePath, `${JSON.stringify(state, null, 2)}\n`, 0o644);
  const retainedReleases = new Set([releaseId, previous].filter(Boolean));
  for (const entry of readdirSync(paths.releasesRoot)) {
    if (
      /^[0-9a-f]{40}-[0-9a-f]{12}$/.test(entry) &&
      !retainedReleases.has(entry)
    ) {
      rmSync(join(paths.releasesRoot, entry), { recursive: true, force: true });
    }
  }
  return deploymentReceipt(
    paths,
    state,
    oldActive === releaseId ? "install" : oldActive ? "upgrade" : "install",
  );
}

function rollback(paths) {
  const state = readState(paths) ?? fail("Loading Bay is not installed");
  const active = readLinkRelease(paths.currentLink);
  const previous = readLinkRelease(paths.previousLink);
  if (!active || !previous)
    fail("no previous Loading Bay release is available");
  const previousRoot = join(paths.releasesRoot, previous);
  if (!existsSync(previousRoot))
    fail(`previous release is missing: ${previous}`);
  atomicLink(paths.currentLink, join("releases", previous));
  atomicLink(paths.previousLink, join("releases", active));
  installEntryPoints(paths, previousRoot);
  const next = {
    ...state,
    active: previous,
    previous: active,
    history: [
      ...(state.history ?? []),
      {
        action: "rollback",
        releaseId: previous,
        recordedAt: new Date().toISOString(),
      },
    ].slice(-32),
  };
  atomicWrite(paths.statePath, `${JSON.stringify(next, null, 2)}\n`, 0o644);
  return deploymentReceipt(paths, next, "rollback");
}

function status(paths) {
  const state = readState(paths) ?? fail("Loading Bay is not installed");
  const active = readLinkRelease(paths.currentLink);
  if (active !== state.active) {
    fail(
      `active deployment link ${active} does not match state ${state.active}`,
    );
  }
  const release =
    state.releases?.[active] ?? fail(`active release ${active} is unrecorded`);
  const application = join(paths.currentLink, "usr/bin", APP_BINARY);
  if (sha256(application) !== release.installedApplication.sha256) {
    fail("active desktop application does not match deployment state");
  }
  const sidecar = join(paths.currentLink, "usr/bin", SIDECAR_BINARY);
  if (sha256(sidecar) !== release.installedSidecar.sha256) {
    fail("active desktop sidecar does not match deployment state");
  }
  return { ...deploymentReceipt(paths, state, "status"), release };
}

function removeIfManaged(path, marker) {
  if (!existsSync(path)) return;
  const source = readFileSync(path, "utf8");
  if (!source.includes(marker))
    fail(`refusing to remove unmanaged file ${path}`);
  rmSync(path);
}

function requireManagedOrAbsent(path, marker) {
  if (!existsSync(path)) return;
  const source = readFileSync(path, "utf8");
  if (!source.includes(marker)) {
    fail(`refusing to replace unmanaged file ${path}`);
  }
}

function uninstall(options, paths) {
  const state = readState(paths);
  if (!state) fail("Loading Bay is not installed");
  removeIfManaged(paths.launcherPath, paths.installRoot);
  removeIfManaged(paths.desktopEntryPath, "X-Rusty-Engine-Demo-Managed=true");
  for (const size of ["32x32", "128x128", "256x256@2", "512x512"]) {
    const icon = join(paths.iconRoot, size, "apps/loading-bay-desktop.png");
    if (existsSync(icon)) rmSync(icon);
  }
  rmSync(paths.installRoot, { recursive: true, force: true });
  if (options.purgeData) {
    for (const root of [paths.appDataRoot, paths.cacheRoot]) {
      if (existsSync(root)) rmSync(root, { recursive: true, force: true });
    }
  }
  return {
    ...deploymentReceipt(
      paths,
      null,
      options.purgeData ? "uninstall-purge" : "uninstall",
    ),
    dataPreserved: !options.purgeData,
  };
}

export function run(argv, env = process.env) {
  const { command, options } = parseArguments(argv);
  const paths = environmentPaths(env);
  switch (command) {
    case "install":
      return install(options, paths);
    case "rollback":
      return rollback(paths);
    case "status":
      return status(paths);
    case "uninstall":
      return uninstall(options, paths);
    default:
      fail(
        "usage: deploy-tauri.mjs <install|status|rollback|uninstall> [options]",
      );
  }
}

if (process.argv[1] === new URL(import.meta.url).pathname) {
  try {
    process.stdout.write(
      `${JSON.stringify(run(process.argv.slice(2)), null, 2)}\n`,
    );
  } catch (error) {
    process.stderr.write(`Loading Bay deployment failed: ${error.message}\n`);
    process.exitCode = 1;
  }
}
