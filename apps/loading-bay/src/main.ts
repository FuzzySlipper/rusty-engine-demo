import { bootstrapApplication } from "@angular/platform-browser";
import { provideRouter, withHashLocation } from "@angular/router";
import { mountRustyApplication } from "@rusty-engine/application-host";
import { AppComponent } from "./app.component";
import { appRoutes } from "./app.routes";
import { ENGINE_APPLICATION } from "./engine-application";

const root = document.querySelector<HTMLElement>("#rusty-application");
if (root === null) {
  throw new Error("Rusty Application Host root is missing");
}

mountRustyApplication({
  root,
  initialInteractionMode: "interface",
  mountUi: async (uiRoot, context) => {
    const angularRoot = document.createElement("red-root");
    uiRoot.append(angularRoot);
    const application = await bootstrapApplication(AppComponent, {
      providers: [
        provideRouter(appRoutes, withHashLocation()),
        { provide: ENGINE_APPLICATION, useValue: context },
      ],
    });
    return { dispose: () => application.destroy() };
  },
}).catch((error: unknown) => {
  console.error(error);
});
