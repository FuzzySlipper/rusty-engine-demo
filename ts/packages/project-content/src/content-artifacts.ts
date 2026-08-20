import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import type { StoredProjectContent } from "./schema.js";

export const CANONICAL_PROJECT_FILES = ["doom-e1m1.project.json"] as const;

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
