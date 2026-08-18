import { mkdir, readdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// Materializes every package entry in gameplay/authoring/src/packages/
// (compiled to dist/packages/) into data/gameplay/<domain>-<package>.package.json.
// Output is deterministic: same sources, same bytes, drift-checked by
// `pnpm gameplay:check`. Build plumbing only — semantic validation is Rust's.

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const packagesDirectory = resolve(scriptDirectory, "../dist/packages");
const outputDirectory = resolve(scriptDirectory, "../../../data/gameplay");

const entries = (await readdir(packagesDirectory))
  .filter((entry) => entry.endsWith(".js"))
  .sort();

await mkdir(outputDirectory, { recursive: true });
for (const entry of entries) {
  const module = await import(pathToFileURL(resolve(packagesDirectory, entry)).href);
  const artifact = module.gameplayPackage;
  if (artifact?.canonicalJson === undefined) {
    throw new Error(`${entry} does not export a canonical gameplayPackage artifact`);
  }
  const name = `${artifact.package.domain}-${artifact.package.package}.package.json`;
  const output = resolve(outputDirectory, name);
  // canonicalJson is the exact byte string the Engine fingerprints.
  await writeFile(output, `${artifact.canonicalJson}\n`, "utf8");
  console.log(`materialized ${name} (${artifact.fingerprint})`);
}
