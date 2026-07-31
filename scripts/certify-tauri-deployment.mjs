import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import process from "node:process";

const repoRoot = resolve(dirname(new URL(import.meta.url).pathname), "..");
const outputPath = resolve(
  repoRoot,
  process.env.TAURI_DEPLOYMENT_EVIDENCE ??
    "target/tauri-deployment-evidence.json",
);
const smokeEvidencePath = resolve(
  repoRoot,
  process.env.TAURI_SMOKE_EVIDENCE ??
    "target/tauri-installed-smoke-evidence.json",
);
const screenshotPath = resolve(
  repoRoot,
  process.env.TAURI_SMOKE_SCREENSHOT ?? "target/tauri-installed-smoke.png",
);
const narrowScreenshotPath = resolve(
  repoRoot,
  process.env.TAURI_SMOKE_NARROW_SCREENSHOT ??
    "target/tauri-installed-smoke-narrow.png",
);
const skipCampaign = process.argv.includes("--skip-campaign");

const status = JSON.parse(
  await runCapture(process.execPath, ["scripts/deploy-tauri.mjs", "status"]),
);
const desktopEntry = JSON.parse(
  await runCapture(process.execPath, ["scripts/probe-tauri-desktop-entry.mjs"]),
);
const release = status.release;
if (
  release?.sourceRevision === undefined ||
  release?.engineRevision === undefined
) {
  throw new Error(
    "installed deployment status has no exact source/Engine identity",
  );
}
const currentRoot = resolve(status.installRoot, "current");
const application = resolve(currentRoot, "usr/bin/loading-bay-desktop");
const sidecar = resolve(currentRoot, "usr/bin/loading-bay-browser-host");

let campaignOutput = "";
if (!skipCampaign) {
  await run(
    process.execPath,
    ["scripts/browser-smoke.mjs"],
    {
      ...process.env,
      RUSTY_ENGINE_DEMO_HOST_BINARY: sidecar,
      RUSTY_BROWSER_SMOKE_EXPORT_SAVE_ROOT: status.saveRoot,
    },
    (chunk) => {
      campaignOutput += chunk;
      process.stdout.write(chunk);
    },
  );
}
await run(process.execPath, ["scripts/tauri-smoke.mjs"], {
  ...process.env,
  TAURI_APPLICATION: application,
  TAURI_SMOKE_EVIDENCE: smokeEvidencePath,
  TAURI_SMOKE_SCREENSHOT: screenshotPath,
  TAURI_SMOKE_NARROW_SCREENSHOT: narrowScreenshotPath,
  ...(skipCampaign
    ? {}
    : {
        TAURI_SMOKE_REQUIRE_CONTINUE: "true",
        TAURI_SMOKE_SEED_SAVE_ROOT: status.saveRoot,
      }),
});

const smoke = JSON.parse(readFileSync(smokeEvidencePath, "utf8"));
const evidence = {
  schemaVersion: 1,
  certifiedAt: new Date().toISOString(),
  sourceRevision: release.sourceRevision,
  engineRevision: release.engineRevision,
  activeRelease: status.active,
  artifact: release.artifact,
  installedApplication: release.installedApplication,
  installedSidecar: release.installedSidecar,
  directBuildEvidence: release.directBuildEvidence,
  installedBytes: directoryBytes(currentRoot),
  paths: {
    installRoot: status.installRoot,
    launcher: status.launcherPath,
    desktopEntry: status.desktopEntryPath,
    appDataRoot: status.appDataRoot,
    saveRoot: status.saveRoot,
    cacheRoot: status.cacheRoot,
    logRoot: status.logRoot,
  },
  native: smoke,
  desktopEntry,
  fullCampaign: skipCampaign
    ? { status: "skipped", reason: "--skip-campaign" }
    : {
        status: "passed",
        hostBinary: sidecar,
        authority:
          "unchanged browser-smoke campaign served by the exact installed sidecar and package resources",
        gameSession: proofFromOutput(campaignOutput, "game-session proof"),
        renderer: proofFromOutput(
          campaignOutput,
          "shared-renderer correctness proof",
        ),
      },
};
mkdirSync(dirname(outputPath), { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(evidence, null, 2)}\n`);
process.stdout.write(`${JSON.stringify(evidence, null, 2)}\n`);

function directoryBytes(path) {
  const result = runSync("du", ["-sbL", path]).trim().split(/\s+/)[0];
  const bytes = Number(result);
  if (!Number.isSafeInteger(bytes) || bytes <= 0) {
    throw new Error(`could not measure installed bytes for ${path}`);
  }
  return bytes;
}

function proofFromOutput(output, prefix) {
  const line = output
    .split("\n")
    .find((candidate) => candidate.startsWith(`${prefix} `));
  if (line === undefined) throw new Error(`campaign did not emit ${prefix}`);
  const start = line.indexOf("{");
  let depth = 0;
  for (let index = start; index < line.length; index += 1) {
    if (line[index] === "{") depth += 1;
    if (line[index] === "}") depth -= 1;
    if (depth === 0) return JSON.parse(line.slice(start, index + 1));
  }
  throw new Error(`campaign emitted malformed ${prefix}`);
}

function runSync(command, args) {
  return execFileSync(command, args, { cwd: repoRoot, encoding: "utf8" });
}

async function runCapture(command, args) {
  let stdout = "";
  await run(command, args, process.env, (chunk) => {
    stdout += chunk;
  });
  return stdout;
}

function run(command, args, env, onStdout) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      cwd: repoRoot,
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    child.stdout.on("data", (chunk) => {
      const text = String(chunk);
      if (onStdout === undefined) process.stdout.write(text);
      else onStdout(text);
    });
    child.stderr.on("data", (chunk) => process.stderr.write(chunk));
    child.once("error", rejectRun);
    child.once("exit", (code) => {
      if (code === 0) resolveRun();
      else rejectRun(new Error(`${command} exited with ${String(code)}`));
    });
  });
}
