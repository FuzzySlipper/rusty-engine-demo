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
  ".github",
  "scripts",
  "rust",
  "ts",
];
const engineRevision = "8cb49db6cfe9471faa23ab0661656a2366a83d8c";

const files = operationalRoots.flatMap((entry) => collect(resolve(repoRoot, entry)));
const forbidden = [
  ["historical Engine checkout", ["asha", "-engine"].join("")],
  ["historical demo checkout", ["asha", "-demo"].join("")],
  ["historical package scope", ["@asha", "/"].join("")],
  ["private render-contracts package", "@rusty-engine-demo/render-contracts"],
  ["private renderer-three package", "@rusty-engine-demo/renderer-three"],
  ["old Rust product package", ["game", "-host"].join("")],
  ["old Rust product crate", ["game", "_host"].join("")],
  ["absolute sibling checkout", ["/home/dev/", "rusty-engine"].join("")],
  ["relative sibling checkout", ["../", "rusty-engine"].join("")],
];

const violations = [];
for (const file of files) {
  if (file === auditPath) continue;
  const content = readFileSync(file, "utf8");
  for (const [label, marker] of forbidden) {
    if (content.includes(marker)) {
      violations.push(`${file.slice(repoRoot.length + 1)}: ${label} (${marker})`);
    }
  }
}

const expectedPackages = new Map([
  ["package.json", "rusty-engine-demo"],
  ["ts/packages/browser-shell/package.json", "@rusty-engine-demo/browser-shell"],
  ["ts/packages/project-content/package.json", "@rusty-engine-demo/project-content"],
]);
for (const [relativePath, expectedName] of expectedPackages) {
  const packageJson = JSON.parse(readFileSync(resolve(repoRoot, relativePath), "utf8"));
  if (packageJson.name !== expectedName) {
    violations.push(`${relativePath}: expected package name ${expectedName}`);
  }
}

for (const relativePath of ["ts/packages/render-contracts", "ts/packages/renderer-three"]) {
  if (existsSync(resolve(repoRoot, relativePath))) {
    violations.push(`${relativePath}: demo-private renderer package must remain absent`);
  }
}

const browserPackage = JSON.parse(
  readFileSync(resolve(repoRoot, "ts/packages/browser-shell/package.json"), "utf8"),
);
for (const packageName of [
  "render-contracts",
  "render-projection",
  "renderer-host",
  "renderer-three",
]) {
  const dependencyName = `@rusty-engine/${packageName}`;
  const expected = `github:FuzzySlipper/rusty-engine#${engineRevision}&path:render/packages/${packageName}`;
  if (browserPackage.dependencies?.[dependencyName] !== expected) {
    violations.push(
      `ts/packages/browser-shell/package.json: ${dependencyName} must resolve from exact Engine revision ${engineRevision}`,
    );
  }
}

const cargoManifest = readFileSync(resolve(repoRoot, "Cargo.toml"), "utf8");
if (!cargoManifest.includes(`revision = "${engineRevision}"`)) {
  violations.push(`Cargo.toml: Engine metadata revision must be ${engineRevision}`);
}
for (const match of cargoManifest.matchAll(/git = "https:\/\/github\.com\/FuzzySlipper\/rusty-engine\.git", rev = "([0-9a-f]+)"/g)) {
  if (match[1] !== engineRevision) {
    violations.push(`Cargo.toml: Rust Engine dependency resolves ${match[1]} instead of ${engineRevision}`);
  }
}

if (violations.length > 0) {
  throw new Error(`downstream boundary audit failed:\n${violations.join("\n")}`);
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
