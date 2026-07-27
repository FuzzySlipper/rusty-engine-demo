import assert from "node:assert/strict";
import test from "node:test";
import { analyzeBuildStats } from "./profile-browser-build.mjs";

test("attributes only static entry dependencies to initial JavaScript", () => {
  const report = analyzeBuildStats({
    outputs: {
      "main.js": {
        bytes: 10,
        entryPoint: "apps/loading-bay/src/main.ts",
        imports: [
          { kind: "import-statement", path: "angular.js" },
          { kind: "dynamic-import", path: "game.js" },
        ],
        inputs: {
          "apps/loading-bay/src/main.ts": { bytesInOutput: 8 },
        },
      },
      "angular.js": {
        bytes: 20,
        imports: [],
        inputs: {
          "node_modules/.pnpm/@angular+core/node_modules/@angular/core/core.mjs":
            { bytesInOutput: 18 },
        },
      },
      "game.js": {
        bytes: 30,
        entryPoint: "apps/loading-bay/src/game-screen.component.ts",
        imports: [{ kind: "import-statement", path: "renderer.js" }],
        inputs: {
          "ts/packages/browser-shell/src/game-runtime.ts": {
            bytesInOutput: 25,
          },
        },
      },
      "renderer.js": {
        bytes: 40,
        imports: [],
        inputs: {
          "node_modules/.pnpm/three@1/node_modules/three/build/three.module.js":
            { bytesInOutput: 35 },
        },
      },
    },
  });

  assert.equal(report.initial.rawBytes, 30);
  assert.deepEqual(report.initial.outputs, ["angular.js", "main.js"]);
  assert.deepEqual(report.initial.attribution.categories, [
    { category: "angular", bytes: 18 },
    { category: "loading-bay", bytes: 8 },
  ]);
  assert.equal(report.lazyRoutes.length, 1);
  assert.equal(report.lazyRoutes[0].rawBytes, 70);
  assert.deepEqual(report.lazyRoutes[0].attribution.categories, [
    { category: "three", bytes: 35 },
    { category: "browser-game-runtime", bytes: 25 },
  ]);
  assert.equal(report.allJavaScriptRawBytes, 100);
});

test("rejects malformed or incomplete Angular build stats", () => {
  assert.throws(
    () => analyzeBuildStats({ outputs: {} }),
    /do not identify the Loading Bay entry point/,
  );
  assert.throws(
    () =>
      analyzeBuildStats({
        outputs: {
          "main.js": {
            bytes: 10,
            entryPoint: "apps/loading-bay/src/main.ts",
            imports: [{ kind: "import-statement", path: "missing.js" }],
          },
        },
      }),
    /missing output missing\.js/,
  );
});
