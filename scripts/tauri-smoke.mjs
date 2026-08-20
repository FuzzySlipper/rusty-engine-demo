import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const config = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf8"));
const adapter = readFileSync(resolve(root, "src-tauri/src/lib.rs"), "utf8");
const transport = readFileSync(
  resolve(root, "ts/packages/browser-shell/src/game-session.ts"),
  "utf8",
);

if (config.bundle.externalBin !== undefined) {
  throw new Error("Tauri package must not retain a browser-host sidecar");
}
const resources = JSON.stringify(config.bundle.resources);
if (!resources.includes("doom-e1m1.project.json") || !resources.includes("doom-e1m1/textures") || !resources.includes("doom-e1m1/sprites") || resources.includes('"../content/"')) {
  throw new Error("Tauri package must contain the E1M1 project plus texture/sprite runtime closure only");
}
if (!adapter.includes('WebviewWindowBuilder::new(app, "main"')) {
  throw new Error("Tauri adapter must create its one product WebView");
}
for (const command of [
  "loading_bay_service_begin_session",
  "loading_bay_service_disconnect_session",
  "loading_bay_service_submit",
  "loading_bay_service_readout",
  "loading_bay_service_application_resource",
]) {
  if (!adapter.includes(command)) {
    throw new Error(`Tauri adapter is missing ${command}`);
  }
}
if (!transport.includes("TauriLoadingBayGameSession")) {
  throw new Error("Angular shell does not select the Tauri IPC transport");
}
if (transport.includes("__TAURI_INTERNALS__")) {
  throw new Error("Tauri transport must use the public @tauri-apps/api surface");
}
if (transport.includes("loading_bay_service_advance")) {
  throw new Error("desktop frontend must poll readout; the Rust adapter owns simulation ticks");
}
if (!transport.includes("projection.dynamic") || !transport.includes("projection.resources")) {
  throw new Error("Tauri transport must decode nested dynamic/resources IPC projection");
}
if (!transport.includes("#sendChain")) {
  throw new Error("Tauri transport must serialize typed command dispatch");
}

process.stdout.write(
  "Tauri in-process adapter contract passed: one WebView, typed session roundtrip, projected resources, and no sidecar.\n",
);
