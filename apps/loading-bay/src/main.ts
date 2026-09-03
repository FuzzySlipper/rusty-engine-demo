import { bootstrapApplication } from "@angular/platform-browser";
import { AppComponent } from "./app.component";
import {
  ENGINE_APPLICATION,
  type LoadingBayEngineApplication,
} from "./engine-application";

/** Mounted by the Engine runtime shell after it owns canvas, lifecycle, and input. */
export async function mountProductUi(
  uiRoot: HTMLElement,
  context: LoadingBayEngineApplication,
): Promise<{ readonly dispose: () => void }> {
  const angularRoot = document.createElement("red-root");
  uiRoot.append(angularRoot);
  const application = await bootstrapApplication(AppComponent, {
    providers: [{ provide: ENGINE_APPLICATION, useValue: context }],
  });
  return {
    dispose: () => {
      application.destroy();
      angularRoot.remove();
    },
  };
}
