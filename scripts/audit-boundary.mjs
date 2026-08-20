import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  activeGuidancePaths,
  auditActiveGuidance,
} from "./audit-active-guidance.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const auditPath = resolve(repoRoot, "scripts/audit-boundary.mjs");
const ignoredDirectories = new Set([".git", "dist", "node_modules", "target"]);
const operationalRoots = [
  "Cargo.toml",
  "Cargo.lock",
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
  "rust",
  "ts",
];
const files = operationalRoots.flatMap((entry) =>
  collect(resolve(repoRoot, entry)),
);
const forbidden = [
  ["historical Engine checkout", ["asha", "-engine"].join("")],
  ["historical demo checkout", ["asha", "-demo"].join("")],
  ["historical package scope", ["@asha", "/"].join("")],
  ["private render-contracts package", "@rusty-engine-demo/render-contracts"],
  ["private renderer-three package", "@rusty-engine-demo/renderer-three"],
  ["old Rust product package", ["game", "-host"].join("")],
  ["old Rust product crate", ["game", "_host"].join("")],
  ["donor placeholder actions", "PlaceholderActions"],
  ["donor fake store kernel", "provideTemplateStoreKernel"],
  ["donor fake content authority", "DEMO_CONFIG"],
  ["old runtime spine", "GameplayRuntimeHost"],
  ["old runtime fabric", "GameplayFabric"],
  ["old native runtime bridge", "NativeRuntimeBridge"],
  ["old runtime session", "RuntimeSession"],
  ["old reaction frame", "ReactionFrame"],
  ["old decision receipt", "DecisionReceipt"],
  ["old replay record", "ReplayRecord"],
  ["old proposal envelope", "ProposalEnvelope"],
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
    "ts/packages/browser-shell/package.json",
    "@rusty-engine-demo/browser-shell",
  ],
  [
    "ts/packages/project-content/package.json",
    "@rusty-engine-demo/project-content",
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

const browserPackage = JSON.parse(
  readFileSync(
    resolve(repoRoot, "ts/packages/browser-shell/package.json"),
    "utf8",
  ),
);
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
const browserGameRuntime = readFileSync(
  resolve(repoRoot, "ts/packages/browser-shell/src/game-runtime.ts"),
  "utf8",
);
if (browserGameRuntime.includes("surface.renderOnce(")) {
  violations.push(
    "ts/packages/browser-shell/src/game-runtime.ts: the auto-started shared surface must not receive a parallel explicit render path",
  );
}
const gameSessionSource = readFileSync(
  resolve(repoRoot, "ts/packages/browser-shell/src/game-session.ts"),
  "utf8",
);
for (const staticKey of ["voxelMeshes", "lights", "generatedEnvironment"]) {
  if (!gameSessionSource.includes(`| "${staticKey}"`)) {
    violations.push(
      `ts/packages/browser-shell/src/game-session.ts: ${staticKey} must remain an immutable static resource`,
    );
  }
}
if (
  !gameSessionSource.includes(
    "type RuntimeDynamicState = Omit<RuntimeBrowserState, StaticStateKey>;",
  )
) {
  violations.push(
    "ts/packages/browser-shell/src/game-session.ts: dynamic session state must structurally omit static resource owners",
  );
}
const rustBrowserSession = readFileSync(
  resolve(
    repoRoot,
    "rust/crates/loading-bay-game/src/bin/browser_host/session.rs",
  ),
  "utf8",
);
if (
  !rustBrowserSession.includes(
    "const MAX_OUTBOUND_BUFFER_BYTES: usize = 4 * 1024 * 1024;",
  )
) {
  violations.push(
    "rust/crates/loading-bay-game/src/bin/browser_host/session.rs: cold bootstrap transport must retain its explicit 4 MiB bound (doom-e1m1 1.99M envelope requires headroom beyond 2M)",
  );
}
for (const [label, packageJson] of [
  ["package.json", rootPackage],
  ["ts/packages/browser-shell/package.json", browserPackage],
]) {
  for (const section of ["dependencies", "devDependencies"]) {
    for (const dependencyName of Object.keys(packageJson[section] ?? {})) {
      // The Engine's public downstream surfaces only: the bundled web
      // application host, and the gameplay-rules authoring packages the
      // downstream-adoption guide's TS authoring step is built on (the
      // rusty-dagger worked example depends on exactly these). Renderer and
      // Private Engine internals stay banned in downstream TS.
      const allowedEnginePackages =
        label === "package.json" &&
        section === "dependencies" &&
        [
          "@rusty-engine/application-host",
          "@rusty-engine/gameplay-rules-authoring",
          "@rusty-engine/gameplay-rules-contracts",
        ].includes(dependencyName);
      if (
        dependencyName.startsWith("@rusty-engine/") &&
        !allowedEnginePackages
      ) {
        violations.push(
          `${label}: downstream ${section} must contain only the public Engine application host and gameplay-rules authoring packages, not ${dependencyName}`,
        );
      }
    }
  }
}
if (
  rootPackage.dependencies?.["@rusty-engine/application-host"] !==
  "file:../rusty-engine/render/artifacts/application-host"
) {
  violations.push(
    "package.json: the sole Engine web dependency must use the adjacent bundled application-host artifact",
  );
}
const cargoManifest = readFileSync(resolve(repoRoot, "Cargo.toml"), "utf8");
const expectedEngineDependency =
  'rusty-engine = { path = "../rusty-engine/rust/crates/rusty-engine" }';
if (!cargoManifest.split("\n").includes(expectedEngineDependency)) {
  violations.push(
    "Cargo.toml: ordinary Rust must use one adjacent rusty-engine facade dependency",
  );
}
const engineDependencies = [
  ...cargoManifest.matchAll(
    /^([a-z0-9-]+)\s*=\s*\{[^\n]*(?:git\s*=\s*"[^"]*rusty-engine|path\s*=\s*"[^"]*rusty-engine)[^\n]*\}$/gmu,
  ),
];
if (
  engineDependencies.length !== 1 ||
  engineDependencies[0][1] !== "rusty-engine" ||
  engineDependencies[0][0] !== expectedEngineDependency
) {
  violations.push(
    "Cargo.toml: ordinary Rust must use only the adjacent rusty-engine facade dependency",
  );
}

if (violations.length > 0) {
  throw new Error(
    `downstream boundary audit failed:\n${violations.join("\n")}`,
  );
}

console.log(
  `downstream boundary audit passed: ${String(files.length)} operational files, one adjacent Rust facade, one bundled Engine application host, no private downstream or renderer internals`,
);

function collect(path) {
  const stat = statSync(path);
  if (!stat.isDirectory()) return [path];
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) return [];
    return collect(resolve(path, entry.name));
  });
}
