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
for (const [label, packageJson] of [["package.json", rootPackage]]) {
  for (const section of ["dependencies", "devDependencies"]) {
    for (const dependencyName of Object.keys(packageJson[section] ?? {})) {
      // The browser may use only Engine's public host artifacts. Gameplay
      // declarations remain immutable E1M1 content and execution stays C#.
      const allowedEnginePackages =
        label === "package.json" &&
        section === "dependencies" &&
        [
          "@rusty-engine/application-host",
          "@rusty-engine/live-debug-panel-browser",
          "@rusty-engine/product-browser-host",
        ].includes(dependencyName);
      if (
        dependencyName.startsWith("@rusty-engine/") &&
        !allowedEnginePackages
      ) {
        violations.push(
          `${label}: downstream ${section} must contain only public Engine host artifacts, not ${dependencyName}`,
        );
      }
    }
  }
}
for (const [dependency, location] of [
  ["@rusty-engine/application-host", "application-host"],
  ["@rusty-engine/product-browser-host", "product-browser-host"],
]) {
  if (
    rootPackage.dependencies?.[dependency] !==
    `file:../rusty-engine/render/artifacts/${location}`
  ) {
  violations.push(
      `package.json: ${dependency} must use its adjacent bundled Engine artifact`,
  );
  }
}
if (
  rootPackage.dependencies?.["@rusty-engine/live-debug-panel-browser"] !==
  "file:../rusty-engine/studio/artifacts/live-debug-panel"
) {
  violations.push(
    "package.json: @rusty-engine/live-debug-panel-browser must use its adjacent bundled Engine artifact",
  );
}

if (violations.length > 0) {
  throw new Error(
    `downstream boundary audit failed:\n${violations.join("\n")}`,
  );
}

console.log(
  `downstream boundary audit passed: ${String(files.length)} operational files, C# product ownership, public Engine host artifacts, no private downstream renderer internals`,
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
