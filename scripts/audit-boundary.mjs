import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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
const rustEngineRevision = "a6857d03141e162511231c276ee751a3413c90e5";
const renderEngineRevision = "a6857d03141e162511231c276ee751a3413c90e5";

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
  ["absolute sibling checkout", ["/home/dev/", "rusty-engine"].join("")],
  ["relative sibling checkout", ["../", "rusty-engine"].join("")],
];

const violations = [];
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

const browserPackage = JSON.parse(
  readFileSync(
    resolve(repoRoot, "ts/packages/browser-shell/package.json"),
    "utf8",
  ),
);
const rootPackage = JSON.parse(
  readFileSync(resolve(repoRoot, "package.json"), "utf8"),
);
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
    "const MAX_OUTBOUND_BUFFER_BYTES: usize = 2 * 1024 * 1024;",
  )
) {
  violations.push(
    "rust/crates/loading-bay-game/src/bin/browser_host/session.rs: cold bootstrap transport must retain its explicit 2 MiB bound",
  );
}
for (const packageName of [
  "render-contracts",
  "render-projection",
  "renderer-host",
  "renderer-three",
]) {
  const dependencyName = `@rusty-engine/${packageName}`;
  const expected = `github:FuzzySlipper/rusty-engine#${renderEngineRevision}&path:render/packages/${packageName}`;
  if (browserPackage.dependencies?.[dependencyName] !== expected) {
    violations.push(
      `ts/packages/browser-shell/package.json: ${dependencyName} must resolve from exact Engine revision ${renderEngineRevision}`,
    );
  }
  if (rootPackage.dependencies?.[dependencyName] !== expected) {
    violations.push(
      `package.json: ${dependencyName} must resolve from exact Engine revision ${renderEngineRevision}`,
    );
  }
}
for (const [packageName, packagePath] of [
  ["studio-adapter-client", "studio/libs/adapter-client"],
  ["studio-editor-shell", "studio/libs/editor-shell"],
  ["studio-user-settings", "studio/libs/user-settings"],
  ["studio-viewport", "studio/libs/viewport"],
  ["studio-voxel-editor", "studio/libs/voxel-editor"],
]) {
  const dependencyName = `@rusty-engine/${packageName}`;
  const expected = `github:FuzzySlipper/rusty-engine#${renderEngineRevision}&path:${packagePath}`;
  if (rootPackage.dependencies?.[dependencyName] !== expected) {
    violations.push(
      `package.json: ${dependencyName} must resolve from exact Engine revision ${renderEngineRevision}`,
    );
  }
}

const cargoManifest = readFileSync(resolve(repoRoot, "Cargo.toml"), "utf8");
if (!cargoManifest.includes(`revision = "${rustEngineRevision}"`)) {
  violations.push(
    `Cargo.toml: Engine metadata revision must be ${rustEngineRevision}`,
  );
}
for (const match of cargoManifest.matchAll(
  /git = "https:\/\/github\.com\/FuzzySlipper\/rusty-engine\.git", rev = "([0-9a-f]+)"/g,
)) {
  if (match[1] !== rustEngineRevision) {
    violations.push(
      `Cargo.toml: Rust Engine dependency resolves ${match[1]} instead of ${rustEngineRevision}`,
    );
  }
}

if (violations.length > 0) {
  throw new Error(
    `downstream boundary audit failed:\n${violations.join("\n")}`,
  );
}

console.log(
  `downstream boundary audit passed: ${String(files.length)} operational files, three demo-owned package identities, exact shared Engine Rust/render dependencies, no private renderer or historical runtime references`,
);

function collect(path) {
  const stat = statSync(path);
  if (!stat.isDirectory()) return [path];
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) return [];
    return collect(resolve(path, entry.name));
  });
}
