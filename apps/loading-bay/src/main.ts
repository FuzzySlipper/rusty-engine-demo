import { bootstrapApplication } from "@angular/platform-browser";
import {
  createProductBrowserLocalHttpAdapter,
  createProductBrowserRuntimeTransport,
  mountProductBrowserHost,
} from "@rusty-engine/product-browser-host";
import { AppComponent } from "./app.component";
import { ENGINE_APPLICATION } from "./engine-application";
import { loadRendererPreloadInitialContent } from "./renderer-preload";

const root = document.querySelector<HTMLElement>("#rusty-application");
if (root === null) {
  throw new Error("Rusty Application Host root is missing");
}
const applicationRoot = root;

void mountLoadingBayProduct().catch((error: unknown) => {
  console.error(error);
  const detail = document.createElement("pre");
  detail.id = "loading-bay-product-host-failure";
  detail.textContent =
    error instanceof Error
      ? error.message
      : `Loading Bay host failed: ${String(error)}`;
  document.body.append(detail);
});

async function mountLoadingBayProduct(): Promise<void> {
  const rendererInitialContent = await loadRendererPreloadInitialContent();
  const adapter = createProductBrowserLocalHttpAdapter({
    onTransportError: (error) =>
      console.error("Loading Bay runtime transport", error),
  });

  await mountProductBrowserHost({
    root: applicationRoot,
    transport: createProductBrowserRuntimeTransport(adapter),
    lifecycleMode: "realtime",
    initialInteractionMode: "gameplay",
    runtimeInput: { maximumPointerDelta: 32, maximumWheelDelta: 64 },
    uiProjection: {
      expectedStream: "loading-bay.hud",
      expectedContract: "loading-bay.hud.snapshot.v1",
    },
    renderer: { initialContent: rendererInitialContent },
    mountUi: async (uiRoot, context) => {
      const angularRoot = document.createElement("red-root");
      uiRoot.append(angularRoot);
      const application = await bootstrapApplication(AppComponent, {
        providers: [{ provide: ENGINE_APPLICATION, useValue: context }],
      });
      return {
        dispose: () => {
          application.destroy();
        },
      };
    },
  });

  await mountLiveDebugPanelWhenRequested();
}

async function mountLiveDebugPanelWhenRequested(): Promise<void> {
  if (globalThis.location.hash !== "#live-debug") {
    return;
  }

  const host = document.createElement("aside");
  host.id = "loading-bay-live-debug";
  document.body.append(host);
  const { mountLiveDebugPanel } = await import(
    "@rusty-engine/live-debug-panel-browser"
  );
  const panel = await mountLiveDebugPanel(host, {
    enabled: true,
    presentation: "overlay",
  });
  globalThis.addEventListener("pagehide", () => panel.dispose(), { once: true });
}
