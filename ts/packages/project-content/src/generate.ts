import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { generatedEncounterProjects, loadingBayStoredProject } from "./encounter-project.js";
import { generatedMotionProjects } from "./motion-project.js";

const outputDirectory = fileURLToPath(new URL("../../../../content/generated/", import.meta.url));
const projectDirectory = fileURLToPath(new URL("../../../../content/projects/", import.meta.url));
const mode = process.argv[2] ?? "--check";

if (mode !== "--check" && mode !== "--write") {
  throw new Error(`unsupported generation mode ${mode}`);
}

if (mode === "--write") {
  mkdirSync(outputDirectory, { recursive: true });
}

const generatedProjects = { ...generatedEncounterProjects, ...generatedMotionProjects };
const authoredProjects = { "loading-bay.project.json": loadingBayStoredProject() };

for (const [directory, projects] of [
  [outputDirectory, generatedProjects],
  [projectDirectory, authoredProjects],
] as const) {
  if (mode === "--write") {
    mkdirSync(directory, { recursive: true });
  }
  for (const [filename, project] of Object.entries(projects)) {
  const expected = `${JSON.stringify(project, null, 2)}\n`;
  const output = `${directory}${filename}`;
  if (mode === "--write") {
    writeFileSync(output, expected, "utf8");
    continue;
  }
  const actual = readFileSync(output, "utf8");
  if (actual !== expected) {
    throw new Error(`${filename} is stale; run pnpm run generate:content`);
  }
  }
}
