import assert from "node:assert/strict";
import process from "node:process";

import { ESLint } from "eslint";

const eslint = new ESLint({ cwd: process.cwd() });
const [result] = await eslint.lintText(
  [
    'import { CompassComponent } from "@rusty-engine-demo/ui-compass";',
    "void CompassComponent;",
  ].join("\n"),
  { filePath: "libs/platform/src/boundary-probe.ts" },
);

assert(result !== undefined);
const violations = result.messages.filter(
  (message) => message.ruleId === "@nx/enforce-module-boundaries",
);
assert.equal(
  violations.length,
  1,
  `expected one platform-to-component boundary violation, received ${JSON.stringify(result.messages)}`,
);
assert.match(
  violations[0]?.message ?? "",
  /scope:platform|only depend on libs/i,
);
console.log("boundary regression passed: forbidden platform -> component import rejected");
