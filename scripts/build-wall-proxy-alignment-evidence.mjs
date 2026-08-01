import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";

import { collectWallProxyAlignment } from "./wall-proxy-alignment.mjs";

const root = resolve(import.meta.dirname, "..");
const evidence = await collectWallProxyAlignment();
await writeFile(
  resolve(root, "docs/evidence/wall-proxy-alignment.json"),
  `${JSON.stringify(evidence, null, 2)}\n`,
);
console.log(
  `wall proxy evidence built: measurements=${String(evidence.measurements.length)} maxGap=${String(evidence.threshold.measuredMaximumGap)}`,
);
