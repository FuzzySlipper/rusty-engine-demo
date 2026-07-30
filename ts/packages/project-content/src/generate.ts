import { fileURLToPath } from "node:url";

import {
  CANONICAL_PROJECT_FILES,
  readCanonicalProject,
  synchronizeGeneratedProjects,
} from "./content-artifacts.js";
import { generatedEncounterProjects } from "./encounter-project.js";
import { generatedMotionProjects } from "./motion-project.js";

const outputDirectory = fileURLToPath(
  new URL("../../../../content/generated/", import.meta.url),
);
const projectDirectory = fileURLToPath(
  new URL("../../../../content/projects/", import.meta.url),
);
const mode = process.argv[2] ?? "--check";

if (mode !== "--check" && mode !== "--write") {
  throw new Error(`unsupported generation mode ${mode}`);
}

const generatedProjects = {
  ...generatedEncounterProjects,
  ...generatedMotionProjects,
};

synchronizeGeneratedProjects(
  outputDirectory,
  generatedProjects,
  mode === "--write" ? "write" : "check",
);

for (const filename of CANONICAL_PROJECT_FILES) {
  readCanonicalProject(projectDirectory, filename);
}
