import {
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";

const repoRoot = resolve(dirname(new URL(import.meta.url).pathname), "..");
const manifestPath = resolve(
  repoRoot,
  "src-tauri/resources/desktop-package-manifest.json",
);
const paths = {
  application: resolve(repoRoot, "target/release/loading-bay-desktop"),
  sidecar: resolve(repoRoot, "target/release/loading-bay-browser-host"),
  manifest: manifestPath,
};
const forbiddenBytes = [
  ["/home/dev", "rusty-engine", ""].join("/"),
  "node_modules/",
  "localhost:4200",
  ["..", "rusty-engine"].join("/"),
  "BEGIN PRIVATE KEY",
  "github_pat_",
  "ghp_",
];

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

for (const [label, path] of Object.entries(paths)) {
  if (!existsSync(path)) {
    throw new Error(`required ${label} package output is missing: ${path}`);
  }
}

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
const manifestPaths = new Set(manifest.files.map(({ path }) => path));
if (manifestPaths.size !== manifest.files.length) {
  throw new Error("desktop package manifest contains duplicate paths");
}
if (
  manifest.files.some(
    ({ path }) =>
      !/^(content\/|web\/|loading-bay-browser-host$)/.test(path) ||
      /(^|\/)(node_modules|target|\.git|\.nx|\.angular|dist)(\/|$)/.test(path),
  )
) {
  throw new Error(
    "desktop package manifest contains a development or cache path",
  );
}

for (const path of Object.values(paths)) {
  const source = readFileSync(path).toString("latin1");
  for (const forbidden of forbiddenBytes) {
    if (source.includes(forbidden)) {
      throw new Error(
        `${path} contains forbidden package byte sequence ${forbidden}`,
      );
    }
  }
}

const evidence = {
  schemaVersion: 1,
  sourceRevision: manifest.sourceRevision,
  engineRevision: JSON.parse(
    readFileSync(resolve(repoRoot, "engine-source.json"), "utf8"),
  ).commit,
  targetTriple: manifest.targetTriple,
  files: Object.fromEntries(
    Object.entries(paths).map(([label, path]) => [
      label,
      {
        path,
        byteLen: statSync(path).size,
        sha256: sha256(path),
      },
    ]),
  ),
  packagedResourceCount: manifest.files.filter(
    ({ kind }) => kind === "resource",
  ).length,
  packagedResourceBytes: manifest.files
    .filter(({ kind }) => kind === "resource")
    .reduce((sum, { byteLen }) => sum + byteLen, 0),
};
const evidencePath = resolve(repoRoot, "target/tauri-package-evidence.json");
mkdirSync(dirname(evidencePath), { recursive: true });
writeFileSync(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`);
process.stdout.write(`${JSON.stringify(evidence, null, 2)}\n`);
