import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const E1M1_PROP_SOURCE_ROOT = "content/doom-e1m1/props";

export const E1M1_REQUIRED_PROP_ASSET_IDS = [
  "mesh/prop-kit/breach-scattergun",
  "mesh/prop-kit/energy-cell",
  "mesh/prop-kit/hazard-marker",
  "mesh/prop-kit/impact-vest",
  "mesh/prop-kit/level-exit",
  "mesh/prop-kit/med-patch",
  "mesh/prop-kit/scatter-shells",
  "mesh/prop-kit/security-door",
] as const;

const propsDirectory = fileURLToPath(
  new URL("../../../../content/doom-e1m1/props/", import.meta.url),
);
const repoRoot = resolve(propsDirectory, "../../..");

function sha256(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

/**
 * Read the E1M1-only static prop closure.  The closure retains the imported
 * asset records because their catalog and material hashes are canonical input,
 * while the colocated raw meshes remain the reimport/provenance source.
 */
export function readE1M1PropAssets(): any[] {
  const assets = JSON.parse(
    readFileSync(resolve(propsDirectory, "assets.json"), "utf8"),
  ) as any[];
  if (!Array.isArray(assets)) {
    throw new Error("E1M1 prop closure must contain an asset array");
  }

  const byId = new Map(assets.map((asset) => [asset?.id, asset]));
  for (const id of E1M1_REQUIRED_PROP_ASSET_IDS) {
    const asset = byId.get(id);
    const sourcePath = `${E1M1_PROP_SOURCE_ROOT}/${id.slice("mesh/prop-kit/".length)}.mesh.json`;
    if (asset?.import?.source?.path !== sourcePath) {
      throw new Error(`${id} must retain the E1M1-owned source path`);
    }
    const source = readFileSync(
      resolve(propsDirectory, `${id.slice("mesh/prop-kit/".length)}.mesh.json`),
    );
    if (asset.import.sourceHash !== sha256(source)) {
      throw new Error(`${id} source hash no longer matches its E1M1 source`);
    }
    if (asset.import.sourceByteCount !== source.byteLength) {
      throw new Error(
        `${id} source byte count no longer matches its E1M1 source`,
      );
    }
    for (const dependency of asset.catalog?.dependencies ?? []) {
      if (!byId.has(dependency.id)) {
        throw new Error(
          `${id} is missing material dependency ${dependency.id}`,
        );
      }
    }
  }

  const provenance = JSON.parse(
    readFileSync(resolve(propsDirectory, "source-manifest.json"), "utf8"),
  ) as {
    licenses?: { path?: string; sha256?: string }[];
    assets?: {
      assetId?: string;
      importSourcePath?: string;
      sourcePath?: string;
      sourceSha256?: string;
      sourceDependencies?: { path?: string; sha256?: string }[];
    }[];
  };
  for (const license of provenance.licenses ?? []) {
    if (!license.path || !license.sha256) {
      throw new Error("E1M1 prop license provenance is incomplete");
    }
    const bytes = readFileSync(resolve(repoRoot, license.path));
    if (sha256(bytes) !== license.sha256) {
      throw new Error(`${license.path} no longer matches its declared hash`);
    }
  }
  const provenanceById = new Map(
    provenance.assets?.map((asset) => [asset.assetId, asset]) ?? [],
  );
  for (const id of E1M1_REQUIRED_PROP_ASSET_IDS) {
    const expectedPath = `${E1M1_PROP_SOURCE_ROOT}/${id.slice("mesh/prop-kit/".length)}.mesh.json`;
    const entry = provenanceById.get(id);
    if (entry?.importSourcePath !== expectedPath) {
      throw new Error(`${id} is missing E1M1-owned provenance`);
    }
    if (!entry.sourcePath || !entry.sourceSha256) {
      throw new Error(`${id} source provenance is incomplete`);
    }
    const source = readFileSync(resolve(repoRoot, entry.sourcePath));
    if (sha256(source) !== entry.sourceSha256) {
      throw new Error(`${id} source provenance hash no longer matches`);
    }
    for (const dependency of entry.sourceDependencies ?? []) {
      if (!dependency.path || !dependency.sha256) {
        throw new Error(`${id} source dependency provenance is incomplete`);
      }
      const dependencyBytes = readFileSync(resolve(repoRoot, dependency.path));
      if (sha256(dependencyBytes) !== dependency.sha256) {
        throw new Error(`${id} source dependency hash no longer matches`);
      }
    }
  }

  return assets;
}
