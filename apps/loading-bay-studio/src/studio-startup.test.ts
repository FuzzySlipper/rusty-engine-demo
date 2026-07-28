import assert from "node:assert/strict";
import test from "node:test";

import { readStartupProject } from "./studio-startup.ts";

test("startup accepts one external root and one relative project", () => {
  assert.deepEqual(
    readStartupProject(
      "http://127.0.0.1/?root=%2Fproject&project=content%2Floading-bay.project.json",
    ),
    {
      root: "/project",
      projectFile: "content/loading-bay.project.json",
    },
  );
  assert.equal(readStartupProject("http://127.0.0.1/"), null);
});

test("startup rejects partial duplicate and malformed selectors", () => {
  assert.deepEqual(readStartupProject("http://127.0.0.1/?root=%2Fproject"), {
    diagnostic:
      "Startup requires exactly one external root and one project-relative file.",
  });
  assert.deepEqual(
    readStartupProject(
      "http://127.0.0.1/?root=%2Fa&root=%2Fb&project=content%2Fp.json",
    ),
    {
      diagnostic:
        "Startup requires exactly one external root and one project-relative file.",
    },
  );
  assert.deepEqual(
    readStartupProject(
      `http://127.0.0.1/?root=${encodeURIComponent(`/${"x".repeat(4096)}`)}&project=p.json`,
    ),
    {
      diagnostic: "Startup project selection is empty or malformed.",
    },
  );
});
