import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

const build = spawn("pnpm", ["nx", "build", "loading-bay", "--stats-json"], {
  stdio: ["ignore", "ignore", "inherit"],
});
const buildExitCode = await processExit(build);
if (buildExitCode !== 0) {
  throw new Error(
    `desktop-representative production build exited with ${String(buildExitCode)}`,
  );
}

const port = await reservePort();
const address = `127.0.0.1:${String(port)}`;
const saveRoot = mkdtempSync(join(tmpdir(), "rusty-engine-profile-saves-"));
const host = spawn(
  "cargo",
  [
    "run",
    "--locked",
    "-p",
    "loading-bay-game",
    "--bin",
    "browser-host",
    "--",
    "--addr",
    address,
    "--project",
    "content/projects/loading-bay.project.json",
    "--save-root",
    saveRoot,
  ],
  { stdio: ["ignore", "ignore", "inherit"] },
);

try {
  await waitForHost(`http://${address}/health`, host);
  const profiler = spawn(
    process.execPath,
    ["scripts/profile-desktop-runtime.mjs"],
    {
      env: {
        ...process.env,
        RUSTY_ENGINE_DEMO_URL: `http://${address}/`,
      },
      stdio: "inherit",
    },
  );
  const exitCode = await processExit(profiler);
  if (exitCode !== 0) {
    throw new Error(
      `desktop-representative profiler exited with ${String(exitCode)}`,
    );
  }
} finally {
  await terminateProcess(host);
  rmSync(saveRoot, { recursive: true, force: true });
}

async function waitForHost(url, process) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (process.exitCode !== null) {
      throw new Error(
        `browser host exited before readiness with ${String(process.exitCode)}`,
      );
    }
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // Compilation and startup take a moment.
    }
    await delay(50);
  }
  throw new Error("browser host did not become ready within 30 seconds");
}

function reservePort() {
  return new Promise((resolvePort, rejectPort) => {
    const server = createServer();
    server.once("error", rejectPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (typeof address !== "object" || address === null) {
        server.close();
        rejectPort(new Error("could not reserve browser host port"));
        return;
      }
      server.close(() => resolvePort(address.port));
    });
  });
}

function processExit(process) {
  return new Promise((resolveExit) => {
    process.once("exit", (code) => resolveExit(code ?? 1));
  });
}

function terminateProcess(process) {
  return new Promise((resolveTermination) => {
    if (process.exitCode !== null) {
      resolveTermination();
      return;
    }
    process.once("exit", () => resolveTermination());
    process.kill("SIGTERM");
    setTimeout(() => {
      if (process.exitCode === null) {
        process.kill("SIGKILL");
      }
    }, 2_000).unref();
  });
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => {
    setTimeout(resolveDelay, milliseconds);
  });
}
