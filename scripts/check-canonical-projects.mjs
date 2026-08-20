import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const projectFiles = ["content/projects/doom-e1m1.project.json"];
const proofRoot = mkdtempSync(join(tmpdir(), "loading-bay-project-check-"));

try {
  for (const [index, relativePath] of projectFiles.entries()) {
    const input = resolve(repoRoot, relativePath);
    const output = resolve(proofRoot, `${index}.project.json`);
    const source = readFileSync(input, "utf8");
    JSON.parse(source);

    const result = spawnSync(
      "cargo",
      [
        "run",
        "--quiet",
        "--locked",
        "-p",
        "loading-bay-game",
        "--bin",
        "project-store",
        "--",
        "--input",
        input,
        "--output",
        output,
      ],
      {
        cwd: repoRoot,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      },
    );
    if (result.status !== 0) {
      throw new Error(
        `${relativePath} failed canonical Rust admission/round-trip\n${result.stderr}${result.stdout}`,
      );
    }
    const roundTripped = readFileSync(output, "utf8");
    if (roundTripped !== source) {
      throw new Error(
        `${relativePath} differs after canonical Rust admission/round-trip`,
      );
    }
    const hash = createHash("sha256").update(source).digest("hex");
    console.log(
      `canonical project passed: ${relativePath} sha256=${hash} bytes=${Buffer.byteLength(source)}`,
    );
  }
} finally {
  rmSync(proofRoot, { recursive: true, force: true });
}
