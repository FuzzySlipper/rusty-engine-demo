import { bootstrapApplication } from "@angular/platform-browser";
import { provideRouter, withHashLocation } from "@angular/router";
import { AppComponent } from "./app.component";
import { appRoutes } from "./app.routes";

bootstrapApplication(AppComponent, {
  providers: [provideRouter(appRoutes, withHashLocation())],
}).catch((error: unknown) => {
  console.error(error);
});
