import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import type { StoredProjectContent } from "./schema.js";

export type ContentGenerationMode = "check" | "write";

export const CANONICAL_PROJECT_FILES = [
  "loading-bay.project.json",
  "relay-annex.project.json",
] as const;

export function synchronizeGeneratedProjects(
  outputDirectory: string,
  projects: Readonly<Record<string, unknown>>,
  mode: ContentGenerationMode,
): void {
  if (mode === "write") {
    mkdirSync(outputDirectory, { recursive: true });
  }
  for (const [filename, project] of Object.entries(projects)) {
    const expected = `${JSON.stringify(project, null, 2)}\n`;
    const output = resolve(outputDirectory, filename);
    if (mode === "write") {
      writeFileSync(output, expected, "utf8");
      continue;
    }
    const actual = readFileSync(output, "utf8");
    if (actual !== expected) {
      throw new Error(`${filename} is stale; run pnpm run generate:content`);
    }
  }
}

export function readCanonicalProject(
  projectDirectory: string,
  filename: (typeof CANONICAL_PROJECT_FILES)[number],
): StoredProjectContent {
  const path = resolve(projectDirectory, filename);
  const source = readFileSync(path, "utf8");
  const decoded = JSON.parse(source) as unknown;
  if (
    typeof decoded !== "object" ||
    decoded === null ||
    Array.isArray(decoded)
  ) {
    throw new Error(`${filename} must contain one project object`);
  }
  return decoded as StoredProjectContent;
}
