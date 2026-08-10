import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readlinkSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";

import { run } from "./deploy-tauri.mjs";

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function write(path, contents, mode = 0o644) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents, { mode });
  chmodSync(path, mode);
}

function fixture(root, revision, label) {
  const packageRoot = join(root, `package-${label}`);
  const resourceRoot = join(packageRoot, "usr/lib/Loading Bay");
  const application = join(packageRoot, "usr/bin/loading-bay-desktop");
  const sidecar = join(packageRoot, "usr/bin/loading-bay-browser-host");
  const web = join(resourceRoot, "web/index.html");
  write(
    join(packageRoot, "DEBIAN/control"),
    `Package: loading-bay-${label}\nVersion: 0.1.0\nArchitecture: amd64\nMaintainer: test\nDescription: deployment fixture\n`,
  );
  write(application, `desktop-${label}`, 0o755);
  write(sidecar, `sidecar-${label}`, 0o755);
  write(web, `<title>${label}</title>`);
  for (const size of ["32x32", "128x128", "256x256@2", "512x512"]) {
    write(
      join(
        packageRoot,
        `usr/share/icons/hicolor/${size}/apps/loading-bay-desktop.png`,
      ),
      `icon-${size}-${label}`,
    );
  }
  const manifestPath = join(resourceRoot, "desktop-package-manifest.json");
  const manifest = {
    schemaVersion: 1,
    sourceRevision: revision,
    appVersion: "0.1.0",
    targetTriple: "x86_64-unknown-linux-gnu",
    files: [
      {
        path: "loading-bay-browser-host",
        byteLen: statSync(sidecar).size,
        sha256: sha256(sidecar),
        kind: "sidecar",
      },
      {
        path: "web/index.html",
        byteLen: statSync(web).size,
        sha256: sha256(web),
        kind: "resource",
      },
    ],
  };
  write(manifestPath, `${JSON.stringify(manifest)}\n`);
  const deb = join(root, `loading-bay-${label}.deb`);
  execFileSync("dpkg-deb", ["--build", packageRoot, deb], { stdio: "pipe" });
  const evidence = {
    schemaVersion: 1,
    sourceRevision: revision,
    files: {
      application: {
        byteLen: Buffer.byteLength(`desktop-${label}-before-bundler-strip`),
        sha256: createHash("sha256")
          .update(`desktop-${label}-before-bundler-strip`)
          .digest("hex"),
      },
      sidecar: { byteLen: statSync(sidecar).size, sha256: sha256(sidecar) },
      manifest: {
        byteLen: statSync(manifestPath).size,
        sha256: sha256(manifestPath),
      },
    },
  };
  const evidencePath = join(root, `evidence-${label}.json`);
  write(evidencePath, `${JSON.stringify(evidence)}\n`);
  return { deb, evidencePath, sha256: sha256(deb), revision };
}

function installArgs(bundle) {
  return [
    "install",
    "--artifact",
    bundle.deb,
    "--artifact-sha256",
    bundle.sha256,
    "--evidence",
    bundle.evidencePath,
    "--source-revision",
    bundle.revision,
  ];
}

test("exact user deployment upgrades rolls back and preserves data by default", () => {
  const root = mkdtempSync(join(tmpdir(), "loading-bay-deploy-test-"));
  const home = join(root, "home");
  const env = {
    HOME: home,
    XDG_DATA_HOME: join(home, "data"),
    XDG_CACHE_HOME: join(home, "cache"),
    XDG_BIN_HOME: join(home, "bin"),
  };
  const first = fixture(root, "1".repeat(40), "first");
  const second = fixture(root, "2".repeat(40), "second");
  try {
    const unmanagedLauncher = join(env.XDG_BIN_HOME, "loading-bay");
    write(unmanagedLauncher, "#!/bin/sh\nexit 42\n", 0o755);
    assert.throws(
      () => run(installArgs(first), env),
      /refusing to replace unmanaged file/,
    );
    assert.equal(
      readFileSync(unmanagedLauncher, "utf8"),
      "#!/bin/sh\nexit 42\n",
    );
    rmSync(unmanagedLauncher);

    const installed = run(installArgs(first), env);
    assert.equal(installed.action, "install");
    assert.equal(installed.active.startsWith(first.revision), true);
    assert.match(
      readFileSync(installed.desktopEntryPath, "utf8"),
      /X-Rusty-Engine-Demo-Managed=true/,
    );
    assert.equal(
      readlinkSync(join(installed.installRoot, "current")),
      `releases/${installed.active}`,
    );
    const installedRelease = run(["status"], env).release;
    assert.notEqual(
      installedRelease.installedApplication.sha256,
      installedRelease.directBuildEvidence.application.sha256,
      "the exact Debian digest authenticates a bundler-transformed executable",
    );
    write(join(installed.saveRoot, "slot1.json"), "preserve-me");

    const upgraded = run(installArgs(second), env);
    assert.equal(upgraded.action, "upgrade");
    assert.equal(upgraded.active.startsWith(second.revision), true);
    assert.equal(upgraded.previous.startsWith(first.revision), true);

    const rolledBack = run(["rollback"], env);
    assert.equal(rolledBack.active.startsWith(first.revision), true);
    assert.equal(rolledBack.previous.startsWith(second.revision), true);
    assert.equal(run(["status"], env).active, rolledBack.active);

    assert.throws(
      () =>
        run(
          installArgs(second).map((value) =>
            value === second.sha256 ? "f".repeat(64) : value,
          ),
          env,
        ),
      /artifact hash mismatch/,
    );
    assert.equal(run(["status"], env).active, rolledBack.active);

    const uninstalled = run(["uninstall"], env);
    assert.equal(uninstalled.dataPreserved, true);
    assert.equal(
      readFileSync(join(installed.saveRoot, "slot1.json"), "utf8"),
      "preserve-me",
    );
    assert.equal(existsSync(installed.installRoot), false);

    run(installArgs(second), env);
    run(["uninstall", "--purge-data"], env);
    assert.equal(existsSync(installed.appDataRoot), false);
    assert.equal(existsSync(installed.cacheRoot), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
