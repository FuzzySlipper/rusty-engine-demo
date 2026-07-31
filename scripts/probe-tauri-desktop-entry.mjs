import { spawn } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";

const home = process.env.HOME;
if (home === undefined) throw new Error("HOME is required");
const cacheRoot = process.env.XDG_CACHE_HOME ?? join(home, ".cache");
const environment = {
  HOME: home,
  USER: process.env.USER ?? "user",
  PATH: "/usr/bin:/bin",
  LANG: process.env.LANG ?? "C.UTF-8",
  XDG_DATA_HOME: process.env.XDG_DATA_HOME ?? join(home, ".local/share"),
  XDG_CACHE_HOME: cacheRoot,
  XDG_CONFIG_HOME: process.env.XDG_CONFIG_HOME ?? join(home, ".config"),
  XDG_STATE_HOME: process.env.XDG_STATE_HOME ?? join(home, ".local/state"),
  XDG_BIN_HOME: process.env.XDG_BIN_HOME ?? join(home, ".local/bin"),
};
const output = [];
const launcher = spawn(
  "dbus-run-session",
  ["--", "xvfb-run", "-a", "sh", "-c", "gtk-launch loading-bay; sleep 60"],
  {
    cwd: "/tmp",
    env: environment,
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  },
);
launcher.stdout.on("data", (chunk) => output.push(`stdout ${String(chunk)}`));
launcher.stderr.on("data", (chunk) => output.push(`stderr ${String(chunk)}`));

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

function processExists(pid) {
  try {
    if (readFileSync(`/proc/${pid}/stat`, "utf8").split(" ")[2] === "Z") {
      return false;
    }
  } catch (error) {
    if (error?.code === "ENOENT") return false;
  }
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error?.code === "ESRCH") return false;
    throw error;
  }
}

async function waitFor(predicate, label, timeout = 45_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = predicate();
    if (value) return value;
    await delay(100);
  }
  throw new Error(`timed out waiting for ${label}\n${output.join("")}`);
}

try {
  const readyDirectory = join(
    cacheRoot,
    "dev.fuzzyslipper.rusty-engine-demo.loading-bay",
  );
  const previousReady = new Set(
    existsSync(readyDirectory)
      ? readdirSync(readyDirectory).filter((name) =>
          /^host-ready-\d+\.json$/.test(name),
        )
      : [],
  );
  const readyPath = await waitFor(() => {
    if (!existsSync(readyDirectory)) return null;
    const entry = readdirSync(readyDirectory).find(
      (name) => /^host-ready-\d+\.json$/.test(name) && !previousReady.has(name),
    );
    return entry === undefined ? null : join(readyDirectory, entry);
  }, "desktop-entry host readiness");
  const ready = JSON.parse(readFileSync(readyPath, "utf8"));
  const shellPid = Number(basename(readyPath).match(/\d+/)?.[0]);
  const hostPid = Number(ready.pid);
  if (!Number.isSafeInteger(shellPid) || !Number.isSafeInteger(hostPid)) {
    throw new Error(`desktop-entry readiness has invalid PIDs: ${readyPath}`);
  }
  process.kill(shellPid, "SIGTERM");
  await waitFor(
    () => !processExists(shellPid) && !processExists(hostPid),
    "desktop-entry shell and host shutdown",
    15_000,
  );
  process.stdout.write(
    `${JSON.stringify(
      {
        schemaVersion: 1,
        desktopEntry: "loading-bay.desktop",
        cleanEnvironment: true,
        workingDirectory: "/tmp",
        ready: { address: ready.address, shellPid, hostPid },
        loopback: String(ready.address).startsWith("127.0.0.1:"),
        shutdownClean: true,
      },
      null,
      2,
    )}\n`,
  );
} finally {
  try {
    process.kill(-launcher.pid, "SIGTERM");
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
  if (launcher.exitCode === null) {
    await Promise.race([
      new Promise((resolveExit) => launcher.once("exit", resolveExit)),
      delay(2_000),
    ]);
  }
  try {
    process.kill(-launcher.pid, "SIGKILL");
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
}
