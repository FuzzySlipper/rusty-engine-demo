import { readdirSync, readFileSync, statSync } from "node:fs";
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

const files = operationalRoots.flatMap((entry) => collect(resolve(repoRoot, entry)));
const forbidden = [
  ["historical Engine checkout", ["asha", "-engine"].join("")],
  ["historical demo checkout", ["asha", "-demo"].join("")],
  ["historical package scope", ["@asha", "/"].join("")],
  ["old product package scope", ["@rusty-engine", "/"].join("")],
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
  ["ts/packages/render-contracts/package.json", "@rusty-engine-demo/render-contracts"],
  ["ts/packages/renderer-three/package.json", "@rusty-engine-demo/renderer-three"],
]);
for (const [relativePath, expectedName] of expectedPackages) {
  const packageJson = JSON.parse(readFileSync(resolve(repoRoot, relativePath), "utf8"));
  if (packageJson.name !== expectedName) {
    violations.push(`${relativePath}: expected package name ${expectedName}`);
  }
}

if (violations.length > 0) {
  throw new Error(`downstream boundary audit failed:\n${violations.join("\n")}`);
}

console.log(
  `downstream boundary audit passed: ${String(files.length)} operational files, five demo-owned package identities, no sibling or historical runtime references`,
);

function collect(path) {
  const stat = statSync(path);
  if (!stat.isDirectory()) return [path];
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) return [];
    return collect(resolve(path, entry.name));
  });
}
