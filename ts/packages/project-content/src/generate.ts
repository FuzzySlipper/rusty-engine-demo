import {
  CANONICAL_PROJECT_FILES,
  readCanonicalProject,
} from "./content-artifacts.js";
import { fileURLToPath } from "node:url";

const projectDirectory = fileURLToPath(
  new URL("../../../../content/projects/", import.meta.url),
);
const mode = process.argv[2] ?? "--check";

if (mode !== "--check" && mode !== "--write") {
  throw new Error(`unsupported generation mode ${mode}`);
}

for (const filename of CANONICAL_PROJECT_FILES) {
  readCanonicalProject(projectDirectory, filename);
}
