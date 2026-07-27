import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const defaultStatsPath = "dist/apps/loading-bay/stats.json";

export function analyzeBuildStats(stats) {
  const outputs = stats?.outputs;
  if (typeof outputs !== "object" || outputs === null) {
    throw new Error("Angular build stats must contain an outputs object");
  }

  const mainOutput = Object.entries(outputs).find(
    ([path, output]) =>
      path.endsWith(".js") &&
      output?.entryPoint === "apps/loading-bay/src/main.ts",
  );
  if (mainOutput === undefined) {
    throw new Error(
      "Angular build stats do not identify the Loading Bay entry point",
    );
  }

  const initialOutputs = staticOutputClosure(outputs, [mainOutput[0]]);
  const routeEntries = (mainOutput[1].imports ?? [])
    .filter(({ kind }) => kind === "dynamic-import")
    .map(({ path }) => {
      const output = outputs[path];
      if (output === undefined) {
        throw new Error(`Angular build stats reference missing output ${path}`);
      }
      const closure = staticOutputClosure(outputs, [path]);
      const incrementalOutputs = new Set(
        [...closure].filter((outputPath) => !initialOutputs.has(outputPath)),
      );
      return {
        entryPoint: output.entryPoint ?? path,
        output: path,
        rawBytes: sumOutputBytes(outputs, incrementalOutputs),
        attribution: attributeInputs(outputs, incrementalOutputs),
      };
    })
    .sort((left, right) => right.rawBytes - left.rawBytes);

  const allJavaScriptOutputs = new Set(
    Object.keys(outputs).filter((path) => path.endsWith(".js")),
  );
  return {
    schemaVersion: 1,
    initial: {
      outputs: [...initialOutputs].sort(),
      rawBytes: sumOutputBytes(outputs, initialOutputs),
      attribution: attributeInputs(outputs, initialOutputs),
    },
    lazyRoutes: routeEntries,
    allJavaScriptRawBytes: sumOutputBytes(outputs, allJavaScriptOutputs),
  };
}

function staticOutputClosure(outputs, seeds) {
  const closure = new Set();
  const pending = [...seeds];
  while (pending.length > 0) {
    const path = pending.pop();
    if (closure.has(path)) {
      continue;
    }
    const output = outputs[path];
    if (output === undefined) {
      throw new Error(`Angular build stats reference missing output ${path}`);
    }
    closure.add(path);
    for (const imported of output.imports ?? []) {
      if (imported.kind !== "dynamic-import") {
        pending.push(imported.path);
      }
    }
  }
  return closure;
}

function sumOutputBytes(outputs, paths) {
  return [...paths].reduce((total, path) => total + outputs[path].bytes, 0);
}

function attributeInputs(outputs, paths) {
  const categories = new Map();
  const modules = [];
  for (const outputPath of paths) {
    for (const [input, detail] of Object.entries(
      outputs[outputPath].inputs ?? {},
    )) {
      const bytes = detail.bytesInOutput ?? 0;
      if (bytes <= 0) {
        continue;
      }
      const category = inputCategory(input);
      categories.set(category, (categories.get(category) ?? 0) + bytes);
      modules.push({ bytes, input, output: outputPath });
    }
  }
  return {
    categories: [...categories]
      .map(([category, bytes]) => ({ category, bytes }))
      .sort((left, right) => right.bytes - left.bytes),
    largestModules: modules
      .sort((left, right) => right.bytes - left.bytes)
      .slice(0, 12),
  };
}

function inputCategory(input) {
  if (input.includes("/three@") || input.includes("/node_modules/three/")) {
    return "three";
  }
  if (
    input.includes("@rusty-engine+renderer") ||
    input.includes("@rusty-engine+render-")
  ) {
    return "rusty-engine-renderer";
  }
  if (input.includes("@angular+")) {
    return "angular";
  }
  if (input.startsWith("ts/packages/browser-shell/")) {
    return "browser-game-runtime";
  }
  if (input.startsWith("apps/loading-bay/")) {
    return "loading-bay";
  }
  if (input.startsWith("libs/")) {
    return "demo-libraries";
  }
  if (input.includes("node_modules/")) {
    return "third-party";
  }
  return "other";
}

function currentRevision() {
  return execFileSync("git", ["rev-parse", "HEAD"], {
    encoding: "utf8",
  }).trim();
}

if (
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  const statsPath = process.argv[2] ?? defaultStatsPath;
  const stats = JSON.parse(readFileSync(statsPath, "utf8"));
  console.log(
    JSON.stringify(
      {
        revision: currentRevision(),
        statsPath,
        ...analyzeBuildStats(stats),
      },
      null,
      2,
    ),
  );
}
