import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  activeGuidancePaths,
  auditActiveGuidance,
} from "./audit-active-guidance.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const auditPath = resolve(repoRoot, "scripts/audit-boundary.mjs");
const ignoredDirectories = new Set([".git", "bin", "dist", "node_modules", "obj", "target"]);
const operationalRoots = [
  "package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "nx.json",
  "boundaries.json",
  "eslint.config.mjs",
  "tsconfig.angular.json",
  ".github",
  "apps",
  "libs",
  "scripts",
  "csharp",
  "ts",
];
const files = operationalRoots.flatMap((entry) =>
  collect(resolve(repoRoot, entry)),
);
const forbidden = [
  ["private render-contracts package", "@rusty-engine-demo/render-contracts"],
  ["private renderer-three package", "@rusty-engine-demo/renderer-three"],
  ["checked NativeAOT composition", "LoadingBay.NativeProduct"],
  ["downstream Engine root", "EngineRoot"],
];

const violations = [];
for (const relativePath of activeGuidancePaths) {
  const path = resolve(repoRoot, relativePath);
  if (!existsSync(path)) {
    violations.push(`${relativePath}: active guidance file is missing`);
    continue;
  }
  const content = readFileSync(path, "utf8");
  for (const finding of auditActiveGuidance(relativePath, content)) {
    violations.push(
      `${relativePath}: ${finding.label} (${finding.excerpt.replaceAll("\n", " ")})`,
    );
  }
}
for (const file of files) {
  if (file === auditPath) continue;
  const content = readFileSync(file, "utf8");
  for (const [label, marker] of forbidden) {
    if (content.includes(marker)) {
      violations.push(
        `${file.slice(repoRoot.length + 1)}: ${label} (${marker})`,
      );
    }
  }
}

const downstreamTypeScript = files.filter((file) => {
  const relative = file.slice(repoRoot.length + 1);
  return (
    (relative.startsWith("apps/") ||
      relative.startsWith("libs/") ||
      relative.startsWith("ts/")) &&
    /\.(?:[cm]?js|ts)$/.test(relative)
  );
});
for (const file of downstreamTypeScript) {
  const relative = file.slice(repoRoot.length + 1);
  const content = readFileSync(file, "utf8");
  for (const [label, marker] of [
    ["browser-owned inventory authority", "class InventoryService"],
    ["browser-owned pickup authority", "class PickupService"],
    ["browser-owned combat authority", "class CombatService"],
    ["browser-owned damage authority", "class DamageService"],
    ["browser-owned enemy authority", "class EnemyCombatService"],
    ["legacy input HTTP mutator", "/api/input-intent"],
    ["legacy edge HTTP mutator", "/api/input-edge"],
    ["legacy reset HTTP mutator", "/api/reset"],
    ["legacy phase HTTP mutator", "/api/motion-phase"],
    ["legacy phase HTTP mutator", "/api/navigation-phase"],
    ["legacy beacon HTTP mutator", "/api/extraction-beacon/activate"],
  ]) {
    if (content.includes(marker)) {
      violations.push(`${relative}: ${label} (${marker})`);
    }
  }
  for (const [label, marker] of [
    ["parallel animation-frame scheduler", "requestAnimationFrame("],
    ["parallel animation-frame scheduler", "cancelAnimationFrame("],
    ["private Three renderer construction", "new WebGLRenderer("],
    ["private Three renderer construction", "THREE.WebGLRenderer"],
    ["direct Engine renderer import", "@rusty-engine/render-contracts"],
    ["direct Engine renderer import", "@rusty-engine/render-projection"],
    ["direct Engine renderer import", "@rusty-engine/renderer-host"],
    ["direct Engine renderer import", "@rusty-engine/renderer-three"],
    ["direct Three import", 'from "three'],
    ["direct Three import", "from 'three"],
  ]) {
    if (content.includes(marker)) {
      violations.push(`${relative}: ${label} (${marker})`);
    }
  }
}

const expectedPackages = new Map([
  ["package.json", "rusty-engine-demo"],
  [
    "ts/packages/project-content/package.json",
    "@rusty-engine-demo/project-content",
  ],
  [
    "ts/packages/doom-e1m1-authoring/package.json",
    "@rusty-engine-demo/doom-e1m1-authoring",
  ],
]);
for (const [relativePath, expectedName] of expectedPackages) {
  const packageJson = JSON.parse(
    readFileSync(resolve(repoRoot, relativePath), "utf8"),
  );
  if (packageJson.name !== expectedName) {
    violations.push(`${relativePath}: expected package name ${expectedName}`);
  }
}

for (const relativePath of [
  "ts/packages/render-contracts",
  "ts/packages/renderer-three",
]) {
  if (existsSync(resolve(repoRoot, relativePath))) {
    violations.push(
      `${relativePath}: demo-private renderer package must remain absent`,
    );
  }
}

for (const relativePath of [
  "csharp/LoadingBay.NativeProduct",
  "apps/loading-bay/src/renderer-preload.ts",
  "apps/loading-bay/src/startup-error.html",
  "apps/loading-bay/src/styles.css",
  "libs/theme",
  ".gitattributes",
]) {
  if (existsSync(resolve(repoRoot, relativePath))) {
    violations.push(`${relativePath}: superseded host or shell residue must remain absent`);
  }
}

for (const relativePath of [
  "engine-source.json",
  "engine-development.json",
  "docs/engine-revision-updates.md",
  "scripts/engine-revision.mjs",
  "scripts/verify-engine-freshness.mjs",
]) {
  if (existsSync(resolve(repoRoot, relativePath))) {
    violations.push(
      `${relativePath}: adjacent Engine development must not reintroduce revision or freshness machinery`,
    );
  }
}

const rootPackage = JSON.parse(
  readFileSync(resolve(repoRoot, "package.json"), "utf8"),
);
for (const scriptName of Object.keys(rootPackage.scripts ?? {})) {
  if (/engine:(?:revision|freshness|pin|update)/u.test(scriptName)) {
    violations.push(
      `package.json: obsolete Engine lifecycle script must remain absent (${scriptName})`,
    );
  }
}
for (const section of ["dependencies", "devDependencies"]) {
  for (const dependencyName of Object.keys(rootPackage[section] ?? {})) {
    if (dependencyName.startsWith("@rusty-engine/")) {
      violations.push(
        `package.json: Engine browser artifacts belong to the matched runtime pack, not ${dependencyName}`,
      );
    }
  }
}

const gameProjectPath = resolve(repoRoot, "csharp/LoadingBay.Game/LoadingBay.Game.csproj");
const gameProject = readFileSync(gameProjectPath, "utf8");
for (const [label, marker] of [
  ["packaged Rusty.Engine SDK", '<PackageReference Include="Rusty.Engine" Version="0.1.0-dev.cbf35130d06c"'],
  ["explicit product entry type", "<RustyEngineProductEntryType>LoadingBay.Game.LoadingBayProduct</RustyEngineProductEntryType>"],
  ["Angular staged UI root", "<RustyEngineProductUiRoot>$(MSBuildThisFileDirectory)../../dist/apps/loading-bay/browser</RustyEngineProductUiRoot>"],
  ["E1M1 content root", "<RustyEngineProductContentRoot>$(MSBuildThisFileDirectory)../../content</RustyEngineProductContentRoot>"],
  ["explicit product watch roots", "<RustyEngineWatchPaths>$(MSBuildProjectDirectory);$(RustyEngineProductUiRoot);$(RustyEngineProductContentRoot)</RustyEngineWatchPaths>"],
  ["stable Angular staged entry", "<RustyEngineProductUiEntry>main.js</RustyEngineProductUiEntry>"],
  ["realtime lifecycle", "<RustyEngineProductLifecycleMode>realtime</RustyEngineProductLifecycleMode>"],
  ["HUD projection declaration", "<RustyEngineProductUiProjectionStream>loading-bay.hud</RustyEngineProductUiProjectionStream>"],
]) {
  if (!gameProject.includes(marker)) {
    violations.push(`csharp/LoadingBay.Game/LoadingBay.Game.csproj: missing ${label}`);
  }
}
if (gameProject.includes("ProjectReference") || gameProject.includes("EngineRoot")) {
  violations.push("csharp/LoadingBay.Game/LoadingBay.Game.csproj: ordinary products must not bind an Engine source project");
}

const productRunner = readFileSync(resolve(repoRoot, "scripts/run-csharp-product.sh"), "utf8");
for (const marker of ["cargo run", "RUSTY_ENGINE_ROOT", "LoadingBay.NativeProduct", "--manifest-path"]) {
  if (productRunner.includes(marker)) {
    violations.push(`scripts/run-csharp-product.sh: obsolete host launch marker ${marker}`);
  }
}
for (const marker of ["rusty\" dev", "--project \"$game_project\"", "--runtime \"$runtime_pack\""]) {
  if (!productRunner.includes(marker)) {
    violations.push(`scripts/run-csharp-product.sh: missing packaged development marker ${marker}`);
  }
}

const shellProject = JSON.parse(readFileSync(resolve(repoRoot, "apps/loading-bay/project.json"), "utf8"));
if (shellProject.targets?.build?.configurations?.production?.outputHashing !== "none") {
  violations.push("apps/loading-bay/project.json: staged runtime UI must keep the declared main.js entry stable");
}
const shellEntry = readFileSync(resolve(repoRoot, "apps/loading-bay/src/main.ts"), "utf8");
if (!shellEntry.includes("export async function mountProductUi")) {
  violations.push("apps/loading-bay/src/main.ts: runtime shell entry must export mountProductUi");
}
for (const marker of ["mountProductBrowserHost", "createProductBrowser", "loadRendererPreloadInitialContent"]) {
  if (shellEntry.includes(marker)) {
    violations.push(`apps/loading-bay/src/main.ts: Engine runtime shell owns ${marker}`);
  }
}

if (violations.length > 0) {
  throw new Error(
    `downstream boundary audit failed:\n${violations.join("\n")}`,
  );
}

console.log(
  `downstream boundary audit passed: ${String(files.length)} operational files, packaged C# SDK, runtime-pack-owned host and renderer, no private downstream renderer internals`,
);

function collect(path) {
  if (!existsSync(path)) return [];
  const stat = statSync(path);
  if (!stat.isDirectory()) return [path];
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) return [];
    return collect(resolve(path, entry.name));
  });
}
