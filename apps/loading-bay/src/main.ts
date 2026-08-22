import { bootstrapApplication } from "@angular/platform-browser";
import { provideRouter, withHashLocation } from "@angular/router";
import { mountRustyApplication } from "@rusty-engine/application-host";
import { createLoadingBayDeveloperCommandClient } from "@rusty-engine-demo/developer-command";
import { AppComponent } from "./app.component";
import { appRoutes } from "./app.routes";
import { ENGINE_APPLICATION } from "./engine-application";

const root = document.querySelector<HTMLElement>("#rusty-application");
if (root === null) {
  throw new Error("Rusty Application Host root is missing");
}

const developerClient = createLoadingBayDeveloperCommandClient();
let applicationContext: Parameters<NonNullable<Parameters<typeof mountRustyApplication>[0]["mountUi"]>>[1] | null = null;

mountRustyApplication({
  root,
  initialInteractionMode: "interface",
  developerCommands: {
    client: developerClient,
    label: "Developer commands",
    enterInterface: () => {
      const context = applicationContext;
      if (context === null) return () => undefined;
      const previous = context.ui.interactionMode();
      context.ui.setInteractionMode("interface");
      return () => context.ui.setInteractionMode(previous);
    },
  },
  mountUi: async (uiRoot, context) => {
    applicationContext = context;
    const angularRoot = document.createElement("red-root");
    uiRoot.append(angularRoot);
    const application = await bootstrapApplication(AppComponent, {
      providers: [
        provideRouter(appRoutes, withHashLocation()),
        { provide: ENGINE_APPLICATION, useValue: context },
      ],
    });
    return {
      dispose: () => {
        applicationContext = null;
        developerClient.dispose();
        application.destroy();
      },
    };
  },
}).catch((error: unknown) => {
  console.error(error);
});
