import type { Routes } from "@angular/router";
import { MainMenuComponent } from "./main-menu.component";

export const appRoutes: Routes = [
  { path: "", component: MainMenuComponent, title: "Loading Bay" },
  {
    path: "game",
    loadComponent: () =>
      import("./game-screen.component").then(
        ({ GameScreenComponent }) => GameScreenComponent,
      ),
    title: "Loading Bay",
  },
  {
    path: "settings",
    loadComponent: () =>
      import("./settings-screen.component").then(
        ({ SettingsScreenComponent }) => SettingsScreenComponent,
      ),
    title: "Loading Bay settings",
  },
  {
    path: "diagnostics",
    loadComponent: () =>
      import("./diagnostics-screen.component").then(
        ({ DiagnosticsScreenComponent }) => DiagnosticsScreenComponent,
      ),
    title: "Loading Bay diagnostics",
  },
  { path: "**", redirectTo: "" },
];
