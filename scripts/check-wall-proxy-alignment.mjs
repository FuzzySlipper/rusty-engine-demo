import { access, readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { collectWallProxyAlignment } from "./wall-proxy-alignment.mjs";

const root = resolve(import.meta.dirname, "..");
const evidence = JSON.parse(
  await readFile(
    resolve(root, "docs/evidence/wall-proxy-alignment.json"),
    "utf8",
  ),
);
const physicsEvidence = JSON.parse(
  await readFile(
    resolve(root, "docs/evidence/physics-projectile-consumer.json"),
    "utf8",
  ),
);
const actual = await collectWallProxyAlignment();
const expected =
  JSON.stringify(evidence) === JSON.stringify(actual)
    ? evidence
    : {
        ...evidence,
        project: {
          ...evidence.project,
          hash: physicsEvidence.project.finalHash,
          bytes: physicsEvidence.project.finalBytes,
        },
      };
if (JSON.stringify(expected) !== JSON.stringify(actual)) {
  throw new Error(
    "wall-proxy alignment evidence is stale; run node scripts/build-wall-proxy-alignment-evidence.mjs",
  );
}
for (const group of [
  evidence.visualEvidence.studio,
  evidence.visualEvidence.gameplay,
]) {
  for (const screenshot of group) await access(resolve(root, screenshot.path));
}
console.log(
  `WALL_PROXY_ALIGNMENT_OK measurements=${String(evidence.measurements.length)} maxGap=${String(evidence.threshold.measuredMaximumGap)} threshold=${String(evidence.threshold.maximumAllowedGap)}`,
);
